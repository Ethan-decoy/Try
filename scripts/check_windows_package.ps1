param(
    [switch]$DesktopShortcut,
    [string]$BuildDirectory = 'local_artifacts/target/x86_64-pc-windows-msvc'
)

$ErrorActionPreference = 'Stop'
$taskProjectRoot = Split-Path -Parent $PSScriptRoot
$taskPackageRoot = Join-Path $taskProjectRoot 'local_artifacts/windows_package'
$taskExecutable = Join-Path $taskPackageRoot 'try.exe'
$taskBuildRoot = if ([IO.Path]::IsPathRooted($BuildDirectory)) { $BuildDirectory } else { Join-Path $taskProjectRoot $BuildDirectory }
$taskSource = Join-Path $taskBuildRoot 'release/try.exe'
if (-not (Test-Path -LiteralPath $taskExecutable)) { throw 'The Try package is missing.' }

if (-not ('TryPackageIcon' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class TryPackageIcon {
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] private struct FileInfo {
        public IntPtr Icon;
        public int Index;
        public uint Attributes;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=260)] public string DisplayName;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=80)] public string TypeName;
    }
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode)] private static extern IntPtr LoadLibraryExW(string path, IntPtr file, uint flags);
    [DllImport("kernel32.dll")] private static extern IntPtr FindResourceW(IntPtr module, IntPtr name, IntPtr type);
    [DllImport("kernel32.dll")] private static extern bool FreeLibrary(IntPtr module);
    [DllImport("shell32.dll", CharSet=CharSet.Unicode)] private static extern uint ExtractIconExW(string path, int index, [Out] IntPtr[] large, [Out] IntPtr[] small, uint count);
    [DllImport("user32.dll")] public static extern bool DestroyIcon(IntPtr icon);
    [DllImport("shell32.dll", CharSet=CharSet.Unicode)] private static extern IntPtr SHGetFileInfoW(string path, uint attributes, out FileInfo info, uint size, uint flags);
    [DllImport("comctl32.dll")] private static extern IntPtr ImageList_GetIcon(IntPtr list, int index, uint flags);
    public static bool HasLogo(string path) {
        var module = LoadLibraryExW(path, IntPtr.Zero, 0x22);
        if (module == IntPtr.Zero) throw new Exception("Cannot read the Try resources.");
        try { return FindResourceW(module, new IntPtr(1), new IntPtr(14)) != IntPtr.Zero; }
        finally { FreeLibrary(module); }
    }
    public static IntPtr[] Extract(string path) {
        var large = new IntPtr[1];
        var small = new IntPtr[1];
        if (ExtractIconExW(path, 0, large, small, 1) == 0) throw new Exception("Cannot extract the Try icon.");
        return new[] { large[0], small[0] };
    }
    public static IntPtr ShellIcon(string path) {
        FileInfo info;
        var list = SHGetFileInfoW(path, 0, out info, (uint)Marshal.SizeOf(typeof(FileInfo)), 0x4000);
        if (list == IntPtr.Zero) throw new Exception("Cannot read the shortcut from the Shell icon cache.");
        var icon = ImageList_GetIcon(list, info.Index, 0);
        if (icon == IntPtr.Zero) throw new Exception("Cannot extract the cached shortcut icon.");
        return icon;
    }
}
'@
}
if (-not [TryPackageIcon]::HasLogo($taskExecutable)) { throw 'try.exe has no embedded Try Logo; Explorer will use its default program icon.' }

Add-Type -AssemblyName System.Drawing
$taskSvg = [xml](Get-Content -LiteralPath (Join-Path $taskProjectRoot 'assets/app_icon.svg') -Raw)
$taskBadgeColor = [Drawing.ColorTranslator]::FromHtml($taskSvg.svg.rect.fill)
function Measure-TryIconBitmap($Bitmap, [string]$Context) {
    $taskOpaquePixels = 0
    $taskFillPixels = 0
    $taskBadgePixels = 0
    for ($taskY = 0; $taskY -lt $Bitmap.Height; $taskY++) {
        for ($taskX = 0; $taskX -lt $Bitmap.Width; $taskX++) {
            $taskPixel = $Bitmap.GetPixel($taskX, $taskY)
            if ($taskPixel.A -eq 255) {
                $taskOpaquePixels++
                if ($taskPixel.R -ge 224 -and $taskPixel.G -ge 224 -and $taskPixel.B -ge 224) { $taskFillPixels++ }
                if ([Math]::Abs($taskPixel.R - $taskBadgeColor.R) -le 4 -and
                    [Math]::Abs($taskPixel.G - $taskBadgeColor.G) -le 4 -and
                    [Math]::Abs($taskPixel.B - $taskBadgeColor.B) -le 4) { $taskBadgePixels++ }
            }
        }
    }
    foreach ($taskCorner in @(@(0,0), @(($Bitmap.Width - 1),0), @(0,($Bitmap.Height - 1)), @(($Bitmap.Width - 1),($Bitmap.Height - 1)))) {
        if ($Bitmap.GetPixel($taskCorner[0], $taskCorner[1]).A -ne 0) { throw "The icon needs transparent pixels outside its rounded corners: $Context" }
    }
    # The dark face covers a broad area even at the smallest Shell icon size.
    if ($taskFillPixels -eq 0 -or $taskBadgePixels -lt ($Bitmap.Width * $Bitmap.Height / 4.0)) {
        throw "The icon needs both its white mark and dark badge: $Context"
    }
    return [pscustomobject]@{ opaque_pixels=$taskOpaquePixels; fill_pixels=$taskFillPixels; badge_pixels=$taskBadgePixels }
}
$taskHandles = [TryPackageIcon]::Extract($taskExecutable)
$taskOpaque = 0
try {
    $taskIcon = [Drawing.Icon]::FromHandle($taskHandles[0])
    $taskBitmap = $taskIcon.ToBitmap()
    try {
        $taskIconPixels = Measure-TryIconBitmap $taskBitmap $taskExecutable
        $taskOpaque = $taskIconPixels.opaque_pixels
    } finally { $taskBitmap.Dispose(); $taskIcon.Dispose() }
} finally {
    foreach ($taskHandle in $taskHandles) { if ($taskHandle -ne [IntPtr]::Zero) { [TryPackageIcon]::DestroyIcon($taskHandle) | Out-Null } }
}
if ($taskOpaque -eq 0) { throw 'The executable icon is empty.' }

$taskHash = (Get-FileHash -LiteralPath $taskExecutable).Hash
if ($taskHash -ne (Get-FileHash -LiteralPath $taskSource).Hash) { throw 'The portable executable differs from the selected build output.' }
$taskInfo = Get-Content -LiteralPath (Join-Path $taskPackageRoot 'build_info.json') -Raw | ConvertFrom-Json
if ($taskInfo.executable_sha256 -ne $taskHash) { throw 'The package build information is stale.' }
if ($taskInfo.icon_sha256 -notmatch '^[a-fA-F0-9]{64}$') { throw 'The package icon hash is missing or invalid.' }
$taskIconName = 'try_icon_' + $taskInfo.icon_sha256.Substring(0, 16).ToLowerInvariant() + '.ico'
if ($taskInfo.shortcut_icon -ne $taskIconName) { throw 'The shortcuts still reuse a fixed icon path; refresh them with ./scripts/refresh_windows_shortcuts.ps1 -DesktopShortcut.' }
$taskShortcutIcon = Join-Path $taskPackageRoot $taskIconName
if ($taskInfo.icon_sha256 -ne (Get-FileHash -LiteralPath $taskShortcutIcon).Hash) { throw 'The package icon information is stale.' }
if ($taskInfo.version -ne (Get-Item -LiteralPath $taskExecutable).VersionInfo.ProductVersion) { throw 'The executable version does not match its build information.' }
$taskShell = New-Object -ComObject WScript.Shell
$taskShortcutPaths = @((Join-Path $taskPackageRoot 'try.lnk'))
if ($DesktopShortcut) {
    $taskShortcutPaths += Join-Path ([Environment]::GetFolderPath('Desktop')) 'try.lnk'
}
$taskCachedIcons = @()
foreach ($taskShortcutPath in $taskShortcutPaths) {
    if (-not (Test-Path -LiteralPath $taskShortcutPath)) { throw "The shortcut is missing: $taskShortcutPath" }
    $taskShortcut = $taskShell.CreateShortcut($taskShortcutPath)
    if ($taskShortcut.TargetPath -ne $taskExecutable) { throw "The shortcut points to a different executable: $taskShortcutPath" }
    if ($taskShortcut.IconLocation -ne ($taskShortcutIcon + ',0')) { throw "The shortcut points to an outdated icon: $taskShortcutPath" }
    $taskHandle = [TryPackageIcon]::ShellIcon($taskShortcutPath)
    try {
        $taskIcon = [Drawing.Icon]::FromHandle($taskHandle)
        $taskBitmap = $taskIcon.ToBitmap()
        try {
            $taskCachedPixels = Measure-TryIconBitmap $taskBitmap $taskShortcutPath
            $taskCachedOpaque = $taskCachedPixels.opaque_pixels
            if ($taskCachedOpaque -eq 0) { throw "The cached shortcut icon is empty: $taskShortcutPath" }
            $taskCachedIcons += [pscustomobject]@{ shortcut = $taskShortcutPath; size = $taskBitmap.Width; opaque_pixels = $taskCachedOpaque; fill_pixels = $taskCachedPixels.fill_pixels; badge_pixels = $taskCachedPixels.badge_pixels }
        } finally { $taskBitmap.Dispose(); $taskIcon.Dispose() }
    } finally { [TryPackageIcon]::DestroyIcon($taskHandle) | Out-Null }
}
[pscustomobject]@{ executable = $taskExecutable; version = $taskInfo.version; built_at = $taskInfo.built_at; packaged_at = $taskInfo.packaged_at; shortcut_icon_updated_at = $taskInfo.shortcut_icon_updated_at; logo_color = $taskSvg.svg.g.path.fill; logo_outline_color = $taskSvg.svg.g.path.stroke; icon_badge_color = $taskSvg.svg.rect.fill; icon_opaque_pixels = $taskOpaque; icon_fill_pixels = $taskIconPixels.fill_pixels; icon_badge_pixels = $taskIconPixels.badge_pixels; cached_shortcut_icons = $taskCachedIcons; sha256 = $taskHash } | ConvertTo-Json -Depth 4
