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
  void refreshMaterial(mode);
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
 * 重新让 DWM 按「当前应用主题」选择 Mica 的浅色/深色变体。
 * 材质由后端 set_effects 应用，前端只负责把结果写回 dataset 供 CSS 分层使用。
 */
export function syncMaterialBackdrop(): void {
  const root = document.documentElement;
  const name = root.dataset.backdropName ?? "";
  if (!name || name === "none") {
    delete root.dataset.backdrop;
    return;
  }
  root.dataset.backdrop = name;
}

/** 主题切换后重算材质（MicaLight / MicaDark），失败保持纯色。 */
export async function refreshMaterial(mode: ThemeMode): Promise<void> {
  const root = document.documentElement;
  try {
    const { api } = await import("../api/tauri");
    const name = await api.backdropApply(mode);
    root.dataset.backdropName = name;
    syncMaterialBackdrop();
  } catch {
    delete root.dataset.backdrop;
  }
}
