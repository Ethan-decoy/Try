param(
    [switch]$Background,
    [switch]$LocalProfile,
    [switch]$Practice
)

$ErrorActionPreference = 'Stop'
$taskProjectRoot = Split-Path -Parent $PSScriptRoot
$taskExecutable = Join-Path $taskProjectRoot 'local_artifacts/target/x86_64-pc-windows-msvc/release/try.exe'
if (-not (Test-Path -LiteralPath $taskExecutable)) {
    throw 'Build Try first: ./scripts/build_windows.ps1'
}
if ($LocalProfile) {
    $env:TRY_DATA_DIR = Join-Path $taskProjectRoot 'local_artifacts/run_data'
}
$taskArguments = @()
if ($Background) { $taskArguments += '--background' }
if ($Practice) { $taskArguments += '--practice' }
if ($Background -and -not $Practice) {
    Start-Process -FilePath $taskExecutable -ArgumentList $taskArguments -WindowStyle Hidden
} else {
    if ($taskArguments.Count) {
        Start-Process -FilePath $taskExecutable -ArgumentList $taskArguments -WindowStyle Normal
    } else {
        Start-Process -FilePath $taskExecutable -WindowStyle Normal
    }
}
