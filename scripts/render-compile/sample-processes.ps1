param([int]$ObserverParentPid, [int]$IntervalMs, [string]$StopFile, [string]$ExpectedCompilerPath)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'process-identities.ps1')
function Emit($record) { [Console]::Out.WriteLine(($record | ConvertTo-Json -Compress -Depth 5)) }
try {
    $initial = Census
    $owner = @($initial.rows | Where-Object { $_.pid -eq $ObserverParentPid })
    if ($owner.Count -ne 1) { throw 'observer parent unavailable' }
    $created = $owner[0].createdMs
    Emit @{ kind = 'ready' }
    while (-not (Test-Path -LiteralPath $StopFile)) {
        $frame = Census
        $sameOwner = @($frame.rows | Where-Object { $_.pid -eq $ObserverParentPid -and $_.createdMs -eq $created })
        if ($sameOwner.Count -ne 1) { break }
        Add-OwnedRustcIdentifiers $frame $ObserverParentPid $created $ExpectedCompilerPath
        Emit $frame
        Start-Sleep -Milliseconds $IntervalMs
    }
} catch { Emit @{ kind = 'error'; code = 'cim-observer-failed' }; exit 1 }
