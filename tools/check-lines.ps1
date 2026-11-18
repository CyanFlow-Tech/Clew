param(
    [int]$Limit = 300,
    [switch]$Quiet
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

# Rust：#[cfg(test)] 之前的行算主逻辑
$rs = Get-ChildItem (Join-Path $root "crates"), (Join-Path $root "src-tauri") `
    -Recurse -Filter *.rs -File `
    | Where-Object { $_.FullName -notmatch '\\target\\|\\gen\\|\\tests\\' }

$rows = foreach ($f in $rs) {
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

# 前端：src/ 下自己写的 js 一个文件不超过 Limit 行。vendor 是第三方，不参与。
$js = Get-ChildItem (Join-Path $root "src") -Recurse -Filter *.js -File `
    | Where-Object { $_.FullName -notmatch '\\vendor\\' }

$rows += foreach ($f in $js) {
    $total = (Get-Content $f.FullName).Count
    [pscustomobject]@{
        File  = $f.FullName.Replace("$root\", "").Replace('\', '/')
        Main  = $total
        Tests = 0
    }
}

$over = $rows | Where-Object { $_.Main -gt $Limit } | Sort-Object Main -Descending

if (-not $Quiet) {
    if ($over) {
        Write-Host "over $Limit lines:" -ForegroundColor Yellow
        foreach ($r in $over) {
            Write-Host ("  {0,-52} {1,5}  (+{2}, tests {3})" -f $r.File, $r.Main, ($r.Main - $Limit), $r.Tests)
        }
        Write-Host ""
        Write-Host "$($over.Count) file(s) over the limit" -ForegroundColor Yellow
    }
    else {
        Write-Host "all files within $Limit lines" -ForegroundColor Green
    }
}

if ($over) { exit 1 }
exit 0
