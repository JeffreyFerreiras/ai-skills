param([Parameter(Mandatory=$true)][string]$Project)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Speech
$spec = Get-Content -LiteralPath (Join-Path $Project 'video-spec.json') -Raw | ConvertFrom-Json
$audioDir = Join-Path $Project 'public/audio'
[System.IO.Directory]::CreateDirectory($audioDir) | Out-Null
$voice = New-Object System.Speech.Synthesis.SpeechSynthesizer
try {
    foreach ($scene in $spec.scenes) {
        if ($scene.id -notmatch '^[a-z0-9-]+$') { throw 'Unsafe fixture scene ID' }
        $output = Join-Path $audioDir ($scene.id + '.wav')
        if (Test-Path -LiteralPath $output) { throw "Refusing to replace $output" }
        $voice.SetOutputToWaveFile($output)
        $voice.Speak([string]$scene.narration.text)
        $voice.SetOutputToNull()
    }
    [pscustomobject]@{ engine='Windows System.Speech'; voice=$voice.Voice.Name; rate=$voice.Rate; purpose='Synthetic integration fixture, not an approved production voice' } | ConvertTo-Json
} finally {
    $voice.Dispose()
}
