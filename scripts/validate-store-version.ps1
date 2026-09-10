param(
    [Parameter(Mandatory = $true)]
    [string]$Version
)
# Store reserves the fourth segment; the first must be nonzero.
if ($Version -notmatch '^[1-9][0-9]*\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') {
    throw 'Store version must be major.minor.patch with major >= 1 and no leading zeros.'
}
foreach ($part in $Version.Split('.')) {
    $number = 0
    if (-not [int]::TryParse($part, [ref]$number) -or $number -gt 65535) {
        throw 'Each Store version segment must be at most 65535.'
    }
}
