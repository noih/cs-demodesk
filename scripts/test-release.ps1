# Offline release-flow tests. Git, gh, cargo and npm are mocked; nothing is published.
$ErrorActionPreference = 'Stop'
$source = $PSScriptRoot
$original = Get-Location
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('demodesk-release-test-' + [guid]::NewGuid())
$global:releaseCalls = [Collections.Generic.List[string]]::new()
$global:releaseFailure = ''
$global:releaseDraft = $true
$global:releaseTarget = 'abc123'
function Assert($condition, $message) { if (-not $condition) { throw $message } }
function git {
  $global:LASTEXITCODE = 0
  $call = $args -join ' '
  $global:releaseCalls.Add("git $call")
  switch -Wildcard ($call) {
    'branch --show-current' { 'main' }
    'remote get-url origin' { 'git@github-alias:owner/project.git' }
    'describe *' { 'v1.0.29' }
    'rev-parse *' { 'abc123' }
    'cat-file *' { 'BEGIN SSH SIGNATURE' }
  }
}
function gh {
  $global:LASTEXITCODE = 0
  $call = $args -join ' '
  $global:releaseCalls.Add("gh $call")
  if ($call.StartsWith('release create ')) {
    $index = [array]::IndexOf($args, '--notes-file')
    $global:releaseBody = [IO.File]::ReadAllText($args[$index + 1])
    if ($global:releaseFailure -eq 'create') { $global:LASTEXITCODE = 1 }
  } elseif ($call.StartsWith('release view ')) {
    if ($global:releaseFailure -eq 'view') { $global:LASTEXITCODE = 1; return }
    @{ isDraft=$global:releaseDraft; body='Keep these notes'; targetCommitish=$global:releaseTarget } | ConvertTo-Json
  } elseif ($call.StartsWith('release upload ') -and $global:releaseFailure -eq 'upload') {
    $global:LASTEXITCODE = 1
  }
}
function cargo { $global:LASTEXITCODE = 0 }
function npm { $global:LASTEXITCODE = 0 }
try {
  New-Item -ItemType Directory -Path "$fixture/scripts", "$fixture/src-tauri", "$fixture/dist-portable" | Out-Null
  foreach ($file in @('release.ps1','validate-store-version.ps1','test-store-version.ps1','publish-release.ps1')) {
    Copy-Item -LiteralPath "$source/$file" -Destination "$fixture/scripts/$file"
  }
  Set-Content "$fixture/Cargo.toml" 'version = "1.0.29"'
  Set-Content "$fixture/src-tauri/tauri.conf.json" '{"version": "1.0.29"}'
  Set-Content "$fixture/dist-portable/app.exe" 'fixture'
  Set-Content "$fixture/dist-portable/SHA256SUMS" 'checksum'
  $notes = "## What's New`n`nUnicode: $([char]0x756B)$([char]0x8CEA)`nLiteral: " + '$(throw "must not execute")'
  [IO.File]::WriteAllText("$fixture/notes with spaces.md", $notes, [Text.UTF8Encoding]::new($false))
  Set-Location $fixture
  & "$fixture/scripts/release.ps1" 1.3.0 -NotesFile './notes with spaces.md'
  Assert ($global:releaseBody.StartsWith($notes)) 'Notes changed or were interpreted as commands.'
  Assert ($global:releaseBody.Contains('owner/project/compare/v1.0.29...v1.3.0')) 'Changelog link missing.'
  $draft = @($global:releaseCalls | Where-Object { $_ -like 'gh release create *' })[0]
  Assert ($global:releaseCalls.IndexOf($draft) -lt $global:releaseCalls.IndexOf('git push origin v1.3.0')) 'Tag pushed before draft.'
  Assert (-not ($global:releaseCalls | Where-Object { $_ -like 'git add *notes*' })) 'Notes staged in Git.'
  foreach ($failure in @('missing','empty','create')) {
    $global:releaseCalls.Clear()
    $global:releaseFailure = $failure
    $path = "$fixture/notes with spaces.md"
    if ($failure -eq 'missing') { $path = "$fixture/missing.md" }
    if ($failure -eq 'empty') { $path = "$fixture/empty.md"; Set-Content $path ' ' }
    $rejected = $false
    try { & "$fixture/scripts/release.ps1" 1.3.0 -NotesFile $path } catch { $rejected = $true }
    Assert $rejected "Accepted $failure notes/draft failure."
    Assert (-not $global:releaseCalls.Contains('git push origin v1.3.0')) 'Pushed tag after failure.'
  }
  foreach ($scenario in @('check','publish','upload','view','target','published')) {
    $global:releaseCalls.Clear()
    $global:releaseFailure = $scenario
    $global:releaseDraft = $scenario -ne 'published'
    $global:releaseTarget = if ($scenario -eq 'target') { 'wrong' } else { 'abc123' }
    $rejected = $false
    try { & "$fixture/scripts/publish-release.ps1" -Tag v1.3.0 -Repository owner/project -Commit abc123 -CheckOnly:($scenario -eq 'check') } catch { $rejected = $true }
    Assert ($rejected -eq ($scenario -in @('upload','view','target'))) "Unexpected outcome: $scenario"
    $edits = @($global:releaseCalls | Where-Object { $_ -like 'gh release edit *' })
    Assert ($edits.Count -eq [int]($scenario -eq 'publish')) "Published in wrong scenario: $scenario"
    Assert (-not ($global:releaseCalls | Where-Object { $_ -like '*--notes*' })) 'Workflow rewrote notes.'
  }
  Write-Output 'Release flow tests passed (offline; no GitHub writes).'
} finally {
  Set-Location $original
  # Only remove the unique temporary fixture created above.
  Remove-Item -LiteralPath $fixture -Recurse -Force
}
