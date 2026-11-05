param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [Parameter(Mandatory=$true)][string]$ArgumentsBase64,
    [Parameter(Mandatory=$true)][string]$Directory,
    [Parameter(Mandatory=$true)][string]$StopFile,
    [Parameter(Mandatory=$true)][string]$ProofFile,
    [Parameter(Mandatory=$true)][string]$Token
)
$ErrorActionPreference = 'Stop'
$entered = $false
try {
    Add-Type -LiteralPath (Join-Path $PSScriptRoot 'job-launcher.cs')
    $decoded = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($ArgumentsBase64))
    [string[]]$targetArguments = ConvertFrom-Json -InputObject $decoded
    if ($null -eq $targetArguments) { $targetArguments = [string[]]@() }
    $entered = $true
    $result = [WorkspaceJobLauncher]::Run($Executable, $targetArguments, $Directory, $StopFile, $ProofFile, $Token)
    exit $result
} catch {
    # Compilation/argument decoding failure launched no target. Native launcher failures
    # acknowledge cleanup themselves only after their process/job handles are verified.
    if (-not $entered) { [IO.File]::WriteAllText($ProofFile, $Token) }
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 125
}
