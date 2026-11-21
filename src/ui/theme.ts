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
  syncMaterialBackdrop();
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

/**
 * 系统材质按系统主题取色，无法按应用主题重新调色。
 * 只有「应用主题与系统主题一致」时才启用材质；强制切到相反主题时退回纯色，
 * 避免浅色材质 + 深色 UI 的脏灰。
 */
export function syncMaterialBackdrop(): void {
  const root = document.documentElement;
  const name = root.dataset.backdropName ?? "";
  const mode = (getState().settings?.theme ?? "system") as ThemeMode;
  const matchesSystem = resolve(mode) === (media.matches ? "dark" : "light");
  if (!name || name === "none" || (mode !== "system" && !matchesSystem)) {
    delete root.dataset.backdrop;
    return;
  }
  root.dataset.backdrop = name;
}
