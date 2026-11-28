import { el, icons, svgIcon } from "./dom";

/**
 * Android 底部 ActionSheet（Material 3 语义 + Fluent 皮肤）。
 *
 * 只在 Android 分支使用；样式定义在 src/styles/android.css。
 * 桌面端仍使用自绘下拉（ui/dropdown.ts）与内联按钮，不受影响。
 */
export interface SheetItem {
  label: string;
  danger?: boolean;
  /** 选择类面板（sheetSelect）中标记当前值 */
  selected?: boolean;
  onSelect?: () => void;
}

interface ActiveSheet {
  close(): void;
  scrim: HTMLElement;
  sheet: HTMLElement;
}

let active: ActiveSheet | null = null;

export function closeSheet(): void {
  active?.close();
}

/** 打开底部操作面板；再次调用会先关闭上一个。 */
export function actionSheet(title: string | null, items: SheetItem[]): void {
  closeSheet();

  const scrim = el("div", { class: "sheet-scrim" });
  const sheet = el("div", {
    class: "sheet",
    role: "dialog",
    "aria-modal": "true",
    "aria-label": title ?? "",
  });
  sheet.append(el("span", { class: "sheet-handle", "aria-hidden": "true" }));
  if (title) sheet.append(el("div", { class: "sheet-title", text: title }));
  for (const item of items) {
    sheet.append(
      el(
        "button",
        {
          class: `sheet-item${item.danger ? " danger" : ""}`,
          type: "button",
          "aria-selected": item.selected === undefined ? undefined : String(item.selected),
          onclick: () => {
            close();
            item.onSelect?.();
          },
        },
        el("span", { class: "sheet-item-label", text: item.label }),
        item.selected ? el("span", { class: "tick" }, svgIcon(icons.check, 16)) : null,
      ),
    );
  }

  const onKey = (event: KeyboardEvent) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    close();
  };

  function close(): void {
    scrim.remove();
    sheet.remove();
    document.removeEventListener("keydown", onKey, true);
    document.documentElement.classList.remove("sheet-open");
    if (active && active.close === close) active = null;
  }

  scrim.addEventListener("click", close);
  document.documentElement.classList.add("sheet-open");
  document.addEventListener("keydown", onKey, true);
  document.body.append(scrim, sheet);
  active = { close, scrim, sheet };
}

/** 用底部面板替代下拉选择（设置页的压缩/主题/语言）。 */
export function sheetSelect(opts: {
  title?: string;
  value: string;
  options: { value: string; label: string }[];
  onChange: (value: string) => void;
}): void {
  actionSheet(
    opts.title ?? null,
    opts.options.map((option) => ({
      label: option.label,
      selected: option.value === opts.value,
      onSelect: () => opts.onChange(option.value),
    })),
  );
}
