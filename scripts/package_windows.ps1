# Compatibility entry point for existing local-preview commands.
# New commands should use prepare_windows_preview.ps1.
param(
    [switch]$DesktopShortcut,
    [string]$SeedDataDirectory,
    [string]$BuildDirectory = 'local_artifacts/target/x86_64-pc-windows-msvc'
)

$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'prepare_windows_preview.ps1') @PSBoundParameters
