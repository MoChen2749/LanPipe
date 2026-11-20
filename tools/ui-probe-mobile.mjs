// 移动端视口探针：设置 390x844 后读取布局与指示条状态。
const port = process.argv[2] ?? "9333";
const exprFile = process.argv[3];
import { readFileSync } from "node:fs";
const expression = readFileSync(exprFile, "utf8");

const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = list.find((t) => t.type === "page" && t.webSocketDebuggerUrl);
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r, j) => {
  ws.onopen = r;
  ws.onerror = j;
});

let id = 0;
const call = (method, params = {}) =>
  new Promise((resolve, reject) => {
    const mid = ++id;
    const on = (e) => {
      const m = JSON.parse(e.data);
      if (m.id !== mid) return;
      ws.removeEventListener("message", on);
      if (m.error) reject(new Error(`${method}: ${m.error.message}`));
      else resolve(m.result);
    };
    ws.addEventListener("message", on);
    ws.send(JSON.stringify({ id: mid, method, params }));
  });

await call("Runtime.enable");
await call("Emulation.setDeviceMetricsOverride", {
  width: 390,
  height: 844,
  deviceScaleFactor: 2,
  mobile: true,
  screenWidth: 390,
  screenHeight: 844,
});
await new Promise((r) => setTimeout(r, 400));
const r = await call("Runtime.evaluate", {
  expression,
  returnByValue: true,
  awaitPromise: true,
});
console.log(JSON.stringify(r.result?.value, null, 2));
await call("Emulation.clearDeviceMetricsOverride");
ws.close();
