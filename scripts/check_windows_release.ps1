param(
    [Parameter(Mandatory)][string]$Executable,
    [string]$ExpectedCommit,
    [switch]$RequireCleanSource
)

$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskExecutable = [IO.Path]::GetFullPath($Executable)
if (-not (Test-Path -LiteralPath $taskExecutable -PathType Leaf)) { throw 'Release executable is missing.' }
if (-not ('TryReleaseImports' -as [type])) {
    Add-Type @'
using System;
using System.IO;
using System.Collections.Generic;
using System.Text;
public static class TryReleaseImports {
    static uint U32(byte[] bytes, int offset) { return BitConverter.ToUInt32(bytes, offset); }
    static int Offset(byte[] bytes, int sections, int count, uint rva) {
        for (int index = 0; index < count; index++) {
            int section = sections + index * 40;
            uint start = U32(bytes, section + 12);
            uint length = Math.Max(U32(bytes, section + 8), U32(bytes, section + 16));
            if (rva >= start && (ulong)rva < (ulong)start + length) {
                return checked((int)(U32(bytes, section + 20) + rva - start));
            }
        }
        throw new InvalidDataException("Import RVA has no PE section.");
    }
    public static string[] Read(string path) {
        byte[] bytes = File.ReadAllBytes(path);
        if (bytes.Length < 64 || bytes[0] != 'M' || bytes[1] != 'Z') throw new InvalidDataException("Not a PE executable.");
        int pe = checked((int)U32(bytes, 60));
        if (U32(bytes, pe) != 0x00004550 || BitConverter.ToUInt16(bytes, pe + 4) != 0x8664) throw new InvalidDataException("Release must be Windows x64.");
        int optional = pe + 24;
        if (BitConverter.ToUInt16(bytes, optional) != 0x20b) throw new InvalidDataException("Expected PE32+.");
        int count = BitConverter.ToUInt16(bytes, pe + 6);
        int sections = optional + BitConverter.ToUInt16(bytes, pe + 20);
        if (U32(bytes, optional + 112 + 13 * 8) != 0) throw new InvalidDataException("Inspect unexpected delayed imports before release.");
        uint imports = U32(bytes, optional + 112 + 8);
        var names = new List<string>();
        if (imports == 0) return names.ToArray();
        int descriptor = Offset(bytes, sections, count, imports);
        while (U32(bytes, descriptor + 12) != 0) {
            int name = Offset(bytes, sections, count, U32(bytes, descriptor + 12));
            int end = Array.IndexOf<byte>(bytes, 0, name);
            if (end < 0) throw new InvalidDataException("Unterminated import name.");
            names.Add(Encoding.ASCII.GetString(bytes, name, end - name));
            descriptor = checked(descriptor + 20);
        }
        return names.ToArray();
    }
}
'@
}
$taskAllowed = @('advapi32.dll', 'bcrypt.dll', 'bcryptprimitives.dll', 'combase.dll', 'comctl32.dll', 'dwrite.dll', 'dwmapi.dll', 'gdi32.dll', 'imm32.dll', 'kernel32.dll', 'ntdll.dll', 'ole32.dll', 'oleaut32.dll', 'shell32.dll', 'shlwapi.dll', 'user32.dll', 'uxtheme.dll', 'winhttp.dll', 'ws2_32.dll')
$taskImports = [TryReleaseImports]::Read($taskExecutable) | Sort-Object -Unique
foreach ($taskDll in $taskImports) {
    if ($taskDll -match '^(vcruntime|msvcp|msvcr|concrt|ucrtbase|api-ms-win-crt-)') { throw "External C/C++ runtime dependency remains: $taskDll" }
    if ($taskDll.ToLowerInvariant() -notin $taskAllowed -and $taskDll -notmatch '^api-ms-win-(core|shcore|ntuser)-') {
        throw "Inspect unexpected runtime dependency before release: $taskDll"
    }
}
function Read-DistributionOutput([string]$Argument) {
    $taskProbe = [Diagnostics.Process]::new()
    $taskProbe.StartInfo.FileName = $taskExecutable
    $taskProbe.StartInfo.Arguments = $Argument
    $taskProbe.StartInfo.WorkingDirectory = $taskRoot
    $taskProbe.StartInfo.UseShellExecute = $false
    $taskProbe.StartInfo.CreateNoWindow = $true
    $taskProbe.StartInfo.RedirectStandardOutput = $true
    $taskProbe.StartInfo.RedirectStandardError = $true
    $taskProbe.StartInfo.StandardOutputEncoding = [Text.UTF8Encoding]::new($false)
    $taskProbe.StartInfo.StandardErrorEncoding = [Text.UTF8Encoding]::new($false)
    try {
        if (-not $taskProbe.Start()) { throw 'Cannot start the distribution probe.' }
        $taskOutputRead = $taskProbe.StandardOutput.ReadToEndAsync()
        $taskErrorRead = $taskProbe.StandardError.ReadToEndAsync()
        if (-not $taskProbe.WaitForExit(10000)) {
            $taskProbe.Kill()
            if (-not $taskProbe.WaitForExit(3000)) { throw 'Distribution probe did not stop after timeout.' }
            throw 'Distribution probe timed out.'
        }
        $taskOutput = $taskOutputRead.GetAwaiter().GetResult()
        $taskProbeError = $taskErrorRead.GetAwaiter().GetResult()
        if ($taskProbe.ExitCode -ne 0) { throw "Cannot read distribution output: $taskProbeError" }
        return $taskOutput
    } finally {
        $taskProbe.Dispose()
    }
}
$taskInfo = Read-DistributionOutput '--distribution-info' | ConvertFrom-Json
if ($ExpectedCommit -and $taskInfo.source_commit -ne $ExpectedCommit) {
    throw 'Executable source commit does not match the expected release source.'
}
if ($RequireCleanSource -and ($taskInfo.source_state -ne 'clean' -or $taskInfo.source_commit -notmatch '^[0-9a-f]{40}$')) {
    throw 'Release executable must come from a clean Git commit.'
}
if ($taskInfo.format -ne 'try_distribution_info' -or -not $taskInfo.portable -or
    -not $taskInfo.runtime_static -or -not $taskInfo.notices_complete -or $taskInfo.components -le 3) {
    throw 'Executable is not a complete single-file portable distribution.'
}
$taskLock = (Get-FileHash -LiteralPath (Join-Path $taskRoot 'Cargo.lock')).Hash.ToLowerInvariant()
if ($taskInfo.lock_sha256 -ne $taskLock) { throw 'Embedded notices do not match Cargo.lock.' }
if ((Get-Item -LiteralPath $taskExecutable).VersionInfo.ProductVersion -ne $taskInfo.version) { throw 'Executable version resource is inconsistent.' }
$taskNotices = Read-DistributionOutput '--third-party-notices'
$taskBundle = Get-Content -Raw -LiteralPath (Join-Path $taskRoot 'local_artifacts/open_source/embedded_notices.json') | ConvertFrom-Json
if ($taskBundle.lock_sha256 -ne $taskLock -or $taskBundle.components.Count -ne $taskInfo.components) { throw 'Prepared notice bundle is inconsistent.' }
foreach ($taskComponent in $taskBundle.components) {
    $taskHeader = "$($taskComponent.name) $($taskComponent.version)`nLicense: $($taskComponent.license)`nSource: $($taskComponent.source)"
    if ($taskNotices.IndexOf($taskHeader, [StringComparison]::Ordinal) -lt 0) { throw "Missing embedded source or license: $($taskComponent.name)" }
    foreach ($taskDocument in $taskComponent.documents) {
        if ($taskNotices.IndexOf($taskDocument.text, [StringComparison]::Ordinal) -lt 0) { throw "Embedded notice differs from original: $($taskComponent.name)/$($taskDocument.name)" }
    }
}
$taskNoticeBytes = [Text.UTF8Encoding]::new($false).GetBytes($taskNotices)
$taskSha = [Security.Cryptography.SHA256]::Create()
try {
    $taskNoticeHash = [BitConverter]::ToString($taskSha.ComputeHash($taskNoticeBytes)).Replace('-', '').ToLowerInvariant()
} finally {
    $taskSha.Dispose()
}
[ordered]@{
    executable = $taskExecutable; version = $taskInfo.version
    source_commit = $taskInfo.source_commit; source_state = $taskInfo.source_state
    portable = $taskInfo.portable; runtime_static = $taskInfo.runtime_static
    notices_complete = $taskInfo.notices_complete; components = $taskInfo.components
    lock_sha256 = $taskInfo.lock_sha256; imports = @($taskImports)
    notice_bytes = $taskNoticeBytes.Length
    notice_sha256 = $taskNoticeHash
    sha256 = (Get-FileHash -LiteralPath $taskExecutable).Hash.ToLowerInvariant()
} | ConvertTo-Json -Depth 3
