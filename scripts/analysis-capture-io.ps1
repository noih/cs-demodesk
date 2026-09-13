# Capture storage helpers; no process launch or dynamic code loading.
$utf8=[Text.UTF8Encoding]::new($false)
function Save-New([string]$file,$value){
    $bytes=$utf8.GetBytes(($value|ConvertTo-Json -Depth 12 -Compress))
    $temporary=$file+'.pending-'+[Guid]::NewGuid().ToString('N')
    $stream=[IO.File]::Open($temporary,'CreateNew','Write','None')
    try{$stream.Write($bytes,0,$bytes.Length);$stream.Flush($true)}finally{$stream.Dispose()}
    [IO.File]::Move($temporary,$file)
}
function Open-Log([string]$path){
    $script:outputFile=[IO.File]::Open($path,'CreateNew','Write','Read')
    $script:output=[IO.Compression.GZipStream]::new($script:outputFile,[IO.Compression.CompressionLevel]::Optimal,$true)
    $script:outputBytes=0L
}
function Close-Log {
    if($script:output){$script:output.Dispose();$script:output=$null}
    if($script:outputFile){try{$script:outputFile.Flush($true)}finally{$script:outputFile.Dispose();$script:outputFile=$null}}
}
