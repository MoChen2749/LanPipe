import "./styles/index.css";

import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { t, loadLocale } from "./i18n";
import { applyVersion, buildShell, type Route, type Shell } from "./layout/shell";
import { mountSend } from "./views/send";
import { mountReceive } from "./views/receive";
import { mountHistory } from "./views/history";
import { mountSettings } from "./views/settings";
import { mountDiagnostics } from "./views/diagnostics";
import { applyTransferEvent, getState, setPeers, subscribe } from "./state/store";
import { loadBootstrap, refreshPeers, setToastHandler, stagePaths } from "./actions";
import { api, events } from "./api/tauri";
import { syncMaterialBackdrop, watchTheme } from "./ui/theme";
import { applyPlatform, isAndroid } from "./platform";
import { initAndroidInsets } from "./ui/android-insets";

// 首屏渲染之前先落平台标记：CSS 的 Android 分支全部挂在 [data-platform="android"] 下
applyPlatform();

const ROUTES: Route[] = ["send", "receive", "history", "diagnostics", "settings"];
// Android 返回栈（桌面不使用）
const routeStack: Route[] = ["send"];

let shell: Shell;
let cleanup: (() => void) | null = null;
let currentRoute: Route = "send";
let lastBackAt = 0;

async function main(): Promise<void> {
  const root = document.getElementById("app");
  if (!root) return;

  await loadLocale("zh-CN");
  shell = buildShell(root, (route) => navigateTab(route));
  setToastHandler((text) => shell.showToast(text));
  subscribe(syncChrome);

  events.peers((peers) => setPeers(peers));
  events.transfer((event) => applyTransferEvent(event));

  await loadBootstrap();

  if (isAndroid) {
    // 原生侧可能还没提供 insets 事件：内部全部容错，CSS 用 env() 兜底
    void initAndroidInsets();
    setupBackNavigation();
  } else {
    void api
      .windowShadow()
      .then((on) => {
        document.documentElement.dataset.shadow = on ? "1" : "0";
      })
      .catch(() => {
        document.documentElement.dataset.shadow = "0";
      });
    applyBackdrop();
  }

  watchTheme();
  navigate("send", false);
  syncChrome();
  void refreshPeers();
  window.setInterval(() => void refreshPeers(), 6000);

  try {
    const versionNode = document.getElementById("app-version");
    if (versionNode) applyVersion(versionNode, await getVersion());
  } catch {
    /* 浏览器预览环境忽略 */
  }

  if (isAndroid) return; // Android 没有拖放

  try {
    await getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload;
      if (payload.type === "drop" && payload.paths.length > 0) {
        void stagePaths(payload.paths);
      }
      const zone = document.getElementById("dropzone");
      if (zone) zone.classList.toggle("drag-over", payload.type === "over");
    });
  } catch {
    /* 浏览器环境无原生拖放 */
  }
}

/** 底部 Tab 切换：替换返回栈顶，不产生新的返回历史。 */
function navigateTab(route: Route): void {
  if (!shell) return;
  if (route === currentRoute) return;
  routeStack.length = 0;
  routeStack.push(route);
  if (isAndroid) replaceRouteState(route);
  navigate(route, false);
}

function navigate(route: Route, push = true): void {
  if (!shell) return;
  const from = ROUTES.indexOf(currentRoute);
  const to = ROUTES.indexOf(route);
  const changed = route !== currentRoute;
  if (isAndroid && push && changed) {
    routeStack.push(route);
    pushRouteState(route);
  }
  currentRoute = route;
  cleanup?.();
  cleanup = null;
  shell.setActive(route);
  shell.content.replaceChildren();
  shell.content.classList.remove("view-enter", "view-slide-forward", "view-slide-back");
  void shell.content.offsetWidth; // 重启动画
  shell.content.classList.add("view-enter");
  if (to > from) shell.content.classList.add("view-slide-forward");
  else if (to < from) shell.content.classList.add("view-slide-back");
  window.setTimeout(() => {
    shell.content.classList.remove("view-slide-forward", "view-slide-back");
  }, 400);

  if (route === "send") cleanup = mountSend(shell.content);
  else if (route === "receive") cleanup = mountReceive(shell.content, navigate);
  else if (route === "history") cleanup = mountHistory(shell.content);
  else if (route === "diagnostics") cleanup = mountDiagnostics(shell.content);
  else cleanup = mountSettings(shell.content);
}

// ---------- Android 返回键（路由栈 + WebView history） ----------

function pushRouteState(route: Route): void {
  try {
    history.pushState({ route }, "");
  } catch {
    /* 某些 WebView 环境不支持 pushState */
  }
}

function replaceRouteState(route: Route): void {
  try {
    history.replaceState({ route }, "");
  } catch {
    /* 忽略 */
  }
}

function setupBackNavigation(): void {
  pushRouteState(currentRoute);
  window.addEventListener("popstate", onPopState);
  // 原生侧若只派发 CustomEvent（而不是调用 WebView.goBack），走同一条历史路径
  window.addEventListener("android:back", () => {
    try {
      history.back();
    } catch {
      /* 忽略 */
    }
  });
}

function onPopState(event: PopStateEvent): void {
  if (!isAndroid) return;
  const data = event.state as { route?: Route } | null;
  const target = data?.route ?? "send";
  if (target === currentRoute) {
    // 已在最早一页：再按一次返回 = 退出确认；同时补回一条历史记录，避免 WebView 直接退出应用
    confirmExit();
    pushRouteState(currentRoute);
    return;
  }
  const index = routeStack.lastIndexOf(target);
  if (index >= 0) routeStack.length = index + 1;
  else routeStack.push(target);
  navigate(target, false);
}

function confirmExit(): void {
  const now = Date.now();
  if (now - lastBackAt <= 2000) {
    lastBackAt = 0;
    void exitApp();
    return;
  }
  lastBackAt = now;
  if (shell) shell.showToast(t("action.backAgainToExit"));
}

/** 退出应用：优先调用原生命令（原生侧实现后生效），最后退回窗口关闭。 */
async function exitApp(): Promise<void> {
  try {
    await invoke("android_exit");
    return;
  } catch {
    /* 原生命令尚未实现 */
  }
  try {
    await getCurrentWindow().close();
    return;
  } catch {
    /* 忽略 */
  }
  try {
    window.close();
  } catch {
    /* 忽略 */
  }
}

async function applyBackdrop(): Promise<void> {
  try {
    const info = await api.backdropInfo();
    // 先记住系统支持的材质，再由 syncMaterialBackdrop 决定是否启用
    // （应用主题与系统主题不一致时不启用，避免脏灰）
    document.documentElement.dataset.backdropName = info || "none";
    syncMaterialBackdrop();
  } catch {
    /* 浏览器预览 / 旧后端忽略 */
  }
}

function syncChrome(): void {
  if (!shell) return;
  const state = getState();
  if (state.local) shell.setIdentity(state.local.name, state.local.shortId ?? "000");
  const tr = state.transfer;
  if (tr && tr.state === "active") {
    shell.setStatus(t(tr.direction === "send" ? "status.sending" : "status.receiving"), "busy");
  } else if (tr && tr.state === "done") {
    shell.setStatus(t("status.done"), "ok");
  } else if (tr && tr.state === "failed") {
    shell.setStatus(t("status.failed"), "error");
  } else {
    shell.setStatus(t("status.discovering", { count: String(state.peers.length) }), "ok");
  }
}

void main();
