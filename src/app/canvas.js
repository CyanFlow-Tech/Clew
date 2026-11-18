// app/canvas.js
import { graph } from "./dom.js";
import { store } from "./store.js";
import { onRelayout } from "./layout.js";

// ─── 画布拖拽 ───────────────────────────────────────────────────
(function panCanvas() {
  let panning = false;
  let startX = 0;
  let startY = 0;
  let startLeft = 0;
  let startTop = 0;

  // 卡片和控件上的按下要留给点击，别被平移吃掉
  function interactive(target) {
    return !!(
      target &&
      target.closest &&
      target.closest(".card, .acc-head, button, select, input, textarea")
    );
  }

  graph.addEventListener("mousedown", function (e) {
    if (e.button !== 0 || interactive(e.target)) return;
    panning = true;
    startX = e.clientX;
    startY = e.clientY;
    startLeft = graph.scrollLeft;
    startTop = graph.scrollTop;
    document.body.classList.add("panning");
    e.preventDefault();
  });

  window.addEventListener("mousemove", function (e) {
    if (!panning) return;
    graph.scrollLeft = startLeft - (e.clientX - startX);
    graph.scrollTop = startTop - (e.clientY - startY);
  });

  function stopPan() {
    if (!panning) return;
    panning = false;
    document.body.classList.remove("panning");
  }

  window.addEventListener("mouseup", stopPan);
  window.addEventListener("blur", stopPan);
})();


// ─── minimap ────────────────────────────────────────────────────
(function minimap() {
  const W = 200;
  const H = 150;
  const box = document.createElement("div");
  box.id = "minimap";
  box.title = "点或拖这里移动画布";
  const canvas = document.createElement("canvas");
  box.appendChild(canvas);
  document.body.appendChild(box);

  const ctx = canvas.getContext("2d");
  let frame = 0;
  let view = null;

  function fit() {
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(W * dpr);
    canvas.height = Math.round(H * dpr);
    canvas.style.width = W + "px";
    canvas.style.height = H + "px";
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }
  fit();

  // 卡片包围盒，留点边
  function bounds() {
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    Object.keys(store.cardEls).forEach(function (id) {
      const el = store.cardEls[id];
      if (!el) return;
      const x = parseFloat(el.style.left) || 0;
      const y = parseFloat(el.style.top) || 0;
      minX = Math.min(minX, x);
      minY = Math.min(minY, y);
      maxX = Math.max(maxX, x + (el.offsetWidth || 0));
      maxY = Math.max(maxY, y + (el.offsetHeight || 0));
    });
    if (!isFinite(minX)) return null;
    const pad = 90;
    return {
      x: minX - pad,
      y: minY - pad,
      w: maxX - minX + pad * 2,
      h: maxY - minY + pad * 2,
    };
  }

  function render() {
    ctx.clearRect(0, 0, W, H);
    const b = bounds();
    if (!b) {
      view = null;
      return;
    }
    const scale = Math.min(W / b.w, H / b.h);
    const ox = (W - b.w * scale) / 2;
    const oy = (H - b.h * scale) / 2;
    view = { scale: scale, ox: ox, oy: oy, b: b };

    const X = function (x) { return ox + (x - b.x) * scale; };
    const Y = function (y) { return oy + (y - b.y) * scale; };

    ctx.fillStyle = "#2b3440";
    ctx.strokeStyle = "#414d5d";
    ctx.lineWidth = 1;
    Object.keys(store.cardEls).forEach(function (id) {
      const el = store.cardEls[id];
      if (!el) return;
      const x = parseFloat(el.style.left) || 0;
      const y = parseFloat(el.style.top) || 0;
      const w = Math.max(2, (el.offsetWidth || 0) * scale);
      const h = Math.max(2, (el.offsetHeight || 0) * scale);
      ctx.fillRect(X(x), Y(y), w, h);
      ctx.strokeRect(X(x), Y(y), w, h);
    });

    // 视口可能比内容还大，裁到 minimap 范围内，否则会画出框外
    const vx0 = Math.max(0, Math.min(W, X(graph.scrollLeft)));
    const vy0 = Math.max(0, Math.min(H, Y(graph.scrollTop)));
    const vx1 = Math.max(0, Math.min(W, X(graph.scrollLeft + graph.clientWidth)));
    const vy1 = Math.max(0, Math.min(H, Y(graph.scrollTop + graph.clientHeight)));
    const vw = Math.max(5, vx1 - vx0);
    const vh = Math.max(5, vy1 - vy0);
    ctx.fillStyle = "rgba(88,166,255,0.14)";
    ctx.strokeStyle = "#58a6ff";
    ctx.lineWidth = 1.5;
    ctx.fillRect(vx0, vy0, vw, vh);
    ctx.strokeRect(vx0, vy0, vw, vh);
  }

  function schedule() {
    if (frame) return;
    frame = requestAnimationFrame(function () {
      frame = 0;
      render();
    });
  }

  let dragging = false;
  function jumpTo(e) {
    if (!view) return;
    const rect = canvas.getBoundingClientRect();
    const worldX = (e.clientX - rect.left - view.ox) / view.scale + view.b.x;
    const worldY = (e.clientY - rect.top - view.oy) / view.scale + view.b.y;
    graph.scrollLeft = worldX - graph.clientWidth / 2;
    graph.scrollTop = worldY - graph.clientHeight / 2;
    schedule();
  }

  canvas.addEventListener("mousedown", function (e) {
    if (e.button !== 0) return;
    dragging = true;
    jumpTo(e);
    e.preventDefault();
    e.stopPropagation();
  });
  window.addEventListener("mousemove", function (e) {
    if (dragging) jumpTo(e);
  });
  window.addEventListener("mouseup", function () {
    dragging = false;
  });

  graph.addEventListener("scroll", schedule);
  window.addEventListener("resize", function () {
    fit();
    schedule();
  });

  onRelayout(schedule);

  schedule();
})();

