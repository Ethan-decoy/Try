$ErrorActionPreference = 'Stop'
$taskGit = (Get-Command git -ErrorAction Stop).Source
$taskGitRoot = Split-Path -Parent (Split-Path -Parent $taskGit)
$taskShell = Join-Path $taskGitRoot 'bin/sh.exe'
if (-not (Test-Path -LiteralPath $taskShell)) {
    throw '未找到 Git for Windows 的 sh.exe，请通过 Git Bash 运行 scripts/setup_git.sh。'
}
Push-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)
try {
    & $taskShell (Join-Path $PSScriptRoot 'setup_git.sh')
    if ($LASTEXITCODE -ne 0) { throw 'Git 配置失败。' }
} finally {
    Pop-Location
}
