param([int]$ObserverParentPid, [int]$IntervalMs, [string]$StopFile)
$ErrorActionPreference = 'Stop'
function Emit($record) { [Console]::Out.WriteLine(($record | ConvertTo-Json -Compress -Depth 5)) }
function Census {
    $start = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    $rows = @(Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,CreationDate,Name,WorkingSetSize,PrivatePageCount,CommandLine | ForEach-Object {
        if ($null -ne $_.CreationDate) {
            $kind = 'other'
            if ($_.Name -ieq 'cargo.exe') { $kind = 'cargo' }
            if ($_.Name -ieq 'rustc.exe') { $kind = 'rustc' }
            $working = $null; $private = $null; $crateName = $null; $targetTriple = $null
            if ($kind -ne 'other') {
                if ($null -ne $_.WorkingSetSize) { $working = [long]$_.WorkingSetSize }
                if ($null -ne $_.PrivatePageCount) { $private = [long]$_.PrivatePageCount }
            }
            # Read command lines only locally to extract strict identifier/triple
            # tokens. Never emit the command line, filesystem target paths or values.
            if ($kind -eq 'rustc' -and $null -ne $_.CommandLine) {
                if ($_.CommandLine -match '(?:^|\s)--crate-name(?:=|\s+)"?([A-Za-z_][A-Za-z0-9_]*)"?(?:\s|$)') { $crateName = $Matches[1] }
                if ($_.CommandLine -match '(?:^|\s)--target(?:=|\s+)"?([A-Za-z0-9_-]+)"?(?:\s|$)') { $targetTriple = $Matches[1] }
            }
            @{ pid = [int]$_.ProcessId; parentPid = [int]$_.ParentProcessId;
               createdMs = ([DateTimeOffset]$_.CreationDate).ToUnixTimeMilliseconds(); kind = $kind;
               workingSetBytes = $working; privateBytes = $private; crateName = $crateName; targetTriple = $targetTriple }
        }
    })
    return @{ kind = 'census'; startMs = $start; endMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds(); rows = $rows }
}
try {
    # Initialize CIM before Cargo starts. Full command lines remain transient here;
    # executable paths, user names and environments are never requested or emitted.
    $initial = Census
    $owner = @($initial.rows | Where-Object { $_.pid -eq $ObserverParentPid })
    if ($owner.Count -ne 1) { throw 'observer parent unavailable' }
    $created = $owner[0].createdMs
    Emit @{ kind = 'ready' }
    while (-not (Test-Path -LiteralPath $StopFile)) {
        $frame = Census
        $sameOwner = @($frame.rows | Where-Object { $_.pid -eq $ObserverParentPid -and $_.createdMs -eq $created })
        if ($sameOwner.Count -ne 1) { break }
        Emit $frame
        Start-Sleep -Milliseconds $IntervalMs
    }
} catch { Emit @{ kind = 'error'; code = 'cim-observer-failed' }; exit 1 }
