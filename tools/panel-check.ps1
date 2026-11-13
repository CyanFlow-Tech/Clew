param([int]$Port = 8899, [int]$Cdp = 9444)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$web = Join-Path $root "target\web"

Copy-Item (Join-Path $root "src\shell.js") $web -Force

# 造一份测试页：在 shell.js 加载之前塞一个 Tauri 桩，让面板走真实的 api.invoke 分支。
# 浏览器预览里 desktop 恒为 false，搜索那条路根本不会被执行，桩是必要的。
$stub = @'
      const noopWin = {
        hide() {}, show() {}, minimize() {}, toggleMaximize() {}, maximize() {},
        unmaximize() {}, close() {}, setTitle() {}, startDragging() {},
        isMaximized() { return Promise.resolve(false); },
        onCloseRequested() { return Promise.resolve(function () {}); },
        onResized() { return Promise.resolve(function () {}); },
      };
      window.__TAURI__ = {
        core: {
          invoke(cmd, args) {
            if (cmd === "roots") return Promise.resolve({ favorites: [], recent: [] });
            if (cmd === "search") {
              const text = (args && args.q) || "";
              const scope = (args && args.scope) || "entry";
              const hit = (kind, module, qualname, line, card, entry) => ({
                kind: kind, module: module, path: module.replace(/\./g, "/") + ".py",
                qualname: qualname, def_line: line, card_id: card,
                is_entry: !!entry, field: "name", spans: nameSpans(qualname), score: 1000,
              });
              const nameSpans = (n) => {
                const i = n.lastIndexOf(".") + 1;
                return n.toLowerCase().includes("main") ? [[i, i + 4]] : [];
              };
              if (!text) {
                const browse = scope === "all"
                  ? [hit("func", "pkg.store", "delete", 50, "pkg.store:delete", false)]
                  : [hit("mod", "pkg.session", "<module>", 0, "pkg.session:<module>", true),
                     hit("func", "pkg.session", "main", 10, "pkg.session:main", true)];
                return Promise.resolve({ hits: browse });
              }
              return Promise.resolve({ hits: [hit("func", "hermes_cli.curator", "main_guard", 525, "hermes_cli.curator:main_guard", true)] });
            }
            return Promise.resolve({});
          },
        },
        window: { getCurrentWindow: () => noopWin },
        dialog: { open: () => Promise.resolve(null), message: () => Promise.resolve() },
      };
'@

$html = Get-Content (Join-Path $root "src\index.html") -Raw
$anchor = "        t.onload = function () {"
if (-not $html.Contains($anchor)) { throw "index.html 里找不到注入点" }
$html = $html.Replace($anchor, $stub + $anchor)
Set-Content -Path (Join-Path $web "harness.html") -Value $html -Encoding UTF8

$server = Start-Process -FilePath "python" -ArgumentList @("-m", "http.server", "$Port", "--directory", $web) -WindowStyle Hidden -PassThru
Start-Sleep -Seconds 2
$profile = Join-Path $root "target\edge-profile-check"
Remove-Item -Recurse -Force $profile -ErrorAction SilentlyContinue
$edge = Start-Process -FilePath "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe" -ArgumentList @(
    "--headless=new", "--remote-debugging-port=$Cdp", "--user-data-dir=$profile",
    "--no-first-run", "--disable-extensions", "--disable-sync", "--disable-gpu",
    "--window-size=1440,920", "http://127.0.0.1:$Port/harness.html"
) -PassThru
Start-Sleep -Seconds 14

$env:CLEW_CDP_PORT = "$Cdp"
$check = @'
(async () => {
  const errs = [];
  window.addEventListener("error", (e) => errs.push(String(e.message)));
  const P = window.__clewPanel;
  if (!P) return { fatal: "panel 没有挂上，shell.js 里可能有异常" };
  const q = document.getElementById("ep-q");
  const rows = () =>
    Array.from(document.querySelectorAll("#ep-one-list .qrow")).map(
      (el) =>
        (el.querySelector(".q-pre") || {}).textContent +
        ":" +
        (el.querySelector(".q-fn") || {}).textContent
    );
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));

  await P.open();
  await wait(400);
  const out = { entryBrowse: { title: document.getElementById("ep-one-title").textContent, rows: rows() } };

  document.querySelector('[data-scope="all"]').click();
  await wait(500);
  out.allBrowse = { title: document.getElementById("ep-one-title").textContent, rows: rows() };

  q.value = "main";
  q.dispatchEvent(new Event("input", { bubbles: true }));
  await wait(600);
  out.typed = { title: document.getElementById("ep-one-title").textContent, rows: rows() };

  q.value = "";
  q.dispatchEvent(new Event("input", { bubbles: true }));
  await wait(500);
  out.cleared = { title: document.getElementById("ep-one-title").textContent, rows: rows() };
  out.errs = errs;
  return out;
})()
'@
$checkPath = Join-Path $root "target\panel-check.js"
Set-Content -Path $checkPath -Value $check -Encoding UTF8
node (Join-Path $root "tools\cdp.js") "@$checkPath"

Stop-Process -Id $edge.Id -Force -ErrorAction SilentlyContinue
Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
Get-CimInstance Win32_Process -Filter "Name='msedge.exe'" | Where-Object { $_.CommandLine -match "$Cdp" } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
Remove-Item -Recurse -Force $profile, $checkPath -ErrorAction SilentlyContinue
