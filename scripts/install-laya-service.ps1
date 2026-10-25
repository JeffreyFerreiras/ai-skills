# Run from an elevated PowerShell. Does not change Codex hook trust.
[CmdletBinding()]
param([string]$Root = 'C:\models\laya', [int]$Port = 8766)
$ErrorActionPreference = 'Stop'
if (-not ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Open PowerShell as Administrator and run this installer again.'
}
if ($Port -lt 1 -or $Port -gt 65535) { throw 'Port must be between 1 and 65535.' }
$Root = (Resolve-Path -LiteralPath $Root).Path
$python = Join-Path $Root 'runtime\Scripts\python.exe'
if (-not (Test-Path -LiteralPath $python)) { throw "Missing runtime: $python" }
if (-not (Test-Path -LiteralPath (Join-Path $Root 'weights'))) { throw 'Missing offline weights.' }
if (Get-Service -Name LayaCodex -ErrorAction SilentlyContinue) { throw 'LayaCodex already exists. Stop it, update its files, then restart it.' }
if (Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue) { throw "Port $Port is already in use. Stop the existing listener first." }
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'laya-service.py') -Destination (Join-Path $Root 'laya-service.py') -Force
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'laya-hook.py') -Destination (Join-Path $Root 'laya-hook.py') -Force
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'laya_routing.py') -Destination (Join-Path $Root 'laya_routing.py') -Force
$baseExe = & $python -B -c 'import sys;print(sys._base_executable)'
if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $baseExe)) { throw 'Could not locate the real Python executable.' }
# The venv executable spawns a child, which cannot connect to SCM as its parent.
$binary = '"{0}" -B "{1}" --windows-service --root "{2}" --port {3}' -f $baseExe, (Join-Path $Root 'laya-service.py'), $Root, $Port
# LocalService can read the runtime and weights without an administrator identity.
& icacls.exe $Root /grant '*S-1-5-19:(OI)(CI)RX' /T /Q | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Could not grant LocalService runtime access.' }
$basePython = & $python -B -c 'import sys;print(sys.base_prefix)'
if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $basePython)) { throw 'Could not locate the base Python installation.' }
# A virtual environment still needs its base interpreter DLLs and standard library.
& icacls.exe $basePython /grant '*S-1-5-19:(OI)(CI)RX' /T /Q | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Could not grant LocalService base Python access.' }
$serviceData = Join-Path $Root 'service-data'
New-Item -ItemType Directory -Path $serviceData -Force | Out-Null
& icacls.exe $serviceData /grant '*S-1-5-19:(OI)(CI)M' /T /Q | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Could not grant LocalService cache access.' }
$credential = [PSCredential]::new('NT AUTHORITY\LocalService', (New-Object Security.SecureString))
New-Service -Name LayaCodex -DisplayName 'Laya Codex local classifier' -BinaryPathName $binary -StartupType Automatic -Credential $credential | Out-Null
& sc.exe config LayaCodex start= delayed-auto | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Could not set delayed automatic startup.' }
& sc.exe failure LayaCodex reset= 86400 actions= restart/5000/restart/15000/restart/60000 | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Could not configure recovery.' }
& sc.exe failureflag LayaCodex 1 | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Could not enable recovery on service failure.' }
Start-Service LayaCodex
$deadline = (Get-Date).AddSeconds(120)
do {
    try { $health = Invoke-RestMethod -Uri "http://127.0.0.1:$Port/health" -TimeoutSec 2 } catch { $health = $null }
    $service = Get-CimInstance Win32_Service -Filter "Name='LayaCodex'"
    if ($health.ready -and $service.State -eq 'Running' -and $health.pid -eq $service.ProcessId) {
        Get-Service LayaCodex; $health; return
    }
    Start-Sleep -Milliseconds 500
} while ((Get-Date) -lt $deadline)
throw 'Service did not become ready in 120 seconds. Inspect Get-Service LayaCodex, service-data\startup-error.txt, and the Windows System event log.'
