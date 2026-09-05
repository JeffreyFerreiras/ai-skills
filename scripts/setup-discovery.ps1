param([string]$RepositoryRoot = (Split-Path $PSScriptRoot -Parent))
$ErrorActionPreference = 'Stop'
$repoPath = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$canonical = Join-Path $repoPath 'skills'
$alias = Join-Path $repoPath '.agents/skills'
if (-not (Test-Path -LiteralPath $canonical -PathType Container)) { throw 'Canonical skills directory is missing.' }
if (Test-Path -LiteralPath $alias -PathType Container) {
    Write-Output 'Discovery directory exists; run skill-doctor to verify its contents.'
    return
}
if (Test-Path -LiteralPath $alias) { throw 'Discovery path is not a directory; refusing replacement.' }
New-Item -ItemType Directory -Path (Split-Path $alias -Parent) -Force | Out-Null
New-Item -ItemType Junction -Path $alias -Target $canonical | Out-Null
Write-Output 'Created a local Windows junction to the canonical skills directory.'
