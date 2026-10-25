$ErrorActionPreference = 'Stop'
try {
    $runtimeConfig = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'config.json') -Raw | ConvertFrom-Json
    if ($runtimeConfig.enabled) {
        & (Join-Path $PSScriptRoot 'manage.ps1') Start | Out-Null
    }
} catch {
    $record = @{ timestamp = [DateTime]::UtcNow.ToString('o'); error_type = $_.Exception.GetType().Name }
    try {
        $record | ConvertTo-Json -Compress | Add-Content -LiteralPath (Join-Path $PSScriptRoot 'startup-errors.jsonl')
    } catch {}
}
exit 0
