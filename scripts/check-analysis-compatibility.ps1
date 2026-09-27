param(
    [Parameter(Mandatory = $true)][string]$State,
    [Parameter(Mandatory = $true)][string]$Game,
    [Parameter(Mandatory = $true)][string]$Vrf,
    [Parameter(Mandatory = $true)][string]$Cache,
    [string]$Assessments,
    [string]$Baseline,
    [string]$Report = 'out/analysis-compatibility/latest.json'
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
try {
    foreach ($path in @($State, $Game, $Vrf)) {
        if (-not (Test-Path -LiteralPath $path)) {
            Write-Output ('{"status":"not-run","missing":"' + $path.Replace('\','/') + '"}')
            exit 2
        }
    }
    $output = [IO.Path]::GetFullPath($Report)
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($output)) | Out-Null
    $log = [IO.Path]::ChangeExtension($output, '.log')
    $ErrorActionPreference = 'Continue'
    & cargo run -p demodesk-core --example analysis_native_check -- $State $Game $Vrf $Cache --diagnose $output *> $log
    $ErrorActionPreference = 'Stop'
    if ($LASTEXITCODE -ne 0) {
        Get-Content -LiteralPath $log -Tail 12 | Write-Output
        exit $LASTEXITCODE
    }
    $arguments = @('scripts/check-analysis-compatibility.mjs', $output)
    if ($Assessments) { $arguments += @('--assessments', $Assessments) }
    if ($Baseline) { $arguments += @('--baseline', $Baseline) }
    & node @arguments
    exit $LASTEXITCODE
} finally {
    Pop-Location
}
