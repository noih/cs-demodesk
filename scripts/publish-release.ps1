# Called by Actions after a signed tag is pushed; never creates a release or changes its notes.
param(
  [Parameter(Mandatory = $true)][string]$Tag,
  [Parameter(Mandatory = $true)][string]$Repository,
  [Parameter(Mandatory = $true)][string]$Commit,
  [switch]$CheckOnly
)
$ErrorActionPreference = 'Stop'
$json = & gh release view $Tag --repo $Repository --json isDraft,body,targetCommitish
if ($LASTEXITCODE -ne 0) { throw 'Prepared release not found. Create the draft with release.ps1 first.' }
$release = $json | ConvertFrom-Json
if ([string]::IsNullOrWhiteSpace($release.body)) { throw 'Release notes are missing.' }
if ($release.targetCommitish -ne $Commit) { throw 'Release draft targets a different commit.' }
if ($CheckOnly) { return }
# A rerun after successful publication must not replace published assets.
if (-not $release.isDraft) { Write-Output 'Release already published; leaving it unchanged.'; return }
$assets = @(Get-ChildItem -LiteralPath dist-portable -Filter '*.exe' -File)
if ($assets.Count -ne 1) { throw 'Expected one portable executable.' }
$checksums = Get-Item -LiteralPath dist-portable/SHA256SUMS -ErrorAction Stop
& gh release upload $Tag --repo $Repository $assets[0].FullName $checksums.FullName --clobber
if ($LASTEXITCODE -ne 0) { throw 'Asset upload failed; release remains a draft.' }
& gh release edit $Tag --repo $Repository --draft=false --verify-tag
if ($LASTEXITCODE -ne 0) { throw 'Could not publish the prepared release. Retry the workflow.' }
