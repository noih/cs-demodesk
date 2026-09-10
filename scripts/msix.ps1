# Build an unsigned x64 Store submission. Microsoft signs it after certification.
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
$version = (Get-Content package.json -Raw | ConvertFrom-Json).version
& "$PSScriptRoot/validate-store-version.ps1" $version
$sdk = Get-ChildItem "${env:ProgramFiles(x86)}/Windows Kits/10/bin/*/x64/MakeAppx.exe" |
    Sort-Object { [version]$_.Directory.Parent.Name } -Descending | Select-Object -First 1
if (-not $sdk) { throw 'Install the Windows 10/11 SDK (MakeAppx.exe is required).' }
& npm.cmd run tauri -- build --no-bundle --target x86_64-pc-windows-msvc
if ($LASTEXITCODE -ne 0) { throw 'Tauri MSIX build failed.' }

$output = Join-Path (Get-Location) 'out/msix'
$stage = Join-Path $output ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path "$stage/Assets" -Force | Out-Null
Copy-Item -LiteralPath 'target/x86_64-pc-windows-msvc/release/demodesk.exe' -Destination "$stage/demodesk.exe"
[xml]$manifest = Get-Content packaging/msix/AppxManifest.xml -Raw
$manifest.Package.Identity.Version = "$version.0"
$manifest.Save("$stage/AppxManifest.xml")
Add-Type -AssemblyName System.Drawing
$source = [System.Drawing.Image]::FromFile((Join-Path (Get-Location) 'src-tauri/icons/icon.png'))
try {
    foreach ($asset in @(@('StoreLogo', 50), @('Square44x44Logo', 44), @('Square150x150Logo', 150))) {
        $bitmap = New-Object System.Drawing.Bitmap($asset[1], $asset[1])
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $graphics.DrawImage($source, 0, 0, $asset[1], $asset[1])
            $bitmap.Save("$stage/Assets/$($asset[0]).png", [System.Drawing.Imaging.ImageFormat]::Png)
        } finally { $graphics.Dispose(); $bitmap.Dispose() }
    }
} finally { $source.Dispose() }
$package = Join-Path $output "CS-DemoDesk-$version-x64.msix"
& $sdk.FullName pack /d $stage /p $package /o
if ($LASTEXITCODE -ne 0) { throw 'MSIX validation or packaging failed.' }
Write-Output "Unsigned Store package: $package"
