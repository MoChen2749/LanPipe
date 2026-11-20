import { getState, subscribe } from "../state/store";

export type ThemeMode = "system" | "light" | "dark";

const media = window.matchMedia("(prefers-color-scheme: dark)");

function resolve(mode: ThemeMode): "light" | "dark" {
  if (mode === "dark") return "dark";
  if (mode === "light") return "light";
  return media.matches ? "dark" : "light";
}

/** 应用主题；system 跟随系统并监听系统切换。 */
export function applyTheme(mode: ThemeMode): void {
  const root = document.documentElement;
  if (mode === "system") {
    delete root.dataset.theme;
  } else {
    root.dataset.theme = resolve(mode);
  }
  root.style.colorScheme = resolve(mode);
}

/** 订阅 store 的设置变化，自动同步主题。 */
export function watchTheme(): () => void {
  const sync = () => {
    const settings = getState().settings;
    applyTheme((settings?.theme as ThemeMode | undefined) ?? "system");
  };
  const onSystem = () => {
    const settings = getState().settings;
    if ((settings?.theme ?? "system") === "system") sync();
  };
  media.addEventListener("change", onSystem);
  const unsub = subscribe(sync);
  return () => {
    media.removeEventListener("change", onSystem);
    unsub();
  };
}
