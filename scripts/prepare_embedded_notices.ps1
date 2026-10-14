param(
    [string]$NoticesDirectory = 'local_artifacts/open_source/distribution_notices',
    [string]$OutputFile = 'local_artifacts/open_source/embedded_notices.json',
    [string]$Target = 'x86_64-pc-windows-msvc'
)

$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskArtifacts = [IO.Path]::GetFullPath((Join-Path $taskRoot 'local_artifacts'))
$taskNotices = [IO.Path]::GetFullPath((Join-Path $taskRoot $NoticesDirectory))
$taskOutput = [IO.Path]::GetFullPath((Join-Path $taskRoot $OutputFile))
if (-not $taskOutput.StartsWith($taskArtifacts + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Embedded notices must stay within local_artifacts.'
}
$taskInventoryPath = Join-Path $taskNotices 'inventory.json'
if (-not (Test-Path -LiteralPath $taskInventoryPath)) {
    throw 'Prepare third-party notices first: ./scripts/prepare_third_party.ps1 -FetchUpstream'
}
$taskInventory = Get-Content -LiteralPath $taskInventoryPath -Raw | ConvertFrom-Json
$taskLockFile = Join-Path $taskRoot 'Cargo.lock'
if ($taskInventory.format -ne 'try_third_party_inventory' -or $taskInventory.schema_version -ne 1 -or
    $taskInventory.lock_sha256 -ne (Get-FileHash -LiteralPath $taskLockFile).Hash.ToLowerInvariant()) {
    throw 'Third-party inventory does not match Cargo.lock.'
}
if (@($taskInventory.unresolved_components).Count -ne 0 -or @($taskInventory.packages).Count -eq 0) {
    throw 'Third-party inventory is incomplete.'
}
if ($Target -notin $taskInventory.targets) { throw "Dependencies for $Target are missing from the inventory." }
$taskUtf8 = [Text.UTF8Encoding]::new($false, $true)
$taskComponents = [Collections.Generic.List[object]]::new()
$taskComponents.Add([ordered]@{
    name = 'Try'; version = (Get-Content -LiteralPath (Join-Path $taskRoot 'Cargo.toml') | Select-String '^version = "([^"]+)"').Matches[0].Groups[1].Value
    license = 'MIT'; source = 'https://github.com/Ethan-decoy/Try'
    documents = @(
        [ordered]@{ name = 'license'; text = [IO.File]::ReadAllText((Join-Path $taskRoot 'license'), $taskUtf8) },
        [ordered]@{ name = 'distribution notes'; text = @'
Try's own source code and logo are MIT licensed. Bundled components and fonts retain their own licenses.
Slint 1.18.1 is used under its Royalty-free 2.0 license; Try provides the official Made with Slint badge in its project README.
STIX Two Math 2.13 is used under the SIL Open Font License 1.1.
The default FSRS 6 scalar inference subset is adapted from fsrs-rs 6.6.2 under BSD-3-Clause.
This notice collection covers enabled normal and build dependencies for the platforms in the inventory. It is not a count of linked runtime libraries.
The Source entry for each component identifies its exact published source archive. option-ext 0.2.0 source is available at https://crates.io/api/v1/crates/option-ext/0.2.0/download under MPL-2.0.
Third-party license and NOTICE text is reproduced below without changing its terms. Listing alternate upstream licenses does not change Try's selected Slint license.
'@ }
    )
})
$taskComponents.Add([ordered]@{
    name = 'Slint'; version = '1.18.1'; license = 'Royalty-free 2.0'
    source = 'https://github.com/slint-ui/slint/tree/v1.18.1'
    documents = @([ordered]@{ name = 'Slint Royalty-free 2.0'; text = [IO.File]::ReadAllText((Join-Path $taskRoot 'third_party_licenses/slint_royalty_free_2_0.md'), $taskUtf8) })
})
$taskComponents.Add([ordered]@{
    name = 'STIX Two Math'; version = '2.13'; license = 'SIL Open Font License 1.1'
    source = 'https://github.com/stipub/stixfonts'
    documents = @([ordered]@{ name = 'copyright and OFL'; text = [IO.File]::ReadAllText((Join-Path $taskRoot 'third_party_licenses/stix_two_math_ofl.txt'), $taskUtf8) })
})
$taskComponents.Add([ordered]@{
    name = 'FSRS scalar inference'; version = '6.6.2'; license = 'BSD-3-Clause'
    source = 'https://github.com/open-spaced-repetition/fsrs-rs/tree/6088949b99d44f46f5328fd7289d209b9a842b68'
    documents = @([ordered]@{ name = 'copyright and BSD-3-Clause'; text = [IO.File]::ReadAllText((Join-Path $taskRoot 'third_party_licenses/fsrs_bsd_3_clause.txt'), $taskUtf8) })
})
foreach ($taskComponent in $taskInventory.packages) {
    if (@($taskComponent.texts).Count -eq 0) { throw "License text missing for $($taskComponent.name)." }
    $taskDocuments = foreach ($taskDocument in $taskComponent.texts) {
        $taskPath = [IO.Path]::GetFullPath((Join-Path $taskNotices $taskDocument.bundled_path))
        if (-not $taskPath.StartsWith($taskNotices + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'License path escaped the notices directory.'
        }
        if ((Get-FileHash -LiteralPath $taskPath).Hash.ToLowerInvariant() -ne $taskDocument.sha256) {
            throw "License checksum mismatch: $($taskDocument.bundled_path)"
        }
        [ordered]@{ name = $taskDocument.original_path; text = [IO.File]::ReadAllText($taskPath, $taskUtf8) }
    }
    $taskComponents.Add([ordered]@{
        name = $taskComponent.name; version = $taskComponent.version
        license = $taskComponent.declared_license; source = $taskComponent.source
        documents = @($taskDocuments)
    })
}
$taskBundle = [ordered]@{
    format = 'try_embedded_notices'; schema_version = 1
    lock_sha256 = $taskInventory.lock_sha256
    lock_content = [IO.File]::ReadAllText($taskLockFile, $taskUtf8)
    components = $taskComponents.ToArray()
}
New-Item -ItemType Directory -Path (Split-Path -Parent $taskOutput) -Force | Out-Null
$taskContents = ($taskBundle | ConvertTo-Json -Depth 8) + "`n"
# Avoid invalidating an otherwise unchanged Rust build on each invocation.
if (-not (Test-Path -LiteralPath $taskOutput) -or [IO.File]::ReadAllText($taskOutput, $taskUtf8) -cne $taskContents) {
    [IO.File]::WriteAllText($taskOutput, $taskContents, [Text.UTF8Encoding]::new($false))
}
[ordered]@{ components = $taskComponents.Count; lock_sha256 = $taskInventory.lock_sha256; output = $taskOutput } | ConvertTo-Json
