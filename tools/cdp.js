// Drive the running Clew window over the WebView2 CDP port.
//
//   node tools/cdp.js "<js expression>"
//
// Requires the app to be started with
//   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222

const port = process.env.CLEW_CDP_PORT || 9222;

async function main() {
  let expression = process.argv[2];
  if (!expression) {
    console.error('usage: node tools/cdp.js "<js expression>"  |  node tools/cdp.js @file.js');
    process.exit(2);
  }
  if (expression.startsWith("@")) {
    expression = require("node:fs").readFileSync(expression.slice(1), "utf8");
  }

  let targets;
  for (let i = 0; i < 40; i++) {
    try {
      targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
      if (targets.length) break;
    } catch {
      /* not up yet */
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  if (!targets || !targets.length) {
    console.error("no CDP target on port " + port);
    process.exit(1);
  }

  const page = targets.find((t) => t.type === "page") || targets[0];
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    ws.addEventListener("open", resolve, { once: true });
    ws.addEventListener("error", reject, { once: true });
  });

  const id = 1;
  const result = new Promise((resolve) => {
    ws.addEventListener("message", (ev) => {
      const msg = JSON.parse(ev.data);
      if (msg.id === id) resolve(msg);
    });
  });

  ws.send(
    JSON.stringify({
      id,
      method: "Runtime.evaluate",
      params: { expression, returnByValue: true, awaitPromise: true },
    }),
  );

  const msg = await result;
  ws.close();

  if (msg.error) {
    console.error(JSON.stringify(msg.error, null, 2));
    process.exit(1);
  }
  const value = msg.result?.result?.value;
  const exc = msg.result?.exceptionDetails;
  if (exc) {
    console.error(exc.exception?.description || exc.text);
    process.exit(1);
  }
  console.log(typeof value === "string" ? value : JSON.stringify(value, null, 2));
}

main();
