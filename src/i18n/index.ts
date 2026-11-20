/**
 * 极简 i18n：从 public/i18n/<locale>.xml 读取文案，渲染时调用 t(key, vars)。
 * 新增语言只需增加 XML 文件并在 availableLocales 注册。
 */

export interface LocaleInfo {
  code: string;
  label: string;
}

export const availableLocales: LocaleInfo[] = [
  { code: "zh-CN", label: "中文（简体）" },
];

type Dict = Record<string, string>;

let dict: Dict = {};
let current = "zh-CN";

export function getLocale(): string {
  return current;
}

export async function loadLocale(code = current): Promise<void> {
  const res = await fetch(`/i18n/${code}.xml`, { cache: "no-store" });
  if (!res.ok) throw new Error(`加载语言包失败：${code}`);
  const xml = new DOMParser().parseFromString(await res.text(), "application/xml");
  const next: Dict = {};
  xml.querySelectorAll("string").forEach((node) => {
    const key = node.getAttribute("key");
    if (key) next[key] = (node.textContent ?? "").trim();
  });
  dict = next;
  current = code;
  document.documentElement.lang = code;
}

export function t(key: string, vars?: Record<string, string | number>): string {
  let text = dict[key] ?? key;
  if (vars) {
    for (const [name, value] of Object.entries(vars)) {
      text = text.replaceAll(`{${name}}`, String(value));
    }
  }
  return text;
}

export function hasKey(key: string): boolean {
  return key in dict;
}
