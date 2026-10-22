param(
    [ValidateSet('build', 'test', 'clippy', 'fmt')]
    [string]$Action = 'build',
    [switch]$Offline,
    [switch]$Distribution,
    [string]$NoticesDirectory = 'local_artifacts/open_source/distribution_notices',
    [switch]$DocTests,
    [ValidateSet('update_driver')]
    [string]$Example,
    [switch]$NativeChecks,
    [ValidateSet('all', 'native_picture_questions', 'native_structured_math', 'native_automatic_updates', 'native_practice_focus', 'native_pack_tags', 'native_desktop_theme', 'native_adaptive_practice', 'native_practice_text_layout', 'native_starter_packs', 'native_question_focus', 'native_question_deletion', 'native_lifecycle', 'native_frameless_repaint', 'native_no_frame_baseline', 'native_drag_repaint', 'native_answer_modes', 'native_question_management', 'native_multi_pack_participation', 'native_pack_deletion', 'native_math_authoring', 'native_formula_answers', 'native_formula_editing', 'native_inline_answers', 'native_inline_script_typing', 'native_settings', 'native_quick_answers', 'native_main_repaint', 'native_repaint_baseline', 'native_localization', 'native_update_check', 'native_update_edit_guard', 'native_update_latest_target', 'native_tray_menu', 'native_licenses')]
    [string]$NativeTest = 'native_lifecycle'
)

$ErrorActionPreference = 'Stop'
$taskProjectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $taskProjectRoot

$taskCargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
$taskCargo = if ($taskCargoCommand) { $taskCargoCommand.Source } else { $null }
if ($env:RUSTUP_HOME) {
    $taskToolchain = if ($env:RUSTUP_TOOLCHAIN) { $env:RUSTUP_TOOLCHAIN } else { 'stable' }
    if ($taskToolchain -notlike '*-pc-windows-msvc') { $taskToolchain += '-x86_64-pc-windows-msvc' }
    $taskRustBin = Join-Path $env:RUSTUP_HOME "toolchains/$taskToolchain/bin"
    if (Test-Path -LiteralPath (Join-Path $taskRustBin 'cargo.exe')) {
        $taskCargo = Join-Path $taskRustBin 'cargo.exe'
    }
}
if (-not $taskCargo -and $env:CARGO_HOME) {
    $taskCandidate = Join-Path $env:CARGO_HOME 'bin/cargo.exe'
    if (Test-Path -LiteralPath $taskCandidate) { $taskCargo = $taskCandidate }
}
if (-not $taskCargo) { throw 'Rust/Cargo was not found. Install Rust or add Cargo to PATH.' }
$env:PATH = (Split-Path -Parent $taskCargo) + ';' + $env:PATH

# Choose a complete MSVC installation; a newer partial install may lack the CRT.
$taskVswhere = Join-Path ([Environment]::GetFolderPath('ProgramFilesX86')) 'Microsoft Visual Studio/Installer/vswhere.exe'
if (Test-Path -LiteralPath $taskVswhere) {
    $taskVsRoot = & $taskVswhere -products '*' -latest -property installationPath
    if ($taskVsRoot) {
        $taskMsvcRoot = Join-Path $taskVsRoot 'VC/Tools/MSVC'
        $taskMsvc = Get-ChildItem -LiteralPath $taskMsvcRoot -Directory |
            Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'lib/x64/msvcrt.lib') } |
            Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
        if ($taskMsvc) {
            $taskMsvcBin = Join-Path $taskMsvc.FullName 'bin/Hostx64/x64'
            $env:PATH = $taskMsvcBin + ';' + $env:PATH
            $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = Join-Path $taskMsvcBin 'link.exe'
            $env:LIB = Join-Path $taskMsvc.FullName 'lib/x64'
            $env:INCLUDE = Join-Path $taskMsvc.FullName 'include'
        }
    }
}
$taskSdkRoot = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots' -ErrorAction SilentlyContinue).KitsRoot10
if ($taskSdkRoot -and (Test-Path -LiteralPath (Join-Path $taskSdkRoot 'Lib'))) {
    $taskSdk = Get-ChildItem -LiteralPath (Join-Path $taskSdkRoot 'Lib') -Directory |
        Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'um/x64/kernel32.lib') } |
        Sort-Object Name -Descending | Select-Object -First 1
    if ($taskSdk) {
        $env:LIB += ';' + (Join-Path $taskSdk.FullName 'ucrt/x64') + ';' + (Join-Path $taskSdk.FullName 'um/x64')
        $taskInclude = Join-Path $taskSdkRoot ('Include/' + $taskSdk.Name)
        $env:INCLUDE += ';' + (Join-Path $taskInclude 'ucrt') + ';' + (Join-Path $taskInclude 'um') + ';' + (Join-Path $taskInclude 'shared')
        $taskResourceCompiler = Join-Path $taskSdkRoot ('bin/' + $taskSdk.Name + '/x64/rc.exe')
        if (Test-Path -LiteralPath $taskResourceCompiler) { $env:TRY_RESOURCE_COMPILER = $taskResourceCompiler }
    }
}

if ($env:GITHUB_ACTIONS -ne 'true' -or -not $env:CARGO_HOME) {
    $env:CARGO_HOME = Join-Path $taskProjectRoot 'local_artifacts/cargo_home'
}
$env:TEMP = Join-Path $taskProjectRoot 'local_artifacts/tmp'
$env:TMP = $env:TEMP
New-Item -ItemType Directory -Path $env:TEMP -Force | Out-Null

if ($Distribution) {
    if ($Action -eq 'fmt') { throw 'Distribution requires build, test or clippy.' }
    & (Join-Path $PSScriptRoot 'prepare_embedded_notices.ps1') -NoticesDirectory $NoticesDirectory | Out-Null
}

$taskArguments = @($Action)
if ($Action -ne 'fmt') {
    # An explicit target keeps host proc-macros separate from the static CRT build.
    $taskArguments += '--release', '--locked', '--target', 'x86_64-pc-windows-msvc'
    $taskArguments += '--config', 'target.x86_64-pc-windows-msvc.rustflags=["-C","target-feature=+crt-static"]'
    if ($Distribution) { $taskArguments += '--features', 'portable,distribution' }
    if ($Offline) { $taskArguments += '--offline' }
}
if ($NativeChecks) {
    if ($Action -ne 'test') { throw 'NativeChecks requires -Action test.' }
    $taskArguments += '--features', 'native_checks'
    if ($NativeTest -ne 'all') {
        $taskArguments += '--bin', 'try', $NativeTest, '--', '--ignored', '--test-threads=1', '--nocapture'
    }
}
if ($Example) {
    if ($Action -ne 'build' -or -not $Distribution) { throw 'Update examples require a distribution build.' }
    $taskArguments += '--example', $Example, '--features', 'native_checks'
}
if ($DocTests) {
    if ($Action -ne 'test' -or $NativeChecks) { throw 'DocTests requires -Action test without NativeChecks.' }
    $taskArguments += '--doc'
}
if ($Action -eq 'clippy') {
    $taskArguments += '--all-targets', '--features', 'native_checks', '--', '-D', 'warnings'
}
& $taskCargo @taskArguments
exit $LASTEXITCODE
