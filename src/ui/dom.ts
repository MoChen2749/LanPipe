type Child = Node | string | number | null | undefined | false;

type Props = {
  class?: string;
  text?: string;
  html?: string;
  style?: string;
  dataset?: Record<string, string>;
} & Record<string, unknown>;

const EVENT_RE = /^on([A-Z].*)$/;

/** 轻量 DOM 构造器：el("div", { class: "x", onclick: fn }, "文本", child) */
export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props?: Props | null,
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (props) {
    for (const [key, value] of Object.entries(props)) {
      if (value === undefined || value === null || value === false) continue;
      if (key === "class") node.className = String(value);
      else if (key === "text") node.textContent = String(value);
      else if (key === "html") node.innerHTML = String(value);
      else if (key === "style") node.setAttribute("style", String(value));
      else if (key === "dataset") {
        for (const [dk, dv] of Object.entries(value as Record<string, string>)) {
          node.dataset[dk] = dv;
        }
      } else {
        const ev = EVENT_RE.exec(key);
        if (ev) {
          node.addEventListener(ev[1].toLowerCase(), value as EventListener);
        } else if (key in node && typeof value !== "string") {
          (node as unknown as Record<string, unknown>)[key] = value;
        } else {
          node.setAttribute(key, String(value));
        }
      }
    }
  }
  node.append(...children.flatMap(toNode));
  return node;
}

function toNode(child: Child): Node[] {
  if (child === null || child === undefined || child === false) return [];
  if (child instanceof Node) return [child];
  return [document.createTextNode(String(child))];
}

export function clear(node: HTMLElement): void {
  node.replaceChildren();
}

export function svgIcon(paths: string[], size = 20): SVGSVGElement {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("width", String(size));
  svg.setAttribute("height", String(size));
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "1.8");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  for (const d of paths) {
    const p = document.createElementNS("http://www.w3.org/2000/svg", "path");
    p.setAttribute("d", d);
    svg.append(p);
  }
  return svg;
}

/** 常用线性图标（Fluent 风格，24 网格） */
export const icons = {
  send: ["M4 12l16 -8 -6 16 -4 -6z"],
  receive: ["M12 4v11", "M7 10l5 5 5 -5", "M4 20h16"],
  history: ["M12 8v5l3 2", "M3 12a9 9 0 1 0 3 -6.7"],
  settings: [
    "M12 15a3 3 0 1 0 0 -6 3 3 0 0 0 0 6z",
    "M19 12a7 7 0 0 0 -.1 -1.2l2 -1.6 -2 -3.4 -2.4 1a7 7 0 0 0 -2 -1.2L14 3h-4l-.4 2.6a7 7 0 0 0 -2 1.2l-2.4 -1 -2 3.4 2 1.6A7 7 0 0 0 5 12c0 .4 0 .8.1 1.2l-2 1.6 2 3.4 2.4 -1a7 7 0 0 0 2 1.2L10 21h4l.4 -2.6a7 7 0 0 0 2 -1.2l2.4 1 2 -3.4 -2 -1.6c.1 -.4.2 -.8.2 -1.2z",
  ],
  folder: ["M3 7a2 2 0 0 1 2 -2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1 -2 2H5a2 2 0 0 1 -2 -2z"],
  file: ["M7 3h7l4 4v14H7z", "M14 3v4h4"],
  device: ["M4 5h16v10H4z", "M9 19h6", "M12 15v4"],
  up: ["M12 19V5", "M6 11l6 -6 6 6"],
  refresh: ["M20 12a8 8 0 1 1 -2.3 -5.7", "M20 4v5h-5"],
  link: ["M10 14a4 4 0 0 0 5.7 0l3 -3a4 4 0 0 0 -5.7 -5.7l-1 1", "M14 10a4 4 0 0 0 -5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1 -1"],
  check: ["M5 13l4 4 10 -10"],
  info: ["M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0 -18", "M12 11v6", "M12 7.5v.5"],
  chevronDown: ["M6 9l6 6 6 -6"],
  chevronUp: ["M6 15l6 -6 6 6"],
  alert: ["M12 4l9 16H3z", "M12 10v4", "M12 17v.5"],
  pulse: ["M3 12h4l2 6 4 -12 2 6h6"],
  minimize: ["M5 12h14"],
  maximize: ["M5 5h14v14H5z"],
  restore: ["M8 8h11v11H8z", "M5 16V5h11"],
  close: ["M6 6l12 12", "M18 6L6 18"],
};


/** 重建 DOM 前后保留焦点与输入内容（整页重渲染时使用）。 */
export function withFocusPreserved(render: () => void): void {
  const active = document.activeElement as HTMLInputElement | HTMLTextAreaElement | null;
  const id = active?.id || null;
  const value = active && "value" in active ? active.value : null;
  const selStart = active && "selectionStart" in active ? active.selectionStart : null;
  const selEnd = active && "selectionEnd" in active ? active.selectionEnd : null;
  render();
  if (!id) return;
  const next = document.getElementById(id) as HTMLInputElement | HTMLTextAreaElement | null;
  if (!next) return;
  if (value !== null && next.value !== value) next.value = value;
  next.focus();
  if (selStart !== null && selEnd !== null) {
    try {
      next.setSelectionRange(selStart, selEnd);
    } catch {
      /* 部分 input 类型不支持 */
    }
  }
}
