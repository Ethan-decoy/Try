param(
    [string]$BuildDirectory = 'local_artifacts/target/x86_64-pc-windows-msvc',
    [string]$OutputDirectory,
    [switch]$ReviewCandidate
)

$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskBuild = if ([IO.Path]::IsPathRooted($BuildDirectory)) { $BuildDirectory } else { Join-Path $taskRoot $BuildDirectory }
$taskSource = Join-Path $taskBuild 'release/try.exe'
if (-not (Test-Path -LiteralPath $taskSource)) { throw 'Build a distribution first: ./scripts/build_windows.ps1 -Distribution' }
$taskCommit = & git -C $taskRoot rev-parse HEAD
if ($LASTEXITCODE -ne 0) { throw 'A Git source commit is required for release packaging.' }
$taskCheckParams = @{
    Executable = $taskSource; ExpectedCommit = $taskCommit
    RequireCleanSource = -not $ReviewCandidate
}
$taskCheck = & (Join-Path $PSScriptRoot 'check_windows_release.ps1') @taskCheckParams | ConvertFrom-Json
$taskReleases = Join-Path $taskRoot 'local_artifacts/releases'
New-Item -ItemType Directory -Path $taskReleases -Force | Out-Null
$taskBuildName = 'windows_x64_' + ($taskCheck.version -replace '\.', '_')
$taskName = 'try.exe'
$taskOutput = if ($OutputDirectory) {
    [IO.Path]::GetFullPath($(if ([IO.Path]::IsPathRooted($OutputDirectory)) { $OutputDirectory } else { Join-Path $taskRoot $OutputDirectory }))
} else { Join-Path $taskReleases $taskBuildName }
if (-not $taskOutput.StartsWith($taskReleases + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Release output must stay inside local_artifacts/releases.'
}
$taskBuildName = Split-Path -Leaf $taskOutput
$taskExecutable = Join-Path $taskOutput $taskName
if (Test-Path -LiteralPath $taskOutput) { throw 'Release output directory already exists; inspect it before replacing it.' }
New-Item -ItemType Directory -Path $taskOutput -Force | Out-Null
Copy-Item -LiteralPath $taskSource -Destination $taskExecutable
$taskHash = (Get-FileHash -LiteralPath $taskExecutable).Hash.ToLowerInvariant()
if ($taskHash -ne $taskCheck.sha256) { throw 'Release copy checksum mismatch.' }
[IO.File]::WriteAllText(($taskExecutable + '.sha256'), "$taskHash  $taskName`n", [Text.UTF8Encoding]::new($false))
$taskRecord = [ordered]@{
    executable = $taskExecutable; version = $taskCheck.version
    source_commit = $taskCheck.source_commit; source_state = $taskCheck.source_state
    prepared_at = [DateTimeOffset]::Now.ToString('o')
    bytes = (Get-Item -LiteralPath $taskExecutable).Length; sha256 = $taskHash
    portable = $true; runtime_static = $true; notices_embedded = $true
    lock_sha256 = $taskCheck.lock_sha256; components = $taskCheck.components
    notice_bytes = $taskCheck.notice_bytes; notice_sha256 = $taskCheck.notice_sha256
    private_data_excluded = $true; hosted_builds_verified = $false
}
# The review record is local build metadata, separate from the downloadable exe.
[IO.File]::WriteAllText((Join-Path $taskReleases ($taskBuildName + '.json')), ($taskRecord | ConvertTo-Json) + "`n", [Text.UTF8Encoding]::new($false))
$taskRecord | ConvertTo-Json
