# Only metadata is enumerated globally. Query arguments for a proved rustc identity.
function Census {
    $start = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    $script:CreationDates = @{}
    $rows = @(Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,CreationDate,Name,WorkingSetSize,PrivatePageCount | ForEach-Object {
        if ($null -ne $_.CreationDate) {
            $script:CreationDates[[int]$_.ProcessId] = $_.CreationDate
            $kind = 'other'
            if ($_.Name -ieq 'cargo.exe') { $kind = 'cargo' }
            if ($_.Name -ieq 'rustc.exe') { $kind = 'rustc' }
            $working = $null; $private = $null
            if ($kind -ne 'other') {
                if ($null -ne $_.WorkingSetSize) { $working = [long]$_.WorkingSetSize }
                if ($null -ne $_.PrivatePageCount) { $private = [long]$_.PrivatePageCount }
            }
            @{ pid = [int]$_.ProcessId; parentPid = [int]$_.ParentProcessId;
               createdMs = ([DateTimeOffset]$_.CreationDate).ToUnixTimeMilliseconds(); kind = $kind;
               workingSetBytes = $working; privateBytes = $private; crateName = $null; targetTriple = $null }
        }
    })
    return @{ kind = 'census'; startMs = $start; endMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds(); rows = $rows }
}

function Add-OwnedRustcIdentifiers($frame, [int]$ownerPid, [long]$ownerCreated) {
    $byPid = @{}; foreach ($row in $frame.rows) { $byPid[$row.pid] = $row }
    foreach ($row in $frame.rows) {
        if ($row.kind -ne 'rustc') { continue }
        $cursor = $row; $seen = @{}; $cargo = $false; $proved = $false
        while ($null -ne $cursor -and -not $seen.ContainsKey($cursor.pid)) {
            $seen[$cursor.pid] = $true
            if ($cursor.kind -eq 'cargo') { $cargo = $true }
            if ($cursor.pid -eq $ownerPid) { $proved = $cargo -and $cursor.createdMs -eq $ownerCreated; break }
            $parent = $byPid[$cursor.parentPid]
            if ($null -eq $parent -or $parent.createdMs -gt $cursor.createdMs) { break }
            $cursor = $parent
        }
        if (-not $proved -or -not $script:CreationDates.ContainsKey($row.pid)) { continue }
        # Include creation time in the provider filter, BEFORE requesting CommandLine:
        # a reused PID must not expose an unrelated application's arguments.
        $date = [System.Management.ManagementDateTimeConverter]::ToDmtfDateTime($script:CreationDates[$row.pid])
        $filter = "ProcessId = $($row.pid) AND CreationDate = '$date'"
        $target = @(Get-CimInstance Win32_Process -Filter $filter -Property ProcessId,ParentProcessId,CreationDate,CommandLine)
        if ($target.Count -ne 1 -or $target[0].ProcessId -ne $row.pid -or $target[0].ParentProcessId -ne $row.parentPid -or
            ([DateTimeOffset]$target[0].CreationDate).ToUnixTimeMilliseconds() -ne $row.createdMs) { continue }
        $arguments = $target[0].CommandLine
        if ($arguments -match '(?:^|\s)--crate-name(?:=|\s+)"?([A-Za-z_][A-Za-z0-9_]*)"?(?:\s|$)') { $row.crateName = $Matches[1] }
        if ($arguments -match '(?:^|\s)--target(?:=|\s+)"?([A-Za-z0-9_-]+)"?(?:\s|$)') { $row.targetTriple = $Matches[1] }
    }
}
