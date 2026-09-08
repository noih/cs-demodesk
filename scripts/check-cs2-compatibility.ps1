#requires -Version 5.1
[CmdletBinding()]
param(
    [ValidateSet('Scan','Accept')][string]$Mode='Scan',
    [string]$Cs2, [string]$Hlae, [string]$Hook,
    [string]$OutputDir,
    [ValidateRange(10,300)][int]$TimeoutSeconds=60,
    [ValidateRange(1,60)][int]$ObserveSeconds=8
)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'compatibility-report.ps1')
$utf8=[Text.UTF8Encoding]::new($false)
if (!$OutputDir) { $OutputDir=Join-Path (Split-Path $PSScriptRoot) 'target/compatibility' }
$root=[IO.Path]::GetFullPath($OutputDir)
[IO.Directory]::CreateDirectory($root) | Out-Null
$lock=[IO.File]::Open((Join-Path $root '.lock'), 'OpenOrCreate', 'ReadWrite', 'None')
try {
    $latest=Join-Path $root 'latest'; $baseline=Join-Path $root 'baseline.json'
    if ($Mode -eq 'Accept') {
        Save-CompatibilityBaseline (Get-Content -LiteralPath (Join-Path $latest 'report.json') -Raw | ConvertFrom-Json) $baseline
        Write-Output "Accepted summary: $baseline"
        return
    }
    if ($ObserveSeconds -ge $TimeoutSeconds) { throw 'ObserveSeconds must be less than TimeoutSeconds.' }
    if (Test-Path -LiteralPath $latest) {
        $resolved=(Resolve-Path -LiteralPath $latest).Path
        if ($resolved -ne [IO.Path]::GetFullPath((Join-Path $root 'latest')) -or
            (Get-Item -LiteralPath $latest).Attributes.HasFlag([IO.FileAttributes]::ReparsePoint) -or
            !(Test-Path -LiteralPath (Join-Path $latest '.demodesk-diagnostic'))) { throw 'Refusing to replace an unrecognized latest directory.' }
        if (Get-ChildItem -LiteralPath $latest -Recurse -Force | Where-Object { $_.Attributes.HasFlag([IO.FileAttributes]::ReparsePoint) }) { throw 'Refusing cleanup through a reparse point.' }
        Remove-Item -LiteralPath $latest -Recurse -Force
    }
    [IO.Directory]::CreateDirectory($latest) | Out-Null
    [IO.File]::WriteAllText((Join-Path $latest '.demodesk-diagnostic'), '1', $utf8)
    $report=[ordered]@{schemaVersion=1; startedAt=[DateTime]::UtcNow.ToString('o'); startupPassed=$false;
        windowsVersion=[Environment]::OSVersion.VersionString; powershellVersion=$PSVersionTable.PSVersion.ToString();
        files=[ordered]@{}; errors=@(); windows=@(); steamBuildId=$null; differences=@();
        scope=@('Startup only; verify recording/audio and uninterrupted input manually before accepting.',
            'Only known hooked APIs are recorded. Missing calls are not proof of removal.',
            'Events begin after PID discovery; initial enumeration and bounded hook samples supplement them.',
            'This diagnostic does not run the app Windows audio mute or fallback event hider.')}
    $loaderLog=$null; $loader=$null; $game=$null; $observer=$null; $client=$null
    $clock=[Diagnostics.Stopwatch]::StartNew(); $console=[IO.MemoryStream]::new()
    $hookLog=Join-Path $latest 'window-hook.log'
    try {
        if (Get-Process cs2 -ErrorAction SilentlyContinue) { throw 'CS2 already running; nothing was launched.' }
        if (-not ('DemoDeskWindowObserver' -as [type])) { Add-Type -Path (Join-Path $PSScriptRoot 'CompatibilityNative.cs') }
        function File-Identity([string]$Path) {
            $file=Get-Item -LiteralPath $Path
            $hash=[Security.Cryptography.SHA256]::Create(); $stream=[IO.File]::OpenRead($file.FullName)
            try { $digest=([BitConverter]::ToString($hash.ComputeHash($stream))).Replace('-','').ToLowerInvariant() }
            finally { $stream.Dispose(); $hash.Dispose() }
            return [ordered]@{path=$file.FullName; sha256=$digest; bytes=$file.Length; fileVersion=$file.VersionInfo.FileVersion}
        }
        foreach ($pair in @(@('cs2',$Cs2),@('hlae',$Hlae),@('hook',$Hook))) {
            if ([string]::IsNullOrWhiteSpace($pair[1])) { throw "Missing -$($pair[0]) path." }
            $report.files[$pair[0]]=File-Identity $pair[1]
        }
        $Cs2=$report.files.cs2.path; $Hlae=$report.files.hlae.path; $Hook=$report.files.hook.path
        $afx=Join-Path (Split-Path $Hlae) 'x64/AfxHookSource2.dll'; $report.files.afx=File-Identity $afx
        foreach ($pair in @(@('engine2','engine2.dll'),@('sdl','SDL3.dll'))) {
            $path=Join-Path (Split-Path $Cs2) $pair[1]
            if (Test-Path -LiteralPath $path) { $report.files[$pair[0]]=File-Identity $path }
        }
        $ancestor=[IO.DirectoryInfo]::new((Split-Path $Cs2))
        while ($ancestor) {
            $manifest=Join-Path $ancestor.FullName 'appmanifest_730.acf'
            if (Test-Path -LiteralPath $manifest) {
                if ([IO.File]::ReadAllText($manifest) -match '"buildid"\s+"(\d+)"') { $report.steamBuildId=$Matches[1] }
                break
            }
            $ancestor=$ancestor.Parent
        }
        $listener=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,0)
        $listener.Start(); $port=$listener.LocalEndpoint.Port; $listener.Stop()
        $cfg=Join-Path $latest 'cfg'; [IO.Directory]::CreateDirectory($cfg) | Out-Null
        [IO.File]::WriteAllText($hookLog,'',$utf8)
        $gameArgs="-insecure -novid -windowed -width 1920 -height 1080 -netconport $port -afxFixNetCon -afxDisableSteamStorage +cl_demo_predict 0 +engine_no_focus_sleep 0 +demo_ui_mode 0 +cl_show_observer_crosshair 2"
        $info=[Diagnostics.ProcessStartInfo]::new($Hlae)
        $info.UseShellExecute=$false; $info.CreateNoWindow=$true; $info.RedirectStandardOutput=$true; $info.RedirectStandardError=$true
        $info.Arguments='-noGui -noConfig -autoStart -afxDisableSteamStorage -customLoader -hookDllPath "'+$Hook+'" -hookDllPath "'+$afx+'" -programPath "'+$Cs2+'" -cmdLine "'+$gameArgs+'"'
        $info.EnvironmentVariables['USRLOCALCSGO']=$cfg
        $info.EnvironmentVariables['DEMODESK_WINDOW_HOOK_LOG']=$hookLog
        $report.command=@($Hlae,$info.Arguments)
        $report.environment=@{USRLOCALCSGO=$cfg; DEMODESK_WINDOW_HOOK_LOG=$hookLog}
        $loader=[Diagnostics.Process]::Start($info); $loaderLog=[DemoDeskLoaderLog]::new($loader); $report.loaderPid=$loader.Id
        $deadline=$clock.Elapsed.TotalSeconds+$TimeoutSeconds
        while ($clock.Elapsed.TotalSeconds -lt $deadline) {
            $children=@([DemoDeskWindowObserver]::Children($loader.Id))
            if ($children.Count -gt 1) { throw 'Ambiguous CS2 children; refusing to control them.' }
            if ($children.Count -eq 1) {
                $candidate=[Diagnostics.Process]::GetProcessById($children[0])
                if ([DemoDeskWindowObserver]::Image($candidate) -ne $Cs2) { $candidate.Dispose(); throw 'CS2 path mismatch; not controlling it.' }
                $game=$candidate; $report.gamePid=$game.Id; break
            }
            if ($loader.HasExited -and $loader.ExitCode -ne 0) { throw "HLAE exited with $($loader.ExitCode)." }
            Start-Sleep -Milliseconds 100
        }
        if (!$game) { throw 'No verified direct CS2 child of HLAE before timeout.' }
        $observer=[DemoDeskWindowObserver]::new($game.Id)
        $report.observerStartedSeconds=$clock.Elapsed.TotalSeconds
        $token='demodesk_diagnostic_'+[Guid]::NewGuid().ToString('N')
        $readyAt=$null; $buffer=New-Object byte[] 65536
        while ($clock.Elapsed.TotalSeconds -lt $deadline) {
            if ($game.HasExited) { $report.gameExitCode='0x'+$game.ExitCode.ToString('X8'); throw "CS2 exited: $($report.gameExitCode)" }
            if (!$client) {
                $attempt=[Net.Sockets.TcpClient]::new(); $pending=$null
                try {
                    $pending=$attempt.BeginConnect('127.0.0.1',$port,$null,$null)
                    if (!$pending.AsyncWaitHandle.WaitOne(100)) { throw 'connect pending' }
                    $attempt.EndConnect($pending)
                    $bytes=[Text.Encoding]::ASCII.GetBytes('echo '+$token+[char]10)
                    $attempt.GetStream().Write($bytes,0,$bytes.Length); $client=$attempt
                } catch { $attempt.Dispose() }
                finally { if ($pending) { $pending.AsyncWaitHandle.Close() } }
            }
            if ($client -and $client.GetStream().DataAvailable) {
                $count=$client.GetStream().Read($buffer,0,$buffer.Length)
                if ($console.Length+$count -gt 2MB) { throw 'Netcon log exceeded 2 MiB.' }
                $console.Write($buffer,0,$count)
                if ($null -eq $readyAt -and [Text.Encoding]::UTF8.GetString($console.ToArray()).Contains($token)) {
                    $readyAt=$clock.Elapsed.TotalSeconds; $report.netconReadySeconds=$readyAt
                }
            }
            if ($null -ne $readyAt -and $clock.Elapsed.TotalSeconds-$readyAt -ge $ObserveSeconds) { break }
            Start-Sleep -Milliseconds 20
        }
        if ($null -eq $readyAt -or $clock.Elapsed.TotalSeconds-$readyAt -lt $ObserveSeconds) { throw 'Netcon/observation timed out.' }
        $report.netconEcho=$true
        $report.hook=Get-HookSummary ((Read-SharedHookLog $hookLog))
        if (!$report.hook.installed -or !($report.hook.interceptions -match 'CreateWindowEx')) { throw 'Game window interception not confirmed.' }
        $report.windows=@($observer.Snapshot())
        if ($observer.Error -or $observer.Truncated) { throw "Observer incomplete: $($observer.Error)" }
        if ($report.windows | Where-Object { $_.kind -eq '0x3' }) { throw 'CS2 became foreground.' }
        if ($report.windows | Where-Object { $_.visible -and $_.rect[2]-$_.rect[0] -gt 200 -and $_.rect[3]-$_.rect[1] -gt 200 }) { throw 'Large visible CS2 window observed.' }
        $report.startupPassed=$true
    } catch { $report.errors+= $_.Exception.ToString() }
    finally {
        if ($observer) {
            try { $observer.Dispose(); $report.windows=@($observer.Snapshot()) }
            catch { $report.errors+="Observer cleanup: $_"; $report.startupPassed=$false }
        }
        if ($client) {
            try { $bytes=[Text.Encoding]::ASCII.GetBytes('quit'+[char]10); $client.GetStream().WriteTimeout=1000; $client.GetStream().Write($bytes,0,$bytes.Length) } catch {}
            $client.Dispose()
        }
        if ($game) {
            try {
                if (!$game.WaitForExit(5000)) { $report.forcedGameCleanup=$true; $game.Kill(); if (!$game.WaitForExit(5000)) { throw 'CS2 cleanup timed out.' } }
                $report.cleanupExitCode='0x'+$game.ExitCode.ToString('X8')
            } catch { $report.errors+="Game cleanup: $_"; $report.startupPassed=$false }
            $game.Dispose()
        }
        if ($loader) {
            try {
                if (!$loader.WaitForExit(2000)) { $loader.Kill(); $null=$loader.WaitForExit(5000) }
                if ($loader.HasExited) { $report.loaderExitCode=$loader.ExitCode }
            } catch { $report.errors+="Loader cleanup: $_"; $report.startupPassed=$false }
            if ($loaderLog) { [IO.File]::WriteAllText((Join-Path $latest 'hlae.log'),$loaderLog.Finish(),$utf8); $report.loaderLogTruncated=$loaderLog.Truncated }
            $loader.Dispose()
        }
        [IO.File]::WriteAllBytes((Join-Path $latest 'netcon.log'),$console.ToArray()); $console.Dispose()
        $hookText=''
        try { if (Test-Path -LiteralPath $hookLog) { $hookText=Read-SharedHookLog $hookLog } }
        catch { $report.errors+="Hook log: $_"; $report.startupPassed=$false }
        $report.hook=Get-HookSummary $hookText
        $report.hook.classes=@(@($report.hook.classes)+@($report.windows | ForEach-Object { $_.windowClass }) | Where-Object { $_ } | Sort-Object -Unique)
        $report.durationSeconds=$clock.Elapsed.TotalSeconds
        if (Test-Path -LiteralPath $baseline) {
            try { $before=Get-Content -LiteralPath $baseline -Raw | ConvertFrom-Json; $report.differences=@(Compare-Compatibility $before $report) }
            catch { $report.differences=@("Baseline unreadable: $_") }
        } else { $report.differences=@('No accepted baseline yet.') }
        [IO.File]::WriteAllText((Join-Path $latest 'report.json'),($report | ConvertTo-Json -Depth 12),$utf8)
        $lines=@('# CS2 startup compatibility','',"Startup passed: $($report.startupPassed)",'','## Errors')
        $lines+=@($report.errors | ForEach-Object { '- '+$_ })
        $lines+=@('','## Differences')+@($report.differences | ForEach-Object { '- '+$_ })
        $lines+=@('','## Scope')+@($report.scope | ForEach-Object { '- '+$_ })
        $lines+=@('','report.json contains binary identities, exact command, PID, window events and timing.',
            'Next scan replaces latest. Baseline stores only a summary; no log history is accumulated.')
        [IO.File]::WriteAllLines((Join-Path $latest 'report.md'),$lines,$utf8)
        Write-Output (Join-Path $latest 'report.md')
    }
    if (!$report.startupPassed) { throw 'Startup diagnostic failed; see latest/report.md.' }
} finally { $lock.Dispose() }
