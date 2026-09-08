# Pure PowerShell 5.1 report helpers.
function Get-HookSummary([string]$Text) {
    $lines = @($Text -split '\r?\n')
    return [ordered]@{
        installed = $lines -contains 'installed'
        classes = @($lines | Where-Object { $_.StartsWith('create class=') } | ForEach-Object { $_.Substring(13) } | Sort-Object -Unique)
        interceptions = @($lines | Where-Object { $_.StartsWith('intercepted ') } | Sort-Object -Unique)
    }
}
function Compare-Compatibility($Before, $After) {
    foreach ($key in @('cs2','hlae','afx','hook','engine2','sdl')) {
        if ($Before.files.$key.sha256 -ne $After.files.$key.sha256) {
            "$key SHA256: $($Before.files.$key.sha256) -> $($After.files.$key.sha256)"
        }
    }
    foreach ($key in @('classes','interceptions')) {
        foreach ($item in @($After.hook.$key)) { if (@($Before.hook.$key) -notcontains $item) { "$key added: $item" } }
        foreach ($item in @($Before.hook.$key)) { if (@($After.hook.$key) -notcontains $item) { "$key not observed this run: $item" } }
    }
    foreach ($key in @('startupPassed','windowsVersion','steamBuildId')) {
        if ($Before.$key -ne $After.$key) { "${key}: $($Before.$key) -> $($After.$key)" }
    }
}
function Save-CompatibilityBaseline($Report, [string]$Path) {
    if ($Report.schemaVersion -ne 1 -or $Report.startupPassed -ne $true) { throw 'Cannot accept a failed or unsupported report.' }
    $summary = [ordered]@{schemaVersion=1; startupPassed=$true; files=$Report.files; hook=$Report.hook;
        windowsVersion=$Report.windowsVersion; steamBuildId=$Report.steamBuildId; acceptedAt=[DateTime]::UtcNow.ToString('o')}
    $temporary = $Path + '.pending'
    [IO.File]::WriteAllText($temporary, ($summary | ConvertTo-Json -Depth 12), [Text.UTF8Encoding]::new($false))
    if ([IO.File]::Exists($Path)) { [IO.File]::Replace($temporary, $Path, [NullString]::Value) }
    else { [IO.File]::Move($temporary, $Path) }
}
function Read-SharedHookLog([string]$Path) {
    $stream=[IO.File]::Open($Path,'Open','Read','ReadWrite')
    try {
        if ($stream.Length -gt 2MB) { throw 'Hook log exceeded 2 MiB.' }
        $reader=[IO.StreamReader]::new($stream)
        try { return $reader.ReadToEnd() } finally { $reader.Dispose() }
    } finally { $stream.Dispose() }
}
