import { el, icons, svgIcon } from "./dom";

export interface StepperOptions {
  id?: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  ariaLabel?: string;
  onChange?: (value: number) => void;
}

/** WinUI 3 风格 NumberBox：隐藏原生 spinner，自绘上下按钮。 */
export function stepper(opts: StepperOptions): HTMLElement {
  const step = opts.step ?? 1;
  const clamp = (value: number) => Math.min(opts.max, Math.max(opts.min, Math.round(value)));

  const input = el("input", {
    type: "text",
    inputmode: "numeric",
    value: String(opts.value),
    autocomplete: "off",
  }) as HTMLInputElement;
  if (opts.id) input.id = opts.id;
  if (opts.ariaLabel) input.setAttribute("aria-label", opts.ariaLabel);

  const commit = (next: number) => {
    const value = clamp(next);
    input.value = String(value);
    opts.onChange?.(value);
  };

  const up = el(
    "button",
    { type: "button", tabindex: "-1", "aria-label": "increase", onclick: () => commit(Number(input.value) + step) },
    svgIcon(icons.chevronUp, 12),
  );
  const down = el(
    "button",
    { type: "button", tabindex: "-1", "aria-label": "decrease", onclick: () => commit(Number(input.value) - step) },
    svgIcon(icons.chevronDown, 12),
  );

  input.addEventListener("change", () => {
    const parsed = Number(input.value.replace(/[^0-9]/g, ""));
    commit(Number.isFinite(parsed) && input.value.trim() !== "" ? parsed : opts.value);
  });
  input.addEventListener("keydown", (event) => {
    if (event.key === "ArrowUp") {
      event.preventDefault();
      commit(Number(input.value) + step);
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      commit(Number(input.value) - step);
    } else if (event.key === "Enter") {
      input.blur();
    }
  });

  return el("div", { class: "stepper" }, input, el("div", { class: "step" }, up, down));
}
