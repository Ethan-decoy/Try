param(
    [switch]$DesktopShortcut,
    [string]$BuildDirectory = 'local_artifacts/target/x86_64-pc-windows-msvc'
)

$ErrorActionPreference = 'Stop'
$taskProjectRoot = Split-Path -Parent $PSScriptRoot
$taskPackageRoot = Join-Path $taskProjectRoot 'local_artifacts/windows_package'
$taskExecutable = Join-Path $taskPackageRoot 'try.exe'
$taskInfoPath = Join-Path $taskPackageRoot 'build_info.json'
$taskInfo = Get-Content -LiteralPath $taskInfoPath -Raw | ConvertFrom-Json
if (-not (Test-Path -LiteralPath $taskExecutable)) { throw 'The Try package is missing.' }
if ($taskInfo.icon_sha256 -notmatch '^[a-fA-F0-9]{64}$') { throw 'The package icon hash is missing or invalid.' }
$taskIconName = 'try_icon_' + $taskInfo.icon_sha256.Substring(0, 16).ToLowerInvariant() + '.ico'
$taskIconPath = Join-Path $taskPackageRoot $taskIconName
$taskSourceIcon = Join-Path $taskPackageRoot 'try.ico'
if (-not (Test-Path -LiteralPath $taskSourceIcon)) { $taskSourceIcon = $taskIconPath }
if ((Get-FileHash -LiteralPath $taskSourceIcon).Hash -ne $taskInfo.icon_sha256) { throw 'The icon does not match the package build information.' }

$taskShell = New-Object -ComObject WScript.Shell
$taskShortcutPaths = @((Join-Path $taskPackageRoot 'try.lnk'))
$taskDesktopPath = Join-Path ([Environment]::GetFolderPath('Desktop')) 'try.lnk'
$taskRefreshDesktop = $DesktopShortcut.IsPresent
if (Test-Path -LiteralPath $taskDesktopPath) {
    $taskExistingDesktop = $taskShell.CreateShortcut($taskDesktopPath)
    if ($taskExistingDesktop.TargetPath -eq $taskExecutable) { $taskRefreshDesktop = $true }
    elseif ($DesktopShortcut) { throw 'Preserve the existing unrelated desktop shortcut.' }
}
if ($taskRefreshDesktop) { $taskShortcutPaths += $taskDesktopPath }
foreach ($taskShortcutPath in $taskShortcutPaths) {
    if ((Test-Path -LiteralPath $taskShortcutPath) -and $taskShell.CreateShortcut($taskShortcutPath).TargetPath -ne $taskExecutable) {
        throw "Preserve the existing unrelated shortcut: $taskShortcutPath"
    }
}

if ($taskSourceIcon -ne $taskIconPath) { Copy-Item -LiteralPath $taskSourceIcon -Destination $taskIconPath -Force }
$taskInfo | Add-Member -NotePropertyName shortcut_icon -NotePropertyValue $taskIconName -Force
$taskInfo | Add-Member -NotePropertyName shortcut_icon_updated_at -NotePropertyValue ([DateTimeOffset]::Now.ToString('o')) -Force
[IO.File]::WriteAllText($taskInfoPath, ($taskInfo | ConvertTo-Json) + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
foreach ($taskShortcutPath in $taskShortcutPaths) {
    $taskShortcut = $taskShell.CreateShortcut($taskShortcutPath)
    $taskShortcut.TargetPath = $taskExecutable
    $taskShortcut.WorkingDirectory = $taskPackageRoot
    $taskShortcut.IconLocation = $taskIconPath + ',0'
    $taskShortcut.Description = 'Try · 随手一练'
    $taskShortcut.Save()
}

if (-not ('TryShortcutRefresh' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class TryShortcutRefresh {
    [DllImport("shell32.dll", CharSet=CharSet.Unicode)] private static extern void SHChangeNotify(uint change, uint flags, string item, IntPtr other);
    public static void Item(string path) { SHChangeNotify(0x2000, 0x1005, path, IntPtr.Zero); }
    public static void Directory(string path) { SHChangeNotify(0x1000, 0x1005, path, IntPtr.Zero); }
}
'@
}
[TryShortcutRefresh]::Item($taskIconPath)
foreach ($taskShortcutPath in $taskShortcutPaths) { [TryShortcutRefresh]::Item($taskShortcutPath) }
if ($taskRefreshDesktop) { [TryShortcutRefresh]::Directory((Split-Path -Parent $taskDesktopPath)) }

# Validate the links and their cached Shell images before removing obsolete assets.
& (Join-Path $PSScriptRoot 'check_windows_package.ps1') -DesktopShortcut:$taskRefreshDesktop -BuildDirectory $BuildDirectory | Out-Null
$taskResolvedPackage = (Resolve-Path -LiteralPath $taskPackageRoot).Path.TrimEnd('\')
$taskObsoleteIcons = Get-ChildItem -LiteralPath $taskPackageRoot -File | Where-Object {
    ($_.Name -eq 'try.ico' -or $_.Name -match '^try_icon_[a-f0-9]{16}\.ico$') -and $_.Name -ne $taskIconName
}
foreach ($taskObsolete in $taskObsoleteIcons) {
    if ([IO.Path]::GetDirectoryName($taskObsolete.FullName) -ne $taskResolvedPackage) { throw 'Refuse to remove an icon outside the Try package.' }
    Remove-Item -LiteralPath $taskObsolete.FullName
}
[IO.Directory]::SetLastWriteTime($taskPackageRoot, [DateTime]::Now)
Write-Output $taskIconPath
