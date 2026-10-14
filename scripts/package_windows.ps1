param(
    [switch]$DesktopShortcut,
    [string]$SeedDataDirectory,
    [string]$BuildDirectory = 'local_artifacts/target/x86_64-pc-windows-msvc'
)

$ErrorActionPreference = 'Stop'
$taskProjectRoot = Split-Path -Parent $PSScriptRoot
$taskBuildRoot = if ([IO.Path]::IsPathRooted($BuildDirectory)) { $BuildDirectory } else { Join-Path $taskProjectRoot $BuildDirectory }
$taskSourceExecutable = Join-Path $taskBuildRoot 'release/try.exe'
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
Try

版本：$taskVersion
构建时间：$taskBuiltAt
打包时间：$($taskPackagedAt.ToString('o'))

双击 try.exe 打开主窗口。程序已在运行时，再次双击会打开已有实例。
回到托盘后，左键图标开始练习，右键进入设置、关于、暂离、暂停／恢复或退出。
「暂停」持续停止自动出题并保留托盘图标；再次打开不会解除，点击「恢复」后重新计时。
侧栏点击包名浏览，独立开关启用多个知识包。关闭再开启保留题目范围，各包答案和逐题题型分别保存。
答题根据有效对错和经过时间调整选题机会，仍遵循设置的提醒间隔；直接显示答案不更新个人练习记录。
ƒ 展开原生公式面板。← / → 切换位置，↑ 选外层，↗ 在结构后输入。
C 清空当前字段；Ctrl+Z 撤销，包含未完成的公式草稿。
托盘答案框会在输入停顿后直接排版完整公式，支持点击、拖选与方向键；复制保留输入原文。

题库与设置保存在旁边 data 文件夹的 state.json，个人练习状态单独保存在 learning_state.json。移动程序时，请保留整个文件夹。
更新程序前先从托盘退出；保留 data 文件夹即可继续使用现有题库与设置。

设置页「外观」选择「浅色／深色／跟随系统」，默认跟随系统；主窗口、标题栏与练习卡片同步变化并保存选择。
展开知识包标题可以编辑标签，留空不显示。设置页「显示知识包标签」统一控制显示；拖动卡片后标签自动朝屏幕内侧换边。
在设置页底部的「语言」选择「跟随系统／简体中文／English」，立即切换并保存。
Try 自有代码：MIT；Slint：Royalty-free 2.0。第三方组件保留各自许可。

Double-click try.exe to open Try. Closing the main window returns to the tray.
Left-click the tray icon to practice; right-click for Settings, About, timed breaks, Pause/Resume or Quit.
Pause keeps the tray icon and persists through restarting. Choose Resume to restart the full interval.
Browse packs by name and enable several with their sidebar switches. Turning a pack off preserves its selected cards and answer modes.
Answering adjusts selection using valid results and elapsed time within your reminder interval. Fact viewing does not update personal practice history.
Settings > Appearance: Light / Dark / System (default). The main window, title bar and practice card follow your saved choice.
Expand a pack title to edit its tag; leave blank to hide. Settings > Show pack tags controls visibility. Tags switch inward as cards move across the screen.
Settings > Language: System / 简体中文 / English. Your packs keep their content.
The formula pad follows the caret and selection. Use the arrows to navigate; C clears and Ctrl+Z restores your draft.
The tray answer box formats complete formulas after a short pause. Click, drag or use the arrows to edit; copying keeps the original source.
Keep the data folder when moving or updating: state.json contains settings and packs; learning_state.json contains personal practice state. Exit Try before replacing the executable.
"@
[IO.File]::WriteAllText((Join-Path $taskPackageRoot 'read_me.txt'), $taskInstructions, [Text.UTF8Encoding]::new($false))
Copy-Item -LiteralPath (Join-Path $taskProjectRoot 'docs/third_party_notices.md') -Destination (Join-Path $taskPackageRoot 'third_party_notices.md') -Force
Copy-Item -LiteralPath (Join-Path $taskProjectRoot 'third_party_licenses') -Destination $taskPackageRoot -Recurse -Force
Copy-Item -LiteralPath (Join-Path $taskProjectRoot 'license') -Destination $taskPackageRoot -Force
& (Join-Path $PSScriptRoot 'refresh_windows_shortcuts.ps1') -DesktopShortcut:$DesktopShortcut -BuildDirectory $BuildDirectory | Out-Null
[IO.Directory]::SetLastWriteTime($taskPackageRoot, $taskPackagedAt.LocalDateTime)
Write-Output $taskPackageExecutable
