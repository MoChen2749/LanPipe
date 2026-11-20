// 在应用 WebView 内执行表达式并打印 JSON 结果。
// 用法: node tools/ui-eval.mjs <port> "<js 表达式>"
import { readFileSync } from "node:fs";
const [port, raw] = process.argv.slice(2);
const expr = raw?.startsWith("@") ? readFileSync(raw.slice(1), "utf8") : raw;
if (!port || !expr) {
  console.error("usage: node tools/ui-eval.mjs <port> <expression>");
  process.exit(1);
}

const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = list.find((t) => t.type === "page" && t.webSocketDebuggerUrl);
if (!page) {
  console.error("no page target");
  process.exit(2);
}

const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  ws.onopen = resolve;
  ws.onerror = reject;
});

let id = 0;
function call(method, params = {}) {
  return new Promise((resolve, reject) => {
    const mid = ++id;
    const onMessage = (event) => {
      const msg = JSON.parse(event.data);
      if (msg.id !== mid) return;
      ws.removeEventListener("message", onMessage);
      if (msg.error) reject(new Error(msg.error.message));
      else resolve(msg.result);
    };
    ws.addEventListener("message", onMessage);
    ws.send(JSON.stringify({ id: mid, method, params }));
  });
}

await call("Runtime.enable");
const r = await call("Runtime.evaluate", {
  expression: expr,
  returnByValue: true,
  awaitPromise: true,
});
if (r.exceptionDetails) {
  console.error("exception:", r.exceptionDetails.text, r.exceptionDetails.exception?.description ?? "");
  process.exit(3);
}
console.log(JSON.stringify(r.result?.value, null, 2));
ws.close();
