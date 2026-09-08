#requires -Version 5.1
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'compatibility-report.ps1')
function Assert($Condition,[string]$Message) { if (!$Condition) { throw $Message } }
$before=[pscustomobject]@{schemaVersion=1; startupPassed=$true; windowsVersion='Windows'; steamBuildId='1';
    files=@{cs2=@{sha256='old'}}; hook=(Get-HookSummary "installed`ncreate class=SDL_app`nintercepted SetFocus SDL_app")}
$after=[pscustomobject]@{schemaVersion=1; startupPassed=$true; windowsVersion='Windows'; steamBuildId='2';
    files=@{cs2=@{sha256='new'}}; hook=(Get-HookSummary "installed`ncreate class=NewGame`nintercepted SetCursorPos")}
$diff=@(Compare-Compatibility $before $after)
Assert ($diff -match 'cs2 SHA256') 'Missed binary change'
Assert ($diff -contains 'classes added: NewGame') 'Missed class change'
Assert ($diff -contains 'interceptions not observed this run: intercepted SetFocus SDL_app') 'Missing call incorrectly ignored'
Assert (@(Compare-Compatibility $before $before).Count -eq 0) 'Identical report produced differences'
Assert (!(Get-HookSummary 'installation failed').installed) 'Installation failure was accepted'
$testRoot=Join-Path (Split-Path $PSScriptRoot) 'target/compatibility-tests'
[IO.Directory]::CreateDirectory($testRoot) | Out-Null
$baseline=Join-Path $testRoot 'baseline.json'
Save-CompatibilityBaseline $before $baseline
$bytes=[IO.File]::ReadAllText($baseline)
$failed=[pscustomobject]@{schemaVersion=1; startupPassed=$false}
$rejected=$false
try { Save-CompatibilityBaseline $failed $baseline } catch { $rejected=$true }
Assert $rejected 'Failed scan replaced baseline'
Assert ([IO.File]::ReadAllText($baseline) -eq $bytes) 'Baseline changed after rejection'
Save-CompatibilityBaseline $after $baseline
$saved=Get-Content -LiteralPath $baseline -Raw | ConvertFrom-Json
Assert ($saved.files.cs2.sha256 -eq 'new') 'Baseline replacement failed'
Assert ($null -eq $saved.windows) 'Baseline retained detailed logs'
Assert (!(Test-Path -LiteralPath ($baseline+'.pending'))) 'Pending baseline left behind'
'Compatibility report tests passed'
$shared=Join-Path $testRoot 'active-hook.log'
$writer=[IO.File]::Open($shared,'Create','Write','ReadWrite')
try {
    $data=[Text.Encoding]::UTF8.GetBytes('installed')
    $writer.Write($data,0,$data.Length); $writer.Flush()
    Assert ((Read-SharedHookLog $shared) -eq 'installed') 'Cannot read active hook log'
} finally { $writer.Dispose() }
$scanRoot=Join-Path $testRoot 'scan'
foreach ($attempt in 1..2) {
    try { & (Join-Path $PSScriptRoot 'check-cs2-compatibility.ps1') -OutputDir $scanRoot -Cs2 (Join-Path $testRoot 'missing.exe') } catch {}
    $failedReport=Get-Content -LiteralPath (Join-Path $scanRoot 'latest/report.json') -Raw | ConvertFrom-Json
    Assert (!$failedReport.startupPassed -and $failedReport.errors.Count -gt 0) 'Failure report not retained'
    if ($attempt -eq 1) { [IO.File]::WriteAllText((Join-Path $scanRoot 'latest/stale.txt'),'stale') }
}
Assert (!(Test-Path -LiteralPath (Join-Path $scanRoot 'latest/stale.txt'))) 'Latest scan accumulated stale artifacts'
'Active log sharing and failure/retention tests passed'
