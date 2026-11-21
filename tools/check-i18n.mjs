// 检查 t("key") 与 public/i18n/zh-CN.xml 的键是否一一对应（缺失/多余都报出来）。
// 用法: node tools/check-i18n.mjs
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const root = "src";
const files = [];
const walk = (dir) => {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) walk(path);
    else if (/\.(ts|tsx)$/.test(name)) files.push(path);
  }
};
walk(root);

const used = new Set();
for (const file of files) {
  const text = readFileSync(file, "utf8");
  for (const match of text.matchAll(/\bt\(\s*"([^"]+)"/g)) used.add(match[1]);
}

const xml = readFileSync("public/i18n/zh-CN.xml", "utf8");
const defined = new Set([...xml.matchAll(/key="([^"]+)"/g)].map((m) => m[1]));

const missing = [...used].filter((key) => !defined.has(key)).sort();
const unused = [...defined].filter((key) => !used.has(key)).sort();

console.log(`keys: used=${used.size} defined=${defined.size}`);
if (missing.length) console.log("missing:", missing.join(", "));
if (unused.length) console.log("unused:", unused.join(", "));
if (!missing.length) console.log("missing: none");
process.exit(missing.length ? 1 : 0);
