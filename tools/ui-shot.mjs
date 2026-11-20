// 通过 WebView2 DevTools 协议截取应用自身渲染（不依赖屏幕、不需要前台窗口）。
// 用法: node tools/ui-shot.mjs <port> <out.png> [navIndex] [--full]
// navIndex: 0 发送 1 接收 2 历史 3 设置；省略则截当前页。
const [port, out, navIndex, flag] = process.argv.slice(2);
if (!port || !out) {
  console.error("usage: node tools/ui-shot.mjs <port> <out.png> [navIndex] [--full]");
  process.exit(1);
}

const base = `http://127.0.0.1:${port}`;

async function targets() {
  const res = await fetch(`${base}/json/list`);
  return res.json();
}

function pickPage(list) {
  return list.find((t) => t.type === "page" && t.webSocketDebuggerUrl);
}

function connect(url) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(url);
    ws.onopen = () => resolve(ws);
    ws.onerror = (err) => reject(new Error(`ws error: ${err.message ?? "unknown"}`));
  });
}

let id = 0;
function send(ws, method, params = {}) {
  return new Promise((resolve, reject) => {
    const msgId = ++id;
    const onMessage = (event) => {
      let msg;
      try {
        msg = JSON.parse(event.data);
      } catch {
        return;
      }
      if (msg.id !== msgId) return;
      ws.removeEventListener("message", onMessage);
      if (msg.error) reject(new Error(`${method}: ${msg.error.message}`));
      else resolve(msg.result);
    };
    ws.addEventListener("message", onMessage);
    ws.send(JSON.stringify({ id: msgId, method, params }));
  });
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const list = await targets();
const page = pickPage(list);
if (!page) {
  console.error("no page target; is LANPIPE_CDP set and the window open?");
  process.exit(2);
}

const ws = await connect(page.webSocketDebuggerUrl);
await send(ws, "Page.enable");
await send(ws, "Runtime.enable");

if (navIndex !== undefined && navIndex !== "") {
  const expr = `(() => {
    const items = document.querySelectorAll(".nav-item");
    const target = items[${Number(navIndex)}];
    if (!target) return "missing nav item";
    target.click();
    return "clicked " + target.textContent.trim();
  })()`;
  const r = await send(ws, "Runtime.evaluate", { expression: expr, returnByValue: true });
  console.log("nav:", r.result?.value);
  await sleep(900);
}

if (process.env.SHOT_CLICK) {
  const expr = `(() => {
    const node = document.querySelector(${JSON.stringify(process.env.SHOT_CLICK)});
    if (!node) return "missing " + ${JSON.stringify(process.env.SHOT_CLICK)};
    node.click();
    return "clicked";
  })()`;
  const r = await send(ws, "Runtime.evaluate", { expression: expr, returnByValue: true });
  console.log("click:", r.result?.value);
  await sleep(500);
}

if (flag === "--tall") {
  // 临时放大视口，便于一次截完整页（应用用视口高度做滚动容器）。
  await send(ws, "Emulation.setDeviceMetricsOverride", {
    width: 1040,
    height: 1600,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await sleep(500);
}

// 清除残留 hover：把虚拟鼠标移到角落。
await send(ws, "Input.dispatchMouseEvent", { type: "mouseMoved", x: 1, y: 1, button: "none" });
await sleep(150);

const shot = await send(ws, "Page.captureScreenshot", {
  format: "png",
  captureBeyondViewport: false,
});
const { writeFileSync } = await import("node:fs");
writeFileSync(out, Buffer.from(shot.data, "base64"));
console.log("saved", out);
ws.close();
