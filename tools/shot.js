// 截一张窗口的图：先跑 prep 脚本（可选），再 Page.captureScreenshot。
//
//   node tools/shot.js <out.png> [prep.js]
//
// 需要窗口带 CDP：用 tools/dev.ps1 起的窗口自带（默认 9222）。

const fs = require("node:fs");
const port = process.env.CLEW_CDP_PORT || 9222;

async function pickTarget() {
  for (let i = 0; i < 40; i++) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
      const page = list.find((t) => t.type === "page");
      if (page) return page;
    } catch {
      /* 还没起来 */
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error("no CDP page target on port " + port);
}

function connect(url) {
  const ws = new WebSocket(url);
  let seq = 0;
  const waiting = new Map();
  ws.addEventListener("message", (ev) => {
    let msg;
    try {
      msg = JSON.parse(ev.data);
    } catch {
      return;
    }
    const slot = waiting.get(msg.id);
    if (!slot) return;
    waiting.delete(msg.id);
    if (msg.error) slot.reject(new Error(JSON.stringify(msg.error)));
    else slot.resolve(msg.result);
  });
  const send = (method, params) =>
    new Promise((resolve, reject) => {
      const id = ++seq;
      waiting.set(id, { resolve, reject });
      ws.send(JSON.stringify({ id, method, params: params || {} }));
    });
  const ready = new Promise((resolve, reject) => {
    ws.addEventListener("open", resolve, { once: true });
    ws.addEventListener("error", reject, { once: true });
  });
  return { ready, send, close: () => ws.close() };
}

async function main() {
  const out = process.argv[2];
  if (!out) {
    console.error("usage: node tools/shot.js <out.png> [prep.js]");
    process.exit(2);
  }
  const target = await pickTarget();
  const cdp = connect(target.webSocketDebuggerUrl);
  await cdp.ready;

  if (process.argv[3]) {
    const expr = fs.readFileSync(process.argv[3], "utf8");
    const res = await cdp.send("Runtime.evaluate", {
      expression: expr,
      returnByValue: true,
      awaitPromise: true,
    });
    if (res && res.exceptionDetails) {
      console.error("prep failed: " + JSON.stringify(res.exceptionDetails.text || res.exceptionDetails));
    } else if (res && res.result && res.result.value !== undefined) {
      console.log("prep -> " + JSON.stringify(res.result.value));
    }
  }

  const shot = await cdp.send("Page.captureScreenshot", { format: "png" });
  fs.writeFileSync(out, Buffer.from(shot.data, "base64"));
  console.log("saved " + out + " (" + fs.statSync(out).size + " bytes)");
  cdp.close();
}

main().catch((err) => {
  console.error(String(err && err.message ? err.message : err));
  process.exit(1);
});
