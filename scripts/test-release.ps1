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
    'ls-remote *' { if ($global:releaseFailure -ne 'missing-tag') { ('1' * 40) + "`trefs/tags/v1.3.0" } }
    'verify-commit *' { if ($global:releaseFailure -eq 'unsigned') { $global:LASTEXITCODE = 1 } }
    'tag -f -s *' { if ($global:releaseFailure -eq 'signing') { $global:LASTEXITCODE = 1 } }
    'push --force-with-lease=*' { if ($global:releaseFailure -eq 'lease') { $global:LASTEXITCODE = 1 } }
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
    $global:releaseViews++
    $draft = $global:releaseDraft -and -not ($global:releaseFailure -eq 'published-during-checks' -and $global:releaseViews -gt 1)
    @{ isDraft=$draft; body='Keep these notes'; targetCommitish=$global:releaseTarget } | ConvertTo-Json
  } elseif ($call.StartsWith('release edit ') -and $global:releaseFailure -eq 'edit') {
    $global:LASTEXITCODE = 1
  } elseif ($call.StartsWith('release upload ') -and $global:releaseFailure -eq 'upload') {
    $global:LASTEXITCODE = 1
  }
}
function cargo { $global:LASTEXITCODE = 0 }
function npm {
  $global:LASTEXITCODE = 0
  $call = $args -join ' '
  $global:releaseCalls.Add("npm $call")
  if ($call -eq 'run test:privacy' -and $global:releaseFailure -eq 'privacy') { $global:LASTEXITCODE = 1 }
}
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
  foreach ($failure in @('missing','empty','create','privacy')) {
    $global:releaseCalls.Clear()
    $global:releaseFailure = $failure
    $path = "$fixture/notes with spaces.md"
    if ($failure -eq 'missing') { $path = "$fixture/missing.md" }
    if ($failure -eq 'empty') { $path = "$fixture/empty.md"; Set-Content $path ' ' }
    $rejected = $false
    try { & "$fixture/scripts/release.ps1" 1.3.0 -NotesFile $path } catch { $rejected = $true }
    Assert $rejected "Accepted $failure notes/draft failure."
    Assert (-not $global:releaseCalls.Contains('git push origin v1.3.0')) 'Pushed tag after failure.'
    if ($failure -eq 'privacy') {
      Assert (-not ($global:releaseCalls | Where-Object { $_ -like 'npm version *' -or $_ -like 'git push *' -or $_ -like 'git commit *' -or $_ -like 'git tag -s *' })) 'Privacy failure changed the version or published a release.'
    }
  }
  foreach ($scenario in @('retry','published','view','version','missing-tag','privacy','unsigned','signing','edit','lease','published-during-checks')) {
    $global:releaseCalls.Clear()
    $global:releaseFailure = $scenario
    $global:releaseDraft = $scenario -ne 'published'
    $global:releaseViews = 0
    Set-Content "$fixture/package.json" ('{"version":"' + $(if ($scenario -eq 'version') { '1.0.29' } else { '1.3.0' }) + '"}')
    Set-Content "$fixture/Cargo.toml" 'version = "1.3.0"'
    Set-Content "$fixture/src-tauri/tauri.conf.json" '{"version":"1.3.0"}'
    $rejected = $false
    try { & "$fixture/scripts/release.ps1" 1.3.0 -RetryDraft } catch { $rejected = $true }
    Assert ($rejected -eq ($scenario -ne 'retry')) "Unexpected retry outcome: $scenario"
    Assert (-not ($global:releaseCalls | Where-Object { $_ -like 'npm version *' -or $_ -like 'git commit *' -or $_ -like 'git add *' -or $_ -eq 'git push origin main' -or $_ -like '*--notes*' -or $_ -like 'gh release create *' })) 'Retry bumped versions, committed, or replaced notes.'
    $push = 'git push --force-with-lease=refs/tags/v1.3.0:' + ('1' * 40) + ' origin refs/tags/v1.3.0'
    Assert ($global:releaseCalls.Contains($push) -eq ($scenario -in @('retry','lease'))) "Unsafe tag push: $scenario"
    if ($scenario -eq 'retry') {
      Assert ($global:releaseCalls.IndexOf('git verify-commit abc123') -lt $global:releaseCalls.IndexOf('git tag -f -s v1.3.0 -m 1.3.0')) 'Tag created before verifying commit.'
      Assert ($global:releaseCalls.IndexOf('gh release edit v1.3.0 --repo owner/project --target abc123') -lt $global:releaseCalls.IndexOf($push)) 'Tag pushed before draft update.'
    }
    if ($scenario -in @('published','view','version','missing-tag','privacy','unsigned','published-during-checks')) {
      Assert (-not ($global:releaseCalls | Where-Object { $_ -like 'git tag -f *' -or $_ -like 'gh release edit *' })) "Retry mutated a rejected release: $scenario"
    }
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
