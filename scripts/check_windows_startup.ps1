param(
    [ValidateSet('background', 'main', 'practice')]
    [string]$ColdStart = 'background',
    [switch]$ColdOnly,
    [switch]$ExplicitDataDirectory,
    [switch]$Portable,
    [string]$Executable
)

$ErrorActionPreference = 'Stop'
$taskProjectRoot = Split-Path -Parent $PSScriptRoot
if (-not $Executable) {
    $Executable = Join-Path $taskProjectRoot 'local_artifacts/target/x86_64-pc-windows-msvc/release/try.exe'
}
$taskSourceExecutable = [IO.Path]::GetFullPath($Executable)
$taskArtifacts = Join-Path $taskProjectRoot 'local_artifacts'
$taskProfilePrefix = if ($ExplicitDataDirectory) { 'startup data_' } else { 'startup_data_' }
$taskProfile = Join-Path $taskArtifacts ($taskProfilePrefix + [guid]::NewGuid().ToString('N'))
$taskCleanupRoot = $taskProfile
if ($Portable) {
    if ($ExplicitDataDirectory) { throw 'Portable checks must use the default data location.' }
    $taskCleanupRoot = Join-Path $taskArtifacts ('startup_portable_' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $taskCleanupRoot -Force | Out-Null
    $Executable = Join-Path $taskCleanupRoot 'try.exe'
    Copy-Item -LiteralPath $taskSourceExecutable -Destination $Executable
    $taskProfile = Join-Path $taskCleanupRoot 'data'
}
$taskPreviousData = $env:TRY_DATA_DIR
$taskProcesses = [Collections.Generic.List[Diagnostics.Process]]::new()
$taskResults = [Collections.Generic.List[string]]::new()

Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class TryStartupProbe {
    [StructLayout(LayoutKind.Sequential)] struct Rect { public int left, top, right, bottom; }
    [StructLayout(LayoutKind.Sequential)] struct Point { public int x, y; }
    delegate bool EnumCallback(IntPtr window, IntPtr data);
    delegate int PropertyCallback(IntPtr window, IntPtr name, IntPtr value, IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumCallback callback, IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, EnumCallback callback, IntPtr data);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr window, out Rect rectangle);
    [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr window, out Rect rectangle);
    [DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr window, ref Point point);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern IntPtr FindWindowExW(IntPtr parent, IntPtr previous, string name, string title);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowTextW(IntPtr window, StringBuilder text, int size);
    [DllImport("user32.dll")] static extern int EnumPropsExW(IntPtr window, PropertyCallback callback, IntPtr data);
    [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr window, uint message, UIntPtr wp, IntPtr lp);
    public static IntPtr Visible(int pid) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((window, data) => {
            uint owner;
            GetWindowThreadProcessId(window, out owner);
            Rect rectangle;
            if (owner == pid && IsWindowVisible(window) && GetWindowRect(window, out rectangle)
                && rectangle.right - rectangle.left >= 64 && rectangle.bottom - rectangle.top >= 64) {
                found = window; return false;
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    public static string Caption(IntPtr window) {
        var text = new StringBuilder(2048);
        GetWindowTextW(window, text, text.Capacity);
        return text.ToString();
    }
    public static string ErrorText(IntPtr window) {
        var text = new StringBuilder(Caption(window));
        EnumChildWindows(window, (child, data) => { text.Append(" ").Append(Caption(child)); return true; }, IntPtr.Zero);
        return text.ToString();
    }
    public static bool HasFrame(IntPtr window) {
        Rect outer, client;
        var origin = new Point();
        if (!GetWindowRect(window, out outer) || !GetClientRect(window, out client)
            || !ClientToScreen(window, ref origin)) return true;
        return origin.x != outer.left || origin.y != outer.top
            || client.right - client.left != outer.right - outer.left
            || client.bottom - client.top != outer.bottom - outer.top;
    }
    public static bool TrayReady(int pid) {
        var previous = IntPtr.Zero;
        while ((previous = FindWindowExW(new IntPtr(-3), previous, "SlintSystemTrayWindow", null)) != IntPtr.Zero) {
            uint owner;
            GetWindowThreadProcessId(previous, out owner);
            if (owner != pid) continue;
            bool ready = false;
            EnumPropsExW(previous, (window, name, value, data) => {
                if (name.ToInt64() > 65535 && Marshal.PtrToStringUni(name).StartsWith("Try.Instance.")) ready = true;
                return ready ? 0 : 1;
            }, IntPtr.Zero);
            if (ready) return true;
        }
        return false;
    }
}
'@

function Start-TryTest([string]$TaskArgument) {
    $taskArguments = @{
        FilePath = $Executable
        WorkingDirectory = $taskProjectRoot
        WindowStyle = 'Hidden'
        PassThru = $true
    }
    if ($ExplicitDataDirectory) {
        $taskArguments.WorkingDirectory = [Environment]::SystemDirectory
        # Windows paths cannot contain a quote; this generated directory has
        # no trailing slash. Spaces must remain part of the same argument.
        $taskArguments.ArgumentList = "$TaskArgument --data-dir `"$taskProfile`""
    } elseif ($TaskArgument) { $taskArguments.ArgumentList = $TaskArgument }
    $taskStarted = Start-Process @taskArguments
    $taskProcesses.Add($taskStarted)
    return $taskStarted
}

function Wait-TryState([Diagnostics.Process]$TaskProcess, [string]$TaskState) {
    $taskClock = [Diagnostics.Stopwatch]::StartNew()
    do {
        $TaskProcess.Refresh()
        if ($TaskProcess.HasExited) { throw "Try exited while waiting for $TaskState (code $($TaskProcess.ExitCode))." }
        $taskWindow = [TryStartupProbe]::Visible($TaskProcess.Id)
        if ([TryStartupProbe]::Caption($taskWindow) -eq 'Try · 无法启动') {
            throw [TryStartupProbe]::ErrorText($taskWindow)
        }
        $taskReady = [TryStartupProbe]::TrayReady($TaskProcess.Id)
        if ($TaskState -eq 'tray' -and $taskReady -and $taskWindow -eq [IntPtr]::Zero) { return }
        if ($TaskState -eq 'main' -and $taskReady -and [TryStartupProbe]::Caption($taskWindow) -eq 'Try' -and [TryStartupProbe]::HasFrame($taskWindow)) { return }
        if ($TaskState -eq 'practice' -and $taskReady -and $taskWindow -ne [IntPtr]::Zero -and -not [TryStartupProbe]::HasFrame($taskWindow)) { return }
        Start-Sleep -Milliseconds 50
    } while ($taskClock.Elapsed.TotalSeconds -lt 3)
    $taskCaption = [TryStartupProbe]::Caption($taskWindow)
    throw "Try did not reach $TaskState within three seconds. Completed: $($taskResults -join ', '); tray ready: $taskReady; visible window: $taskWindow '$taskCaption'."
}

function Close-TryTestWindow([Diagnostics.Process]$TaskProcess) {
    $taskWindow = [TryStartupProbe]::Visible($TaskProcess.Id)
    if ($taskWindow -eq [IntPtr]::Zero) { throw 'The test window is missing.' }
    [TryStartupProbe]::SendMessageW($taskWindow, 0x10, [UIntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    Wait-TryState $TaskProcess 'tray'
}

try {
    $env:TRY_DATA_DIR = if ($Portable) { $null } elseif ($ExplicitDataDirectory) { Join-Path $taskProfile 'unused_env' } else { $taskProfile }
    $taskArgument = switch ($ColdStart) { 'background' { '--background' }; 'practice' { '--practice' }; default { '' } }
    $taskApp = Start-TryTest $taskArgument
    $taskExpected = if ($ColdStart -eq 'background') { 'tray' } else { $ColdStart }
    Wait-TryState $taskApp $taskExpected
    $taskResults.Add("cold_$ColdStart")
    if ($Portable) {
        if (-not (Test-Path -LiteralPath (Join-Path $taskProfile 'state.json'))) { throw 'Portable data was not created beside the executable.' }
        if (Test-Path -LiteralPath (Join-Path $taskCleanupRoot 'portable.flag')) { throw 'Single-file portable startup must not require a flag.' }
        if (@(Get-ChildItem -LiteralPath $taskCleanupRoot).Count -ne 2) { throw 'Unexpected portable runtime sidecar.' }
        $taskResults.Add('single_executable_creates_adjacent_data_without_flag')
    }
    if ($ExplicitDataDirectory) {
        if (-not (Test-Path -LiteralPath (Join-Path $taskProfile 'state.json'))) { throw 'The explicit data directory was not used.' }
        if (Test-Path -LiteralPath (Join-Path $taskProfile 'unused_env')) { throw 'The environment overrode --data-dir.' }
        $taskResults.Add('explicit_data_overrides_environment_and_working_directory')
    }
    if (-not $ColdOnly) {
        if ($ColdStart -ne 'background') { Close-TryTestWindow $taskApp }
        foreach ($taskMode in @('main', 'practice')) {
            $taskArgument = if ($taskMode -eq 'practice') { '--practice' } else { '' }
            $taskLauncher = Start-TryTest $taskArgument
            if (-not $taskLauncher.WaitForExit(3000)) { throw 'The second launcher did not forward to the first instance.' }
            if ($taskLauncher.ExitCode -ne 0) { throw "The second launcher failed (code $($taskLauncher.ExitCode))." }
            Wait-TryState $taskApp $taskMode
            $taskResults.Add("forward_$taskMode")
            Close-TryTestWindow $taskApp
            $taskResults.Add("close_${taskMode}_to_tray")
        }
    }
} finally {
    foreach ($taskProcess in $taskProcesses) {
        $taskProcess.Refresh()
        if (-not $taskProcess.HasExited) {
            Stop-Process -InputObject $taskProcess
            if (-not $taskProcess.WaitForExit(3000)) { throw 'The test process did not stop.' }
        }
        $taskProcess.Dispose()
    }
    $env:TRY_DATA_DIR = $taskPreviousData
    $taskResolved = [IO.Path]::GetFullPath($taskCleanupRoot)
    $taskAllowed = [IO.Path]::GetFullPath($taskArtifacts).TrimEnd('\') + '\'
    if (-not $taskResolved.StartsWith($taskAllowed, [StringComparison]::OrdinalIgnoreCase)) { throw 'Test profile is outside local_artifacts.' }
    if (Test-Path -LiteralPath $taskResolved) { Remove-Item -LiteralPath $taskResolved -Recurse -Force }
}

$taskReport = [pscustomobject]@{
    checked_at = [DateTimeOffset]::Now.ToString('o')
    executable = $taskSourceExecutable
    executable_sha256 = (Get-FileHash -LiteralPath $taskSourceExecutable).Hash
    explicit_data_directory = [bool]$ExplicitDataDirectory
    portable = [bool]$Portable
    passed = $taskResults.ToArray()
}
$taskJson = $taskReport | ConvertTo-Json -Depth 4
$taskReportName = if ($Portable) { "startup_${ColdStart}_portable.json" } elseif ($ExplicitDataDirectory) { "startup_${ColdStart}_explicit_data.json" } else { "startup_${ColdStart}.json" }
[IO.File]::WriteAllText((Join-Path $taskArtifacts $taskReportName), $taskJson, [Text.UTF8Encoding]::new($false))
$taskJson
