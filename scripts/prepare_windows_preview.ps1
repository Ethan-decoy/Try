# Prepare a local preview; formal releases use prepare_windows_release.ps1.
param(
    [switch]$DesktopShortcut,
    [string]$SeedDataDirectory,
    [string]$BuildDirectory = 'local_artifacts/target/x86_64-pc-windows-msvc'
)

$ErrorActionPreference = 'Stop'
$taskProjectRoot = Split-Path -Parent $PSScriptRoot
$taskBuildRoot = if ([IO.Path]::IsPathRooted($BuildDirectory)) { $BuildDirectory } else { Join-Path $taskProjectRoot $BuildDirectory }
$taskSourceExecutable = Join-Path $taskBuildRoot 'release/try.exe'
# Retain the existing preview location so its data and shortcuts keep working.
$taskPackageRoot = Join-Path $taskProjectRoot 'local_artifacts/windows_package'
$taskPackageExecutable = Join-Path $taskPackageRoot 'try.exe'
$taskPackageData = Join-Path $taskPackageRoot 'data'
if (-not (Test-Path -LiteralPath $taskSourceExecutable)) {
    throw 'Build Try first: ./scripts/build_windows.ps1'
}
$taskRunning = Get-CimInstance Win32_Process -Filter "Name = 'try.exe'" |
    Where-Object { $_.ExecutablePath -eq $taskPackageExecutable }
if ($taskRunning) { throw 'Exit the packaged Try from its tray menu before updating it.' }
$taskIcon = Get-ChildItem -LiteralPath (Join-Path $taskBuildRoot 'release/build') -Filter try.ico -Recurse |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $taskIcon) { throw 'Build the latest Try to generate its desktop icon.' }
New-Item -ItemType Directory -Path $taskPackageRoot, $taskPackageData -Force | Out-Null
Copy-Item -LiteralPath $taskSourceExecutable -Destination $taskPackageExecutable -Force
Copy-Item -LiteralPath $taskIcon.FullName -Destination (Join-Path $taskPackageRoot 'try.ico') -Force
[IO.File]::WriteAllText((Join-Path $taskPackageRoot 'portable.flag'), '')
$taskStatePath = Join-Path $taskPackageData 'state.json'
if ($SeedDataDirectory -and -not (Test-Path -LiteralPath $taskStatePath)) {
    Copy-Item -LiteralPath (Join-Path $SeedDataDirectory 'state.json') -Destination $taskStatePath
}
$taskVersion = (Get-Item -LiteralPath $taskPackageExecutable).VersionInfo.ProductVersion
if (-not $taskVersion) { throw 'Build the latest Try with Windows icon and version resources first.' }
$taskBuiltAt = (Get-Item -LiteralPath $taskSourceExecutable).LastWriteTime.ToString('o')
$taskPackagedAt = [DateTimeOffset]::Now
$taskBuildInfo = [ordered]@{
    version = $taskVersion
    built_at = $taskBuiltAt
    packaged_at = $taskPackagedAt.ToString('o')
    entry_point = 'try.exe'
    executable_sha256 = (Get-FileHash -LiteralPath $taskPackageExecutable).Hash
    icon_sha256 = (Get-FileHash -LiteralPath (Join-Path $taskPackageRoot 'try.ico')).Hash
}
[IO.File]::WriteAllText((Join-Path $taskPackageRoot 'build_info.json'), ($taskBuildInfo | ConvertTo-Json) + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
$taskInstructions = @"
Try · 本地开发预览 / Local development preview

版本 / Version: $taskVersion
构建时间 / Built: $taskBuiltAt
打包时间 / Prepared: $($taskPackagedAt.ToString('o'))

双击 try.exe 打开。设置、知识包与学习记录保存在旁边的 data 目录，移动此预览目录时一并保留。
Double-click try.exe to open. Keep the adjacent data directory when moving this preview.

本地使用说明 / Local guides:
$taskProjectRoot/docs/user_guide.md
$taskProjectRoot/docs/user_guide_en.md

在线使用说明 / Online guides:
https://github.com/Ethan-decoy/Try/blob/main/docs/user_guide.md
https://github.com/Ethan-decoy/Try/blob/main/docs/user_guide_en.md

这是用于本地体验和快捷方式检查的开发目录。正式下载见 / Official downloads:
https://github.com/Ethan-decoy/Try/releases
"@
[IO.File]::WriteAllText((Join-Path $taskPackageRoot 'read_me.txt'), $taskInstructions, [Text.UTF8Encoding]::new($false))
Copy-Item -LiteralPath (Join-Path $taskProjectRoot 'docs/third_party_notices.md') -Destination (Join-Path $taskPackageRoot 'third_party_notices.md') -Force
Copy-Item -LiteralPath (Join-Path $taskProjectRoot 'third_party_licenses') -Destination $taskPackageRoot -Recurse -Force
Copy-Item -LiteralPath (Join-Path $taskProjectRoot 'license') -Destination $taskPackageRoot -Force
& (Join-Path $PSScriptRoot 'refresh_windows_shortcuts.ps1') -DesktopShortcut:$DesktopShortcut -BuildDirectory $BuildDirectory | Out-Null
[IO.Directory]::SetLastWriteTime($taskPackageRoot, $taskPackagedAt.LocalDateTime)
Write-Output $taskPackageExecutable
