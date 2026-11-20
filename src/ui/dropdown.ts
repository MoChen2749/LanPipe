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

/** WinUI 3 风格下拉：自绘弹出层（原生 select 无法定制选项样式）。 */
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
    if (!root.contains(target)) close(false);
  });

  const root = el("div", { class: "dd" }, button, pop);
  syncLabel();
  return root;
}
