<#
  浏览器预览：导出图数据 + 起静态服务，不弹 Tauri 窗口。

    .\tools\web-preview.ps1 -Project D:\CodeEverything\PythonWork\alpha-zero-general\reference
    .\tools\web-preview.ps1 -Project <项目> -Port 8000

  改完 src/ 里的 html/js 直接刷新浏览器即可，不用重新编译桌面端。
  需要验证窗口按钮、拖动窗口、原生文件对话框、以及资源是否嵌进二进制时，
  仍然用 cargo run -p clew-app -- <项目>。
#>
param(
  [Parameter(Mandatory = $true)][string]$Project,
  [int]$Port = 8000
)

$ErrorActionPreference = "Stop"

$repo = Split-Path -Parent $PSScriptRoot
$web = Join-Path $repo "target\web"
$manifest = Join-Path $repo "Cargo.toml"
$db = Join-Path (Resolve-Path $Project) ".clew\index.db"

New-Item -ItemType Directory -Force -Path $web | Out-Null
Copy-Item (Join-Path $repo "src\index.html") $web -Force
Copy-Item (Join-Path $repo "src\app.js") $web -Force
foreach ($extra in @("shell.js", "logo.png")) {
  $src = Join-Path $repo "src\$extra"
  if (Test-Path $src) { Copy-Item $src $web -Force }
}

if (-not (Test-Path $db)) {
  Write-Host "no index yet, scanning once..."
  cargo @("run", "-q", "--manifest-path", $manifest, "-p", "clew-cli", "--", "scan", $Project)
}

cargo @(
  "run", "-q", "--manifest-path", $manifest, "-p", "clew-cli", "--",
  "export", $Project, "--out", (Join-Path $web "__clew_graph.json")
)

Write-Host ""
Write-Host "http://127.0.0.1:$Port/    (Ctrl+C 退出)"
python -m http.server $Port --directory $web
