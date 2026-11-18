// shell/util.js
import { RECENT_KEY } from "./boot.js";
import { icon, ICON } from "../shared/icons.js";

export { ICON };

// ── 共用小工具 ──────────────────────────────────────────────────
export const LOGO_IMG = '<img src="logo.png" alt="" width="20" height="20">';

// 静态 HTML 里的图标占位：<span class="hi" data-icon="branches"></span>
document.querySelectorAll("[data-icon]").forEach(function (el) {
  el.innerHTML = icon(el.dataset.icon);
});

export const AVATAR_COLORS = ["#c94f4f", "#4f86c9", "#8f4fc9", "#c9864f", "#4fa87c", "#c94f86"];

export function normPath(p) {
  return String(p || "")
    .replace(/^\\\\\?\\/, "")   // Windows canonicalize 的 \\?\ 前缀
    .replace(/\\\\+/g, "\\");   // 早期版本存进 localStorage 的重复反斜杠
}

export function basename(p) {
  return normPath(p).split(/[\\/]/).filter(Boolean).pop() || normPath(p);
}

export function initials(name) {
  const parts = String(name).split(/[-_.\s]+/).filter(Boolean);
  if (parts.length >= 2) return (parts[0][0] + parts[1][0]).toUpperCase();
  return name.slice(0, 2).toUpperCase();
}

export function colorFor(name) {
  let h = 0;
  for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) >>> 0;
  return AVATAR_COLORS[h % AVATAR_COLORS.length];
}

export function avatar(name, cls) {
  const el = document.createElement("span");
  el.className = cls;
  el.textContent = initials(name);
  el.style.background = colorFor(name);
  return el;
}

export function hexToRgb(hex) {
  const h = hex.replace("#", "");
  return [
    parseInt(h.slice(0, 2), 16),
    parseInt(h.slice(2, 4), 16),
    parseInt(h.slice(4, 6), 16),
  ].join(",");
}

// 顶栏底色跟着项目走：每个项目一个色，铺成低饱和的渐变
export function applyTitleTint(path) {
  const name = path ? basename(path) : "";
  const rgb = hexToRgb(name ? colorFor(name) : "#4f86c9");
  const top = document.getElementById("top");
  top.style.setProperty("--tint-a", "rgba(" + rgb + ",.34)");
  top.style.setProperty("--tint-b", "rgba(" + rgb + ",.09)");
}

// ── 最近项目 ────────────────────────────────────────────────────
export function recent() {
  let list = [];
  try {
    list = JSON.parse(localStorage.getItem(RECENT_KEY) || "[]");
  } catch {
    list = [];
  }
  const seen = new Set();
  return list
    .map((p) => ({ path: normPath(p.path), name: p.name, hidden: !!p.hidden }))
    .filter((p) => {
      if (!p.path || seen.has(p.path)) return false;
      seen.add(p.path);
      return true;
    });
}

export function saveRecent(list) {
  localStorage.setItem(RECENT_KEY, JSON.stringify(list.slice(0, 200)));
}

export function remember(path) {
  const clean = normPath(path);
  if (!clean) return;
  const list = recent();
  const prev = list.find((p) => p.path === clean);
  const rest = list.filter((p) => p.path !== clean);
  rest.unshift({ path: clean, name: basename(clean), hidden: prev ? prev.hidden : false });

  // 可见项目最多二十条，隐藏的不占这个额度
  let visible = 0;
  const kept = [];
  for (const p of rest) {
    if (p.hidden) {
      kept.push(p);
      continue;
    }
    if (visible >= 20) continue;
    visible++;
    kept.push(p);
  }
  saveRecent(kept);
}

export function setHidden(path, hidden) {
  const clean = normPath(path);
  saveRecent(recent().map((p) => (p.path === clean ? { ...p, hidden } : p)));
}

export function forget(path) {
  const clean = normPath(path);
  saveRecent(recent().filter((p) => p.path !== clean));
}
