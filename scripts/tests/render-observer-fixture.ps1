$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../render-compile/process-identities.ps1')
$script:queries = @()
$base = [DateTime]::Parse('2020-01-01T00:00:00Z').ToUniversalTime()
function Row([int]$id, [int]$parent, [int]$seconds, [string]$name) {
    return @{ ProcessId = $id; ParentProcessId = $parent; CreationDate = $base.AddSeconds($seconds);
        Name = $name; WorkingSetSize = 100; PrivatePageCount = 200 }
}
$script:records = @((Row 1 0 0 'node.exe'), (Row 2 1 1 'cargo.exe'), (Row 3 7 3 'rustc.exe'),
    (Row 4 999 2 'rustc.exe'), (Row 5 2 2 'rustc.exe'), (Row 6 2 0 'rustc.exe'), (Row 7 2 2 'rustc.exe'))
function Get-CimInstance($ClassName, $Property, $Filter) {
    $script:queries += @{ properties = $Property; filter = $Filter }
    if (-not $Filter) {
        if ($Property -contains 'CommandLine' -or $Property -contains 'ExecutablePath') { throw 'Global command-line query refused' }
        return $script:records
    }
    if ($Filter -notmatch "^ProcessId = (3|5|7) AND CreationDate = '[0-9.\+\-]+'$") { throw 'Unproved process arguments requested' }
    $id = [int]$Matches[1]
    $result = (Row $id 2 2 'rustc.exe')
    if ($id -eq 3) { $result = (Row 3 7 3 'rustc.exe') }
    $result.ExecutablePath = 'C:\private-secret\toolchain\bin\rustc.exe'
    if ($id -eq 7) { $result.ExecutablePath = 'C:\private-secret\proxy\rustc.exe' }
    $result.CommandLine = 'rustc --crate-name rbit_render_probe --target x86_64-pc-windows-msvc private-secret-path'
    if ($id -eq 5) { $result.CreationDate = $base.AddSeconds(99) }
    return $result
}
$frame = Census
Add-OwnedRustcIdentifiers $frame 1 ([DateTimeOffset]$base).ToUnixTimeMilliseconds() 'C:\private-secret\toolchain\bin\rustc.exe'
$owned = @($frame.rows | Where-Object { $_.pid -eq 3 })[0]
if ($owned.crateName -ne 'rbit_render_probe' -or $owned.targetTriple -ne 'x86_64-pc-windows-msvc') { throw 'Owned identifiers missing' }
if (-not $owned.actualCompilerMatch) { throw 'Actual compiler not identified' }
$proxy = @($frame.rows | Where-Object { $_.pid -eq 7 })[0]
if ($proxy.crateName -ne $owned.crateName -or $proxy.actualCompilerMatch) { throw 'Proxy was accepted as actual compiler' }
if (@($frame.rows | Where-Object { $_.pid -ne 3 -and $_.pid -ne 7 -and $null -ne $_.crateName }).Count) { throw 'Unproved identifier escaped' }
if (@($frame.rows | Where-Object { $_.pid -ne 3 -and $_.actualCompilerMatch }).Count) { throw 'Unproved executable identity escaped' }
if ($script:queries.Count -ne 4) { throw 'Unexpected metadata or argument query count' }
if (($frame | ConvertTo-Json -Depth 5) -match 'private-secret') { throw 'Raw arguments escaped' }
'Query policy validated'
