# Cuts a release: bumps the version everywhere, runs the checks, makes a signed
# commit and tag, and pushes. GitHub Actions (.github/workflows/release.yml)
# then runs npm run app:release, attests both EXE/MSIX, publishes the EXE,
# and saves the MSIX as an Actions artifact for Store submission.
#
#   .\scripts\release.ps1 1.0.0
#
# Requires: git with commit/tag signing configured, cargo, node, a clean
# working tree on main, and `origin` pointing at GitHub.

param(
  [Parameter(Mandatory = $true, Position = 0)]
  [ValidatePattern('^\d+\.\d+\.\d+$')]
  [string]$Version
)

$ErrorActionPreference = 'Stop'
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

Step 'Checking the working tree'
if ((git branch --show-current) -ne 'main') { throw 'switch to main first' }
if (git status --porcelain) { throw 'working tree is not clean' }
if (git tag -l $tag) { throw "tag $tag already exists" }
Run 'git fetch origin --tags'
if ((git rev-parse HEAD) -ne (git rev-parse origin/main)) { throw 'main is not in sync with origin/main' }

Step "Bumping version to $Version"
# workspace crates
Set-Text Cargo.toml ((Get-Content Cargo.toml -Raw) -replace '(?m)^version = "[^"]+"', "version = `"$Version`"")
Run 'cargo update --workspace --offline'
# frontend + tauri
Run "npm version $Version --no-git-tag-version --allow-same-version"
Set-Text src-tauri/tauri.conf.json ((Get-Content src-tauri/tauri.conf.json -Raw) -replace '"version": "[^"]+"', "`"version`": `"$Version`"")

Step 'Running checks'
& "$PSScriptRoot/test-store-version.ps1"
Run 'cargo test -p demodesk-core'
Run 'npm run build'
Run 'cargo test -p demodesk --lib data_directory::tests'

Step 'Committing and tagging (signed)'
Run 'git add Cargo.toml Cargo.lock package.json package-lock.json src-tauri/tauri.conf.json'
if (git status --porcelain) { Run "git commit -m `"Release $Version`"" } else { Write-Host '    version already up to date, tagging HEAD' }
Run "git tag -s $tag -m `"$Version`""
if (-not (git cat-file tag $tag | Select-String -Quiet 'BEGIN (SSH|PGP) SIGNATURE')) { throw "tag $tag is not signed - configure git signing first" }

Step 'Pushing'
Run 'git push origin main'
Run "git push origin $tag"

$repo = (git remote get-url origin) -replace '^.*github\.com[:/]', '' -replace '\.git$', ''
Write-Host ''
Write-Host "Pushed $tag. Build and release: https://github.com/$repo/actions" -ForegroundColor Green
