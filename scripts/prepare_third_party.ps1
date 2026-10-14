param(
    [switch]$Offline,
    [switch]$FetchUpstream,
    [string]$MetadataFile,
    [string]$VersionsFile,
    [string[]]$Targets = @('x86_64-pc-windows-msvc', 'aarch64-apple-darwin', 'x86_64-unknown-linux-gnu'),
    [string]$OutputDirectory = 'local_artifacts/open_source/distribution_notices'
)

$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskArtifacts = [IO.Path]::GetFullPath((Join-Path $taskRoot 'local_artifacts'))
$taskOutput = [IO.Path]::GetFullPath((Join-Path $taskRoot $OutputDirectory))
if (-not $taskOutput.StartsWith($taskArtifacts + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Third-party output must stay within this project local_artifacts directory.'
}
New-Item -ItemType Directory -Path $taskOutput -Force | Out-Null
if (-not $MetadataFile -or -not $VersionsFile) {
    if (Test-Path -LiteralPath (Join-Path $taskRoot 'local_artifacts/cargo_home/registry')) { $env:CARGO_HOME = Join-Path $taskRoot 'local_artifacts/cargo_home' }
    $taskCargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
    $taskCargo = if ($taskCargoCommand) { $taskCargoCommand.Source } else { $null }
    if (-not $taskCargo -and $env:RUSTUP_HOME) {
        $taskCargo = Join-Path $env:RUSTUP_HOME 'toolchains/stable-x86_64-pc-windows-msvc/bin/cargo.exe'
    }
    if (-not $taskCargo) { throw 'Cargo was not found.' }
    if (-not $MetadataFile) {
        $taskArguments = @('metadata', '--manifest-path', (Join-Path $taskRoot 'Cargo.toml'), '--locked', '--format-version', '1')
        if ($Offline) { $taskArguments += '--offline' }
        $MetadataFile = Join-Path $taskOutput 'dependency_metadata.json'
        & $taskCargo @taskArguments | Set-Content -LiteralPath $MetadataFile -Encoding utf8
        if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed.' }
    }
    if (-not $VersionsFile) {
        $taskVersions = foreach ($taskTarget in $Targets) {
            $taskArguments = @('tree', '--manifest-path', (Join-Path $taskRoot 'Cargo.toml'), '--locked', '--target', $taskTarget, '--edges', 'normal,build', '--prefix', 'none', '--format', '{p}')
            if ($Offline) { $taskArguments += '--offline' }
            & $taskCargo @taskArguments
            if ($LASTEXITCODE -ne 0) { throw 'Cargo dependency graph failed.' }
        }
        $VersionsFile = Join-Path $taskOutput 'dependency_versions.txt'
        $taskVersions | ForEach-Object { if ($_ -match '^([^ ]+) v([^ ]+)') { "$($Matches[1]) $($Matches[2])" } } | Sort-Object -Unique | Set-Content -LiteralPath $VersionsFile -Encoding utf8
    }
}
$taskMetadata = Get-Content -LiteralPath $MetadataFile -Raw | ConvertFrom-Json
$taskIncluded = Get-Content -LiteralPath $VersionsFile
$taskTrees = @{}
$taskRecords = @()
$taskUnresolved = @()
foreach ($taskPackage in ($taskMetadata.packages | Where-Object source | Sort-Object name,version)) {
    if (($taskPackage.name + ' ' + $taskPackage.version) -notin $taskIncluded) { continue }
    $taskPackageRoot = Split-Path -Parent $taskPackage.manifest_path
    $taskKey = ($taskPackage.name + '_' + $taskPackage.version).ToLowerInvariant() -replace '[^a-z0-9_]', '_'
    $taskDestination = Join-Path $taskOutput $taskKey
    New-Item -ItemType Directory -Path $taskDestination -Force | Out-Null
    $taskFiles = @(Get-ChildItem -LiteralPath $taskPackageRoot -Recurse -File | Where-Object {
        $_.Name -match '^(unlicen[sc]e|licen[sc]e|copying|copyright|notice|ofl)([._-]|$)' -or
        $_.FullName.Substring($taskPackageRoot.Length) -match '[/\\]licen[sc]es[/\\]'
    })
    if ($taskPackage.license_file) {
        $taskDeclaredFile = Join-Path $taskPackageRoot $taskPackage.license_file
        if (Test-Path -LiteralPath $taskDeclaredFile -PathType Leaf) { $taskFiles += Get-Item -LiteralPath $taskDeclaredFile }
    }
    $taskTexts = @()
    $taskIndex = 0
    foreach ($taskFile in ($taskFiles | Sort-Object FullName -Unique)) {
        $taskRelative = [IO.Path]::GetRelativePath($taskPackageRoot, $taskFile.FullName)
        $taskFileName = ('{0:d4}_' -f $taskIndex) + ($taskRelative.ToLowerInvariant() -replace '[^a-z0-9.]', '_')
        if (-not [IO.Path]::GetExtension($taskFileName)) { $taskFileName += '.txt' }
        $taskTarget = Join-Path $taskDestination $taskFileName
        Copy-Item -LiteralPath $taskFile.FullName -Destination $taskTarget -Force
        $taskHash = (Get-FileHash -LiteralPath $taskFile.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        if ((Get-FileHash -LiteralPath $taskTarget -Algorithm SHA256).Hash.ToLowerInvariant() -ne $taskHash) { throw 'License copy checksum mismatch.' }
        $taskTexts += [ordered]@{ original_path = $taskRelative.Replace('\', '/'); bundled_path = "$taskKey/$taskFileName"; sha256 = $taskHash }
        $taskIndex++
    }
    if ($taskTexts.Count -eq 0 -and $taskPackage.name -eq 'dispatch' -and $taskPackage.version -eq '0.2.0') {
        $taskNotice = Join-Path $taskRoot 'third_party_licenses/dispatch_0_2_0_notice.txt'
        Copy-Item -LiteralPath $taskNotice -Destination (Join-Path $taskDestination 'notice.txt') -Force
        Copy-Item -LiteralPath (Join-Path $taskPackageRoot 'Cargo.toml.orig') -Destination (Join-Path $taskDestination 'upstream_manifest.txt') -Force
        $taskTexts = @(
            [ordered]@{ original_path = 'Try notice with standard MIT reference'; bundled_path = "$taskKey/notice.txt"; source = 'https://spdx.org/licenses/MIT.html'; sha256 = (Get-FileHash -LiteralPath $taskNotice).Hash.ToLowerInvariant() },
            [ordered]@{ original_path = 'Cargo.toml.orig'; bundled_path = "$taskKey/upstream_manifest.txt"; sha256 = (Get-FileHash -LiteralPath (Join-Path $taskPackageRoot 'Cargo.toml.orig')).Hash.ToLowerInvariant() }
        )
    }
    if ($taskTexts.Count -eq 0 -and $FetchUpstream) {
        $taskVcsPath = Join-Path $taskPackageRoot '.cargo_vcs_info.json'
        if (-not (Test-Path -LiteralPath $taskVcsPath)) { throw "No pinned upstream commit for $taskKey." }
        $taskVcs = Get-Content -LiteralPath $taskVcsPath -Raw | ConvertFrom-Json
        if ($taskPackage.repository -notmatch '^https?://github\.com/([a-zA-Z0-9_.-]+)/([a-zA-Z0-9_.-]+?)(?:\.git)?/?$') {
            throw "Unsupported pinned upstream for $taskKey."
        }
        $taskRepository = "$($Matches[1])/$($Matches[2])"
        $taskCommit = $taskVcs.git.sha1
        if ($taskCommit -notmatch '^[a-f0-9]{40}$') { throw "No valid upstream commit for $taskKey." }
        $taskTreeKey = "$taskRepository/$taskCommit"
        if (-not $taskTrees.ContainsKey($taskTreeKey)) {
            $taskTreeCache = Join-Path $taskOutput (('upstream_' + $taskTreeKey -replace '[^a-zA-Z0-9_]', '_') + '.json')
            if (Test-Path -LiteralPath $taskTreeCache) { $taskTrees[$taskTreeKey] = Get-Content -LiteralPath $taskTreeCache -Raw | ConvertFrom-Json }
            else {
                try {
                    $taskTrees[$taskTreeKey] = Invoke-RestMethod -Uri "https://api.github.com/repos/$taskRepository/git/trees/${taskCommit}?recursive=1" -Headers @{ 'User-Agent' = 'try-license-preparation' } -TimeoutSec 30
                } catch {
                    # Public API quotas need not block a fixed-commit license
                    # inventory. List the corresponding official source archive;
                    # text still comes from the exact raw commit URLs below.
                    $taskArchive = Join-Path $taskOutput (('source_' + $taskTreeKey -replace '[^a-zA-Z0-9_]', '_') + '.tar.gz')
                    if (-not (Test-Path -LiteralPath $taskArchive)) {
                        Invoke-WebRequest -Uri "https://codeload.github.com/$taskRepository/tar.gz/$taskCommit" -OutFile $taskArchive -TimeoutSec 60
                    }
                    $taskArchiveEntries = @(& tar -tzf $taskArchive)
                    if ($LASTEXITCODE -ne 0) { throw 'Could not list the fixed upstream archive.' }
                    $taskArchiveTree = foreach ($taskEntry in $taskArchiveEntries) {
                        if ($taskEntry.EndsWith('/')) { continue }
                        $taskRelative = ($taskEntry -split '/', 2)[1]
                        if (-not $taskRelative -or $taskRelative.StartsWith('/') -or $taskRelative -match '(^|/)\.\.(/|$)') { throw 'Invalid path in the upstream archive.' }
                        if ([IO.Path]::GetFileName($taskRelative) -match '^(unlicen[sc]e|licen[sc]e|copying|copyright|notice|ofl)([._-]|$)') {
                            [pscustomobject]@{ type = 'blob'; path = $taskRelative }
                        }
                    }
                    $taskTrees[$taskTreeKey] = [pscustomobject]@{ truncated = $false; tree = @($taskArchiveTree) }
                }
                $taskTrees[$taskTreeKey] | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $taskTreeCache -Encoding utf8
            }
        }
        if ($taskTrees[$taskTreeKey].truncated) { throw "Upstream tree is incomplete for $taskKey." }
        $taskPrefix = $taskVcs.path_in_vcs
        $taskUpstreamFiles = $taskTrees[$taskTreeKey].tree | Where-Object {
            $_.type -eq 'blob' -and ([IO.Path]::GetFileName($_.path) -match '^(unlicen[sc]e|licen[sc]e|copying|copyright|notice|ofl)([._-]|$)') -and
            ($_.path -notmatch '/' -or ($taskPrefix -and $_.path.StartsWith($taskPrefix.TrimEnd('/') + '/')))
        }
        foreach ($taskFile in ($taskUpstreamFiles | Sort-Object path)) {
            $taskUrl = "https://raw.githubusercontent.com/$taskRepository/$taskCommit/$($taskFile.path)"
            $taskFileName = ('{0:d4}_' -f $taskIndex) + (($taskFile.path.ToLowerInvariant()) -replace '[^a-z0-9.]', '_')
            if (-not [IO.Path]::GetExtension($taskFileName)) { $taskFileName += '.txt' }
            $taskTarget = Join-Path $taskDestination $taskFileName
            if (-not (Test-Path -LiteralPath $taskTarget)) { Invoke-WebRequest -Uri $taskUrl -OutFile $taskTarget -TimeoutSec 30 }
            $taskTexts += [ordered]@{ original_path = $taskFile.path; bundled_path = "$taskKey/$taskFileName"; source = $taskUrl; sha256 = (Get-FileHash -LiteralPath $taskTarget).Hash.ToLowerInvariant() }
            $taskIndex++
        }
    }
    if ($taskTexts.Count -eq 0 -or -not $taskPackage.license) { $taskUnresolved += "$($taskPackage.name) $($taskPackage.version)" }
    $taskRecords += [ordered]@{
        name = $taskPackage.name; version = $taskPackage.version
        declared_license = $taskPackage.license
        source = "https://crates.io/api/v1/crates/$($taskPackage.name)/$($taskPackage.version)/download"
        repository = $taskPackage.repository; texts = $taskTexts
    }
}
$taskInventory = [ordered]@{
    format = 'try_third_party_inventory'; schema_version = 1
    lock_sha256 = (Get-FileHash -LiteralPath (Join-Path $taskRoot 'Cargo.lock')).Hash.ToLowerInvariant()
    targets = $Targets
    coverage = 'Union of enabled normal and build dependencies for the listed targets. Not a count of linked runtime libraries.'
    unresolved_components = $taskUnresolved; packages = $taskRecords
}
[IO.File]::WriteAllText((Join-Path $taskOutput 'inventory.json'), ($taskInventory | ConvertTo-Json -Depth 10) + "`n", [Text.UTF8Encoding]::new($false))
# Metadata contains local absolute cache paths and is not a public release file.
$taskSummary = [ordered]@{ packages = $taskRecords.Count; license_files = ($taskRecords | ForEach-Object { $_.texts.Count } | Measure-Object -Sum).Sum; unresolved = $taskUnresolved; inventory = (Join-Path $taskOutput 'inventory.json') }
$taskSummary | ConvertTo-Json -Depth 4
if ($taskUnresolved.Count -ne 0) { throw 'Some dependencies need their upstream license text before public packaging.' }
