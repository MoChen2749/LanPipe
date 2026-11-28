/**
 * 平台检测：Windows 桌面（Tauri）与 Android（Tauri 移动端 WebView）共用同一套前端。
 *
 * 判定以 UA 为主——Android WebView 的 UA 必定包含 "Android"；
 * 另外支持 `?platform=android|desktop` 查询参数，方便在桌面浏览器里预览 Android 布局。
 * 所有 Android 样式都挂在 `<html data-platform="android">` 下，默认（桌面）路径保持原样。
 */
export type Platform = "desktop" | "android";

const forced = new URLSearchParams(window.location.search).get("platform");

export const platform: Platform =
  forced === "android"
    ? "android"
    : forced === "desktop"
      ? "desktop"
      : /Android/i.test(navigator.userAgent)
        ? "android"
        : "desktop";

export const isAndroid = platform === "android";

/** 粗指针/无 hover 设备（Android 手机和平板恒为 true，桌面触屏 Windows 设备也可能为 true）。 */
export const isTouch =
  typeof window.matchMedia === "function"
    ? window.matchMedia("(hover: none) and (pointer: coarse)").matches
    : false;

export function supportsBackdropFilter(): boolean {
  return (
    typeof CSS !== "undefined" &&
    typeof CSS.supports === "function" &&
    CSS.supports("backdrop-filter", "blur(1px)")
  );
}

/**
 * 首屏渲染前把平台标记写到 `<html>`：
 * - data-platform：CSS 分支（android.css 全部选择器都带这个前缀）
 * - data-blur：是否允许毛玻璃材质（关闭时退回纯色分层）
 */
export function applyPlatform(): void {
  const root = document.documentElement;
  root.dataset.platform = platform;
  root.dataset.blur = supportsBackdropFilter() && isAndroid ? "1" : "0";
}
