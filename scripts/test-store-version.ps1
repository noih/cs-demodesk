$ErrorActionPreference = 'Stop'
foreach ($version in @('1.0.0', '1.0.12345', '65535.65535.65535')) {
    & "$PSScriptRoot/validate-store-version.ps1" $version
}
foreach ($version in @('0.1.0', '1.65536.0', '1.2345667.0', '1.0.999999999999999999999', '1.02.0', '1.0', '1.0.0.0', '1.0.0-beta')) {
    $rejected = $false
    try { & "$PSScriptRoot/validate-store-version.ps1" $version } catch { $rejected = $true }
    if (-not $rejected) { throw "Invalid Store version accepted: $version" }
}
Write-Output 'Store version validation passed.'
