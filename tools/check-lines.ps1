param(
    [int]$Limit = 300,
    [switch]$Quiet
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

$files = Get-ChildItem (Join-Path $root "crates"), (Join-Path $root "src-tauri") `
    -Recurse -Filter *.rs -File `
    | Where-Object { $_.FullName -notmatch '\\target\\|\\gen\\|\\tests\\' }

$rows = foreach ($f in $files) {
    $lines = Get-Content $f.FullName
    $total = $lines.Count
    $marker = $lines | Select-String -Pattern '^#\[cfg\(test\)\]' | Select-Object -First 1
    $main = if ($marker) { $marker.LineNumber - 1 } else { $total }
    [pscustomobject]@{
        File  = $f.FullName.Replace("$root\", "").Replace('\', '/')
        Main  = $main
        Tests = $total - $main
    }
}

$over = $rows | Where-Object { $_.Main -gt $Limit } | Sort-Object Main -Descending

if (-not $Quiet) {
    if ($over) {
        Write-Host "over $Limit lines of main logic:" -ForegroundColor Yellow
        foreach ($r in $over) {
            Write-Host ("  {0,-46} {1,5}  (+{2}, tests {3})" -f $r.File, $r.Main, ($r.Main - $Limit), $r.Tests)
        }
        Write-Host ""
        Write-Host "$($over.Count) file(s) over the limit" -ForegroundColor Yellow
    }
    else {
        Write-Host "all files within $Limit lines of main logic" -ForegroundColor Green
    }
}

if ($over) { exit 1 }
exit 0
