import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/**
 * 原生系统栏安全区（Android）。
 *
 * 原生侧（Kotlin MainActivity）通过 WindowInsets 派发：
 * - 事件 `android:insets`，payload `{ top, bottom, ime }`（单位 dp，等价 CSS px）
 * - 命令 `android_insets`，返回首帧同步值（避免首帧闪烁）
 *
 * 原生侧可能尚未实现（团队并行开发），因此这里所有调用都容错；
 * CSS 里用 env(safe-area-inset-*) 兜底，监听失败不会影响启动。
 */
export interface AndroidInsets {
  top?: number;
  bottom?: number;
  ime?: number;
}

const root = document.documentElement;
let started = false;

function clampInset(value: unknown): number | null {
  const num = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(num) || num < 0) return null;
  return Math.round(num);
}

/** 把 insets 写到 <html> 的 CSS 变量上（幂等，非法值忽略）。 */
export function applyAndroidInsets(insets: AndroidInsets | null | undefined): void {
  if (!insets) return;
  const top = clampInset(insets.top);
  if (top !== null) root.style.setProperty("--android-inset-top", `${top}px`);
  const bottom = clampInset(insets.bottom);
  if (bottom !== null) root.style.setProperty("--android-inset-bottom", `${bottom}px`);
  const ime = clampInset(insets.ime);
  if (ime !== null) root.style.setProperty("--android-ime", `${ime}px`);
}

/**
 * 订阅原生 insets 事件并读取首帧值。重复调用无副作用；
 * 任何失败都静默忽略（原生侧未实现 / 浏览器预览）。
 */
export async function initAndroidInsets(): Promise<void> {
  if (started) return;
  started = true;
  try {
    await listen<AndroidInsets>("android:insets", (event) => applyAndroidInsets(event.payload));
  } catch {
    /* 原生事件尚未提供：交给 CSS 兜底 */
  }
  try {
    applyAndroidInsets(await invoke<AndroidInsets>("android_insets"));
  } catch {
    /* 原生命令尚未提供：交给 CSS 兜底 */
  }
}
