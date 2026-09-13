$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'analysis-capture-io.ps1')
$root=Join-Path ([IO.Path]::GetTempPath()) ('demodesk-checkpoint-'+[Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($root)|Out-Null
try {
    $file=Join-Path $root 'checkpoint.json'
    Save-New $file @{value=1}
    $before=[IO.File]::ReadAllText($file)
    $rejected=$false;try{Save-New $file @{value=2}}catch{$rejected=$true}
    if(!$rejected -or $before -ne [IO.File]::ReadAllText($file)){throw 'Checkpoint was overwritten'}
    if((Get-Content -LiteralPath $file -Raw|ConvertFrom-Json).value -ne 1){throw 'Checkpoint is incomplete'}
    'Checkpoint publication is complete and preserves prior results.'
} finally {
    $resolved=[IO.Path]::GetFullPath($root)
    if($resolved.StartsWith([IO.Path]::GetFullPath([IO.Path]::GetTempPath()),[StringComparison]::OrdinalIgnoreCase) -and [IO.Path]::GetFileName($resolved).StartsWith('demodesk-checkpoint-')){Remove-Item -LiteralPath $resolved -Recurse -Force}
}
