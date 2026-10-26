# Cuts a release: bumps the version everywhere, runs the checks, makes a signed
# commit and tag, and pushes. GitHub Actions (.github/workflows/release.yml)
# then runs npm run app:release, attests both EXE/MSIX, publishes the EXE,
# and saves the MSIX as an Actions artifact for Store submission.
#
#   .\scripts\release.ps1 1.3.0 -NotesFile out/release-notes/1.3.0.md
#
# Requires: authenticated GitHub CLI (gh), git with commit/tag signing configured,
# cargo, node, a clean
# working tree on main, and `origin` pointing at GitHub.

param(
  [Parameter(Mandatory = $true, Position = 0)]
  [ValidatePattern('^\d+\.\d+\.\d+$')]
  [string]$Version,
  [string]$NotesFile,
  [switch]$RetryDraft
)

$ErrorActionPreference = 'Stop'
if ($RetryDraft) {
  if ($NotesFile) { throw '-RetryDraft preserves the existing release notes; omit -NotesFile.' }
} else {
  if (-not $NotesFile -or -not (Test-Path -LiteralPath $NotesFile -PathType Leaf)) {
    throw 'Provide a Markdown file with -NotesFile (for example out/release-notes/1.3.0.md).'
  }
  $notes = [IO.File]::ReadAllText((Resolve-Path -LiteralPath $NotesFile).Path, [Text.Encoding]::UTF8).Trim()
  if ([string]::IsNullOrWhiteSpace($notes)) { throw 'Release notes must not be empty.' }
}
Set-Location (Join-Path $PSScriptRoot '..')

function Step($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Run($cmd) {
  Write-Host "    $cmd" -ForegroundColor DarkGray
  Invoke-Expression $cmd
  if ($LASTEXITCODE -ne 0) { throw "failed: $cmd" }
}
# UTF-8 without BOM regardless of the PowerShell version
function Set-Text($path, $text) { [IO.File]::WriteAllText((Resolve-Path $path), $text, [Text.UTF8Encoding]::new($false)) }

& "$PSScriptRoot/validate-store-version.ps1" $Version
$tag = "v$Version"

Step 'Checking GitHub access'
Get-Command gh -ErrorAction Stop | Out-Null
Run 'gh auth status --hostname github.com'
$remote = git remote get-url origin
if ($LASTEXITCODE -ne 0 -or $remote -notmatch '[:/]([^/:]+/[^/]+?)(?:\.git)?$') { throw 'Cannot resolve origin repository.' }
$repo = $Matches[1]
& gh repo view $repo --json nameWithOwner --jq .nameWithOwner
if ($LASTEXITCODE -ne 0) { throw 'Cannot access the GitHub repository.' }

Step 'Checking the working tree'
if ((git branch --show-current) -ne 'main') { throw 'switch to main first' }
if (git status --porcelain) { throw 'working tree is not clean' }
if ($RetryDraft) {
  Run 'git fetch origin --no-tags'
} else {
  if (git tag -l $tag) { throw "tag $tag already exists" }
  Run 'git fetch origin --tags'
  if (git tag -l $tag) { throw "tag $tag already exists on origin" }
  $previousTag = git describe --tags --abbrev=0
  if ($LASTEXITCODE -ne 0) { throw 'Cannot determine the previous release tag.' }
}
if ((git rev-parse HEAD) -ne (git rev-parse origin/main)) { throw 'main is not in sync with origin/main' }

function Assert-Draft {
  $json = & gh release view $tag --repo $repo --json isDraft,body,targetCommitish
  if ($LASTEXITCODE -ne 0) { throw "Cannot read existing draft $tag." }
  $release = $json | ConvertFrom-Json
  if ($release.isDraft -ne $true) { throw "Release $tag is published; refusing to replace its tag." }
  if ([string]::IsNullOrWhiteSpace($release.body)) { throw "Draft $tag has no release notes." }
}
if ($RetryDraft) {
  Step 'Checking the existing draft'
  Assert-Draft
  $packageVersion = (Get-Content package.json -Raw | ConvertFrom-Json).version
  $tauriVersion = (Get-Content src-tauri/tauri.conf.json -Raw | ConvertFrom-Json).version
  $cargoVersion = [regex]::Match((Get-Content Cargo.toml -Raw), '(?m)^version = "([^"\r\n]+)"').Groups[1].Value
  if ($packageVersion -ne $Version -or $tauriVersion -ne $Version -or $cargoVersion -ne $Version) {
    throw 'Retry requires all project versions to match the existing release.'
  }
  $remoteTag = & git ls-remote --refs origin "refs/tags/$tag"
  $tagMatch = [regex]::Match([string]$remoteTag, '^([0-9a-f]{40,64})\s+refs/tags/')
  if ($LASTEXITCODE -ne 0 -or -not $tagMatch.Success) {
    throw "Cannot resolve the remote tag $tag."
  }
  $expectedTag = $tagMatch.Groups[1].Value
}

Step 'Checking repository privacy'
Run 'npm run test:privacy'

if (-not $RetryDraft) {
  Step "Bumping version to $Version"
  # workspace crates
  Set-Text Cargo.toml ((Get-Content Cargo.toml -Raw) -replace '(?m)^version = "[^"]+"', "version = `"$Version`"")
  Run 'cargo update --workspace --offline'
  # frontend + tauri
  Run "npm version $Version --no-git-tag-version --allow-same-version"
  Set-Text src-tauri/tauri.conf.json ((Get-Content src-tauri/tauri.conf.json -Raw) -replace '"version": "[^"]+"', "`"version`": `"$Version`"")

}

Step 'Running checks'
& "$PSScriptRoot/test-store-version.ps1"
Run 'cargo test -p demodesk-core'
Run 'npm run build'
Run 'cargo test -p demodesk --lib data_directory::tests'

if ($RetryDraft) {
  Step 'Rechecking the draft before replacing its tag'
  Assert-Draft
  if (git status --porcelain) { throw 'Checks changed tracked files; commit them before retrying.' }
  $commit = git rev-parse HEAD
  if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve release commit.' }
  Run "git verify-commit $commit"
  Run "git tag -f -s $tag -m `"$Version`""
  Run "git verify-tag $tag"
  & gh release edit $tag --repo $repo --target $commit
  if ($LASTEXITCODE -ne 0) { throw 'Draft update failed; the remote tag was not changed.' }
  # Bind the lease to the exact remote tag object observed before the checks.
  Run "git push --force-with-lease=refs/tags/${tag}:$expectedTag origin refs/tags/$tag"
  Write-Host "Retried $tag. Build and release: https://github.com/$repo/actions" -ForegroundColor Green
  return
}

Step 'Committing and tagging (signed)'
Run 'git add Cargo.toml Cargo.lock package.json package-lock.json src-tauri/tauri.conf.json'
if (git status --porcelain) { Run "git commit -m `"Release $Version`"" } else { Write-Host '    version already up to date, tagging HEAD' }
Run "git tag -s $tag -m `"$Version`""
if (-not (git cat-file tag $tag | Select-String -Quiet 'BEGIN (SSH|PGP) SIGNATURE')) { throw "tag $tag is not signed - configure git signing first" }

Step 'Pushing the release commit'
Run 'git push origin main'
$commit = git rev-parse HEAD
if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve release commit.' }

Step 'Creating draft release'
$body = @"
$notes

---

The .exe is the standalone download.
Microsoft Store: https://apps.microsoft.com/detail/9N5G4VXSDGS5

**Full Changelog**: https://github.com/$repo/compare/$previousTag...$tag
"@
$tempNotes = [IO.Path]::GetTempFileName()
try {
  [IO.File]::WriteAllText($tempNotes, $body, [Text.UTF8Encoding]::new($false))
  & gh release create $tag --repo $repo --draft --target $commit --title $tag --notes-file $tempNotes
  if ($LASTEXITCODE -ne 0) { throw "Draft creation failed; the tag has not been pushed. See scripts/README.md for recovery." }
} finally {
  Remove-Item -LiteralPath $tempNotes -ErrorAction SilentlyContinue
}
Step 'Pushing the signed tag to start the build'
Run "git push origin $tag"
Write-Host ''
Write-Host "Pushed $tag. Build and release: https://github.com/$repo/actions" -ForegroundColor Green
