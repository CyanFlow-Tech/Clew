<#
  开发模式：起本地静态服务 + 跑 debug 版窗口。

    .\tools\dev.ps1                       # 起服务并打开窗口（项目从命令行参数传）
    .\tools\dev.ps1 -Project D:\Foo\clew   # 直接打开某个项目
    .\tools\dev.ps1 -NoRun                 # 只起静态服务，不启动窗口

  前端（src/ 下的 html/js）由 WebView 直接从这个服务加载，改完存盘页面会自己刷新，
  不用重新编译。只有改 Rust 才需要重跑（debug 构建，比 release 快得多）。
  发版仍然用 `cargo app`（等价于 build --release --features custom-protocol）。
#>
param(
  [string]$Project = "",
  [int]$Port = 1755,
  [switch]$NoRun,
  # WebView2 的远程调试端口：开着就能用 tools/cdp.js 直接查窗口里的 DOM
  [int]$Cdp = 9222
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$src = Join-Path $root "src"
$devUrl = "http://127.0.0.1:$Port"

# devUrl 端口必须和 tauri.conf.json 里的 build.devUrl 一致，否则窗口是白的
$conf = Get-Content (Join-Path $root "src-tauri\tauri.conf.json") -Raw
if ($conf -notmatch [regex]::Escape($devUrl)) {
  Write-Warning "tauri.conf.json 里的 devUrl 不是 $devUrl，窗口可能加载不到前端"
}

# 用带 no-store 的小服务器：python 自带的 http.server 会命中 WebView 的启发式缓存，
# 改了 js 刷新还可能拿到旧文件
$server = Start-Process -FilePath "python" `
  -ArgumentList @((Join-Path $root "tools\dev-server.py"), "$Port", $src) `
  -WindowStyle Hidden -PassThru
Start-Sleep -Milliseconds 800
Write-Host "前端服务: $devUrl  (改 src/ 下文件存盘即刷新)"

if ($NoRun) {
  Write-Host "只起了服务，Ctrl+C 结束"
  try { Wait-Process -Id $server.Id } finally { Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue }
  return
}

# 让 WebView2 开远程调试，tools/cdp.js 才能连进来（不影响前端功能）
if ($Cdp -gt 0) {
  $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Cdp"
  Write-Host "CDP: http://127.0.0.1:$Cdp  (node tools\cdp.js ""<expr>"")"
}

try {
  # 项目路径可选：不带参数就进欢迎页
  if ($Project) {
    cargo run -p clew-app -- $Project
  } else {
    cargo run -p clew-app
  }
} finally {
  Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
}
