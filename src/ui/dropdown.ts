import { el, icons, svgIcon } from "./dom";

export interface DropdownOption {
  value: string;
  label: string;
}

export interface DropdownOptions {
  id?: string;
  value: string;
  options: DropdownOption[];
  ariaLabel?: string;
  onChange?: (value: string) => void;
}

/**
 * WinUI 3 风格下拉：自绘弹出层（原生 select 无法定制选项样式）。
 *
 * 弹出层挂在 document.body 并用 fixed 定位，
 * 避免卡片入场动画的 transform 造成层叠上下文、把浮层闷在卡片里。
 */
export function dropdown(opts: DropdownOptions): HTMLElement {
  let value = opts.value;
  let open = false;
  let activeIndex = Math.max(
    0,
    opts.options.findIndex((option) => option.value === value),
  );

  const valueText = el("span", { class: "dd-value" });
  const button = el(
    "button",
    { class: "dd-btn", type: "button", "aria-haspopup": "listbox", "aria-expanded": "false" },
    valueText,
    el("span", { class: "dd-chevron" }, svgIcon(icons.chevronDown, 14)),
  ) as HTMLButtonElement;
  if (opts.id) button.id = opts.id;
  if (opts.ariaLabel) button.setAttribute("aria-label", opts.ariaLabel);

  const pop = el("div", { class: "dd-pop hidden", role: "listbox" }) as HTMLElement;
  const optionButtons: HTMLButtonElement[] = [];

  const syncLabel = () => {
    const match = opts.options.find((option) => option.value === value);
    valueText.textContent = match ? match.label : "";
    optionButtons.forEach((option, index) => {
      option.setAttribute("aria-selected", String(index === activeIndex));
    });
  };

  for (const [index, option] of opts.options.entries()) {
    const row = el(
      "button",
      {
        class: "dd-opt",
        type: "button",
        role: "option",
        onclick: (event: MouseEvent) => {
          event.stopPropagation();
          choose(index);
        },
      },
      el("span", { text: option.label }),
      el("span", { class: "tick" }, svgIcon(icons.check, 14)),
    ) as HTMLButtonElement;
    optionButtons.push(row);
    pop.append(row);
  }

  const place = () => {
    const rect = button.getBoundingClientRect();
    const gap = 4;
    pop.style.width = `${rect.width}px`;
    pop.style.left = `${Math.round(rect.left)}px`;
    // 先放到按钮下方，空间不够时翻到上方
    pop.style.top = "0px";
    const height = pop.offsetHeight;
    const below = window.innerHeight - rect.bottom;
    const top =
      below < height + gap && rect.top > below ? rect.top - height - gap : rect.bottom + gap;
    pop.style.top = `${Math.round(top)}px`;
  };

  const onViewportChange = () => {
    if (open) place();
  };
  window.addEventListener("resize", onViewportChange);
  window.addEventListener("scroll", onViewportChange, true);

  function close(returnFocus = false) {
    if (!open) return;
    open = false;
    pop.classList.add("hidden");
    button.setAttribute("aria-expanded", "false");
    if (returnFocus) button.focus();
  }

  function show() {
    if (open) return;
    open = true;
    pop.classList.remove("hidden");
    button.setAttribute("aria-expanded", "true");
    place();
    optionButtons[activeIndex]?.focus();
  }

  function choose(index: number) {
    activeIndex = index;
    value = opts.options[index]?.value ?? value;
    syncLabel();
    close(true);
    opts.onChange?.(value);
  }

  button.addEventListener("click", (event) => {
    event.stopPropagation();
    if (open) close(true);
    else show();
  });

  button.addEventListener("keydown", (event) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!open) show();
    }
  });

  pop.addEventListener("keydown", (event) => {
    const key = event.key;
    if (key === "Escape") {
      event.preventDefault();
      close(true);
    } else if (key === "ArrowDown") {
      event.preventDefault();
      activeIndex = Math.min(opts.options.length - 1, activeIndex + 1);
      optionButtons[activeIndex]?.focus();
      syncLabel();
    } else if (key === "ArrowUp") {
      event.preventDefault();
      activeIndex = Math.max(0, activeIndex - 1);
      optionButtons[activeIndex]?.focus();
      syncLabel();
    } else if (key === "Enter" || key === " ") {
      event.preventDefault();
      choose(activeIndex);
    }
  });

  document.addEventListener("click", (event) => {
    const target = event.target as Node | null;
    if (!target) return;
    if (!root.contains(target) && !pop.contains(target)) close(false);
  });

  document.body.append(pop);

  const root = el("div", { class: "dd" }, button);
  syncLabel();
  return root;
}
