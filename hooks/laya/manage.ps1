param([ValidateSet('Start', 'Stop', 'Status', 'Enable', 'Disable')][string]$Action = 'Status')

$ErrorActionPreference = 'Stop'
$runtimeRoot = $PSScriptRoot
$runtimePython = Join-Path $runtimeRoot '.venv\Scripts\python.exe'
$serverScript = Join-Path $runtimeRoot 'server.py'
$serverPidPath = Join-Path $runtimeRoot 'server.pid'
$configPath = Join-Path $runtimeRoot 'config.json'
$runtimeConfig = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json

function Get-OwnedServer {
    if (-not (Test-Path -LiteralPath $serverPidPath)) { return $null }
    $serverProcessId = [int](Get-Content -LiteralPath $serverPidPath -Raw)
    $serverProcess = Get-CimInstance Win32_Process -Filter "ProcessId = $serverProcessId"
    if ($serverProcess -and $serverProcess.ExecutablePath -eq $runtimePython -and
        $serverProcess.CommandLine.Contains($serverScript)) {
        return $serverProcess
    }
    return $null
}

switch ($Action) {
    'Start' {
        $startupMutex = [System.Threading.Mutex]::new($false, 'Local\CodexLayaHookStart')
        $startupLockHeld = $false
        try {
            $startupLockHeld = $startupMutex.WaitOne(5000)
            if (-not $startupLockHeld) { throw 'Another Laya startup is still in progress.' }
            $ownedServer = Get-OwnedServer
            if ($ownedServer) { Write-Output "Already running: $($ownedServer.ProcessId)"; return }
            if (Get-NetTCPConnection -LocalPort $runtimeConfig.port -State Listen -ErrorAction SilentlyContinue) {
                throw "Port $($runtimeConfig.port) is already in use."
            }
            $serverProcess = Start-Process -FilePath $runtimePython -ArgumentList @("`"$serverScript`"") `
                -WorkingDirectory $runtimeRoot -WindowStyle Hidden -PassThru `
                -RedirectStandardOutput (Join-Path $runtimeRoot 'server.stdout.log') `
                -RedirectStandardError (Join-Path $runtimeRoot 'server.stderr.log')
            Set-Content -LiteralPath $serverPidPath -Value $serverProcess.Id -Encoding ascii
            Write-Output "Started Laya server: $($serverProcess.Id). Use Status to check readiness."
        } finally {
            if ($startupLockHeld) { $startupMutex.ReleaseMutex() }
            $startupMutex.Dispose()
        }
    }
    'Stop' {
        $ownedServer = Get-OwnedServer
        if ($ownedServer) { Stop-Process -Id $ownedServer.ProcessId; Write-Output 'Stopped Laya server.' }
        else { Write-Output 'No owned Laya server is running.' }
    }
    'Status' {
        $ownedServer = Get-OwnedServer
        if (-not $ownedServer) { Write-Output 'Laya server is stopped.'; return }
        $headers = @{ Authorization = 'Bearer ' + (Get-Content -LiteralPath (Join-Path $runtimeRoot 'token.txt') -Raw).Trim() }
        try {
            Invoke-RestMethod -Uri "http://127.0.0.1:$($runtimeConfig.port)/health" -Headers $headers -TimeoutSec 2 |
                Select-Object status, loaded, device, checkpoint_devices
        } catch { Write-Output 'Laya server is starting or unavailable. See server.stderr.log.' }
    }
    { $_ -in 'Enable', 'Disable' } {
        $runtimeConfig.enabled = $Action -eq 'Enable'
        $runtimeConfig | ConvertTo-Json | Set-Content -LiteralPath $configPath -Encoding utf8NoBOM
        Write-Output "Laya routing enabled: $($runtimeConfig.enabled)"
    }
}
