// shell/scan.js
// 建索引这条后台流程：状态栏进度、估算时间、建完自动进界面。
import { api } from "./boot.js";
import { setNote } from "./welcome.js";
import { setStatus } from "./titlebar.js";
import { setSideOpen } from "./sidetoggle.js";

export let scanTimer = null;
export let smoothEta = 0;

export function fmtEta(ms) {
  if (!ms || ms < 800) return "即将完成";
  const s = Math.round(ms / 1000);
  if (s < 60) return "还剩 " + s + " 秒";
  const m = Math.floor(s / 60);
  const rest = s % 60;
  return "还剩 " + m + " 分" + (rest ? " " + rest + " 秒" : "");
}

export function showProgress(fraction, text) {
  const pct = Math.max(0, Math.min(100, Math.round(fraction * 100)));
  status.classList.remove("err");
  status.classList.add("busy");
  let bar = status.querySelector(".tb-prog i");
  if (!bar) {
    status.innerHTML = '<span class="tb-prog"><i></i></span><span class="tb-prog-t"></span>';
    bar = status.querySelector(".tb-prog i");
  }
  bar.style.width = pct + "%";
  status.querySelector(".tb-prog-t").textContent = text;
}

export function stopScanWatch() {
  if (scanTimer) {
    clearInterval(scanTimer);
    scanTimer = null;
  }
  smoothEta = 0;
}

// 打开项目后立刻进界面，索引在后头建，这里只负责把进度画出来
export function watchScan(path) {
  stopScanWatch();
  showProgress(0, "准备建索引…");
  scanTimer = setInterval(async function () {
    let s = null;
    try {
      s = await api.invoke("scan_status");
    } catch (err) {
      return;
    }
    if (!s) return;

    if (s.state === "failed") {
      stopScanWatch();
      setStatus(s.error || "建索引失败", true);
      setNote(s.error || "建索引失败", true);
      return;
    }
    if (s.state === "ready") {
      stopScanWatch();
      setStatus("正在载入…");
      try {
        const info = await api.invoke("open_project", { path: path });
        await window.bootProject(info && info.entries);
        setSideOpen(true);
        setStatus("");
        setNote("");
      } catch (err) {
        setStatus(String(err), true);
      }
      return;
    }

    const eta = s.eta_ms > 0 ? s.eta_ms : 0;
    if (eta) smoothEta = smoothEta > 0 ? smoothEta * 0.7 + eta * 0.3 : eta;
    const pct = Math.round((s.fraction || 0) * 100);
    const tail = smoothEta ? " · " + fmtEta(smoothEta) : " · 估算中";
    showProgress(
      s.fraction || 0,
      (s.phase || "建索引") + " " + pct + "%" + tail
    );
  }, 400);
}
