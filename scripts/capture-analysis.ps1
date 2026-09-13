#requires -Version 5.1
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Demo,
    [Parameter(Mandatory)][string]$Context,
    [Parameter(Mandatory)][string]$Cs2,
    [Parameter(Mandatory)][string]$Hlae,
    [Parameter(Mandatory)][string]$Hook,
    [Parameter(Mandatory)][string]$OutputDir,
    [Parameter(Mandatory)][string[]]$Attachments,
    [int]$FirstTick=-1,[int]$LastTick=-1,
    [ValidateRange(64,4096)][int]$SegmentTicks=4096,
    [ValidateRange(-1,4096)][int]$FrameCap=-1
)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'analysis-capture-io.ps1')
foreach($value in @($Demo,$Context,$Cs2,$Hlae,$Hook,$OutputDir)) { if($value -match '[";\r\n]'){throw 'Unsupported command path characters'} }
foreach($name in $Attachments){if($name -notmatch '^[a-zA-Z0-9_]{1,32}$'){throw 'Invalid attachment name'}}
if(!$Attachments.Count -or $Attachments.Count -gt 32 -or @($Attachments|Select-Object -Unique).Count -ne $Attachments.Count){throw 'Expected 1-32 unique attachments'}
$Demo=(Resolve-Path -LiteralPath $Demo).Path;$Cs2=(Resolve-Path -LiteralPath $Cs2).Path;$Hlae=(Resolve-Path -LiteralPath $Hlae).Path;$Hook=(Resolve-Path -LiteralPath $Hook).Path
$afx=Join-Path (Split-Path $Hlae) 'x64/AfxHookSource2.dll'
$source=Get-Content -LiteralPath $Context -Raw | ConvertFrom-Json
if($source.contract.module -ne 'match-context' -or $source.contract.schemaVersion -ne 1){throw 'Expected match-context v1'}
if($source.source.demoFingerprint -ne ('sha1:'+(Get-FileHash -LiteralPath $Demo -Algorithm SHA1).Hash.ToLowerInvariant())){throw 'Demo/context fingerprint mismatch'}
$rate=[double]$source.data.tickRate
if([double]::IsNaN($rate) -or [double]::IsInfinity($rate) -or $rate -le 0 -or $rate -gt 1024){throw 'Invalid source clock'}
if($FirstTick -lt 0){$FirstTick=($source.data.rounds.freezeEndTick|Measure-Object -Minimum).Minimum}
if($LastTick -lt 0){$LastTick=($source.data.rounds.endTick|Measure-Object -Maximum).Maximum}
if($FirstTick -lt 1 -or $LastTick -lt $FirstTick){throw 'Invalid capture range'}
$root=[IO.Path]::GetFullPath($OutputDir);[IO.Directory]::CreateDirectory($root)|Out-Null
$lock=[IO.File]::Open((Join-Path $root '.lock'),'OpenOrCreate','ReadWrite','None')
function Digest([string]$file){return 'sha256:'+(Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()}
function Validate-Capture([string]$file,[int]$first,[int]$last) {
    $validation=& node (Join-Path $PSScriptRoot 'analyze-attachment-capture.mjs') --validate $file
    if($LASTEXITCODE -ne 0){throw 'Capture validation failed'}
    $verified=$validation|ConvertFrom-Json
    if($verified.firstTick -ne $first -or $verified.lastTick -ne $last -or [Math]::Abs($verified.tickRate-$rate) -gt 0.000001){throw 'Capture range/clock mismatch'}
    return $verified
}
try {
    $module=Join-Path $PSScriptRoot 'capture-analysis-attachments.mjs'
    $job=[ordered]@{schemaVersion=1;demo=$source.source.demoFingerprint;context=(Digest $Context);cs2=(Digest $Cs2);hlae=(Digest $Hlae);afx=(Digest $afx);hook=(Digest $Hook);module=(Digest $module);firstTick=$FirstTick;lastTick=$LastTick;segmentTicks=$SegmentTicks;attachments=$Attachments;tickRate=$rate}
    if($FrameCap -ge 0){$job['frameCap']=$FrameCap}
    $jobPath=Join-Path $root 'job.json';$jobText=$job|ConvertTo-Json -Depth 12 -Compress
    if(Test-Path -LiteralPath $jobPath){if([IO.File]::ReadAllText($jobPath) -ne $jobText){throw 'Capture job changed; use a new output directory'}}else{Save-New $jobPath $job}
    $parts=@();$pending=@()
    for($first=$FirstTick;$first -le $LastTick;$first+=$SegmentTicks){
        $last=[Math]::Min($first+$SegmentTicks-1,$LastTick)
        $part=[ordered]@{first=$first;last=$last;path="capture-$first-$last.log.gz"};$parts+=,$part
        $legacy=Join-Path $root "capture-$first-$last.log"
        if(Test-Path -LiteralPath $legacy){$part.path="capture-$first-$last.log"}
        $file=Join-Path $root $part.path;$checkpoint=$file+'.json'
        if(Test-Path -LiteralPath $checkpoint){$saved=Get-Content -LiteralPath $checkpoint -Raw|ConvertFrom-Json;if($saved.fingerprint -ne (Digest $file) -or $saved.validation.firstTick -ne $first -or $saved.validation.lastTick -ne $last){throw 'Completed capture changed'}}elseif(Test-Path -LiteralPath $file){$verified=Validate-Capture $file $first $last;Save-New $checkpoint @{fingerprint=(Digest $file);validation=$verified}}else{$pending+=,$part}
    }
    if($pending.Count){
        if(Get-Process cs2 -ErrorAction SilentlyContinue){throw 'CS2 is already running; nothing launched'}
        if(-not ('DemoDeskWindowObserver' -as [type])){Add-Type -Path (Join-Path $PSScriptRoot 'CompatibilityNative.cs')}
        . (Join-Path $PSScriptRoot 'compatibility-report.ps1')
        Copy-Item -LiteralPath $module -Destination (Join-Path $root 'capture-analysis-attachments.mjs') -Force
        [IO.Directory]::CreateDirectory((Join-Path $root 'cfg'))|Out-Null
        $listener=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,0);$listener.Start();$port=$listener.LocalEndpoint.Port;$listener.Stop()
        $info=[Diagnostics.ProcessStartInfo]::new($Hlae);$info.UseShellExecute=$false;$info.CreateNoWindow=$true;$info.WindowStyle='Hidden';$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
        $gameArgs="-insecure -novid -windowed -width 1280 -height 720 -netconport $port -afxFixNetCon -afxDisableSteamStorage +cl_demo_predict 0 +engine_no_focus_sleep 0 +volume 0 +unbindall +m_yaw 0 +m_pitch 0"
        $info.Arguments='-noGui -noConfig -autoStart -afxDisableSteamStorage -customLoader -hookDllPath "'+$Hook+'" -hookDllPath "'+$afx+'" -programPath "'+$Cs2+'" -cmdLine "'+$gameArgs+'"'
        $info.EnvironmentVariables['USRLOCALCSGO']=Join-Path $root 'cfg'
        $hookLog=Join-Path $root ('window-hook-'+[Guid]::NewGuid().ToString('N')+'.log');$info.EnvironmentVariables['DEMODESK_WINDOW_HOOK_LOG']=$hookLog
        $loader=$null;$game=$null;$client=$null;$observer=$null;$loaderLog=$null;$output=$null;$outputFile=$null
        $script:tail='';$script:done=$false;$script:ready=$false;$script:loaded=$false;$script:resources=$false
        $buffer=New-Object byte[] 65536
        function Send([string]$command){$bytes=$utf8.GetBytes($command+[char]10);$client.GetStream().WriteTimeout=1000;$client.GetStream().Write($bytes,0,$bytes.Length)}
        function Drain {
            if($game -and $game.HasExited){throw 'Replay process exited'}
            while($client -and $client.GetStream().DataAvailable){
                $n=$client.GetStream().Read($buffer,0,$buffer.Length);if(!$n){throw 'Console closed'}
                if($script:outputBytes+$n -gt 128MB){throw 'Capture segment exceeded 128 MiB'};$output.Write($buffer,0,$n);$script:outputBytes+=$n
                $script:tail+=$utf8.GetString($buffer,0,$n)
                if($script:tail.Contains('DEMODESK_POSE_ERROR')){throw 'Capture script failed; inspect the pending log'}
                if($script:tail.Contains('DEMODESK_POSE ["done"')){$script:done=$true}
                if($script:tail.Contains('DEMODESK_POSE ["ready"')){$script:ready=$true}
                if($script:tail.Contains('Host activate: Playing Demo')){$script:loaded=$true}
                if($script:tail.Contains('SetIgnorePacketsForResourceLoading(false)')){$script:resources=$true}
                if($script:tail.Length -gt 512){$script:tail=$script:tail.Substring($script:tail.Length-512)}
            }
        }
        try {
            Open-Log (Join-Path $root ('startup-'+[Guid]::NewGuid().ToString('N')+'.log.gz'))
            $loader=[Diagnostics.Process]::Start($info);$loaderLog=[DemoDeskLoaderLog]::new($loader);$clock=[Diagnostics.Stopwatch]::StartNew()
            while($clock.Elapsed.TotalSeconds -lt 90){
                if(!$game){$children=@([DemoDeskWindowObserver]::Children($loader.Id));if($children.Count -gt 1){throw 'Ambiguous replay children'};if($children.Count -eq 1){$candidate=[Diagnostics.Process]::GetProcessById($children[0]);if([DemoDeskWindowObserver]::Image($candidate) -ne $Cs2){$candidate.Dispose();throw 'Unexpected replay image'};$game=$candidate;$observer=[DemoDeskWindowObserver]::new($game.Id)}}
                if($game -and !$client){$attempt=[Net.Sockets.TcpClient]::new();try{$attempt.Connect('127.0.0.1',$port);$client=$attempt}catch{$attempt.Dispose()}}
                if($client){break};Start-Sleep -Milliseconds 100
            }
            if(!$client){throw 'Replay startup timeout'}
            if(!(Read-SharedHookLog $hookLog).Contains('installed')){throw 'Hidden window hook not confirmed'}
            $fps=if($FrameCap -lt 0){$rate}else{$FrameCap}
            Send ('fps_max '+$fps.ToString([Globalization.CultureInfo]::InvariantCulture)+'; host_framerate '+$rate.ToString([Globalization.CultureInfo]::InvariantCulture)+'; playdemo "'+$Demo.Replace('\','/')+'"')
            while($clock.Elapsed.TotalSeconds -lt 120){Drain;if($script:loaded -and $script:resources){break};Start-Sleep -Milliseconds 100}
            if(!$script:loaded -or !$script:resources){throw 'Demo load timeout'}
            foreach($part in $pending){
                Send 'demo_pause';Drain
                $warm=[Math]::Max(0,$part.first-32);Send ('demo_gototick '+$warm)
                $settle=$clock.Elapsed.TotalSeconds+2;while($clock.Elapsed.TotalSeconds -lt $settle){Drain;Start-Sleep -Milliseconds 20}
                Close-Log
                $temporary=Join-Path $root ($part.path+'.pending-'+[Guid]::NewGuid().ToString('N'))
                Open-Log $temporary;$script:tail='';$script:done=$false;$script:ready=$false
                $wrapper=Join-Path $root ('run-'+$part.first+'-'+[Guid]::NewGuid().ToString('N')+'.mjs')
                $options=@{includeIdentity=$true;firstTick=$part.first;lastTick=$part.last;attachments=$Attachments}|ConvertTo-Json -Compress
                $code="import {startAttachmentCapture} from './capture-analysis-attachments.mjs';`nif(mirv.getDemoTick()>=$($part.first)){mirv.message('DEMODESK_POSE_ERROR seek not settled\n');}else{startAttachmentCapture($options);}`n"
                [IO.File]::WriteAllText($wrapper,$code,$utf8)
                Send ('mirv_script_load "'+$wrapper.Replace('\','/')+'"')
                $deadline=$clock.Elapsed.TotalSeconds+10;while(!$script:ready -and $clock.Elapsed.TotalSeconds -lt $deadline){Drain;Start-Sleep -Milliseconds 15}
                if(!$script:ready){throw 'Capture script did not become ready'}
                Send 'demo_resume';$deadline=$clock.Elapsed.TotalSeconds+($part.last-$warm+1)/$rate+60
                while(!$script:done -and $clock.Elapsed.TotalSeconds -lt $deadline){Drain;Start-Sleep -Milliseconds 15}
                if(!$script:done){throw 'Capture segment timed out'}
                Send 'demo_pause';Drain;Close-Log;$output=$null
                $verified=Validate-Capture $temporary $part.first $part.last
                $file=Join-Path $root $part.path
                if(Test-Path -LiteralPath $file){throw 'Uncheckpointed log exists; preserve it and use a new output directory'}
                [IO.File]::Move($temporary,$file);Save-New ($file+'.json') @{fingerprint=(Digest $file);validation=$verified}
                Write-Output ("Captured {0}-{1}; completed segment retained." -f $part.first,$part.last)
                Open-Log (Join-Path $root ('between-'+[Guid]::NewGuid().ToString('N')+'.log.gz'))
            }
        } finally {
            if($client){try{Send 'quit'}catch{};$client.Dispose()}
            if($game){try{$game.Refresh();Save-New (Join-Path $root ('capture-metrics-'+[Guid]::NewGuid().ToString('N')+'.json')) @{elapsedSeconds=$clock.Elapsed.TotalSeconds;gamePeakWorkingSetBytes=$game.PeakWorkingSet64;controllerPeakWorkingSetBytes=([Diagnostics.Process]::GetCurrentProcess().PeakWorkingSet64)}}catch{Write-Warning 'Capture peak metrics unavailable after process exit'};if(!$game.WaitForExit(5000)){$game.Kill();$null=$game.WaitForExit(5000)};$game.Dispose()}
            if($observer){$observer.Dispose();Save-New (Join-Path $root ('windows-'+[Guid]::NewGuid().ToString('N')+'.json')) @($observer.Snapshot())}
            if($loader){if(!$loader.WaitForExit(5000)){$loader.Kill();$null=$loader.WaitForExit(5000)};if($loaderLog){[IO.File]::WriteAllText((Join-Path $root ('loader-'+[Guid]::NewGuid().ToString('N')+'.log')),$loaderLog.Finish(),$utf8)};$loader.Dispose()}
            Close-Log
        }
    }
    $captures=@($parts|ForEach-Object {@{path=$_.path;fingerprint=(Digest (Join-Path $root $_.path))}})
    $manifest=Join-Path $root 'captures.json'
    if(!(Test-Path -LiteralPath $manifest)){Save-New $manifest @{schemaVersion=1;captures=$captures}}else{
        $saved=Get-Content -LiteralPath $manifest -Raw|ConvertFrom-Json
        if($saved.schemaVersion -ne 1 -or $saved.captures.Count -ne $captures.Count){throw 'Capture manifest changed'}
        for($i=0;$i -lt $captures.Count;$i++){if($saved.captures[$i].path -ne $captures[$i].path -or $saved.captures[$i].fingerprint -ne $captures[$i].fingerprint){throw 'Capture manifest changed'}}
    }
    Write-Output ('Capture manifest ready: '+$manifest)
} finally {$lock.Dispose()}
