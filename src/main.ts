import "./styles/index.css";

import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWebview } from "@tauri-apps/api/webview";
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
import { watchTheme } from "./ui/theme";

let shell: Shell;
let cleanup: (() => void) | null = null;

async function main(): Promise<void> {
  const root = document.getElementById("app");
  if (!root) return;

  await loadLocale("zh-CN");
  shell = buildShell(root, (route) => navigate(route));
  setToastHandler((text) => shell.showToast(text));
  subscribe(syncChrome);

  events.peers((peers) => setPeers(peers));
  events.transfer((event) => applyTransferEvent(event));

  await loadBootstrap();
  void api
    .windowShadow()
    .then((on) => {
      document.documentElement.dataset.shadow = on ? "1" : "0";
    })
    .catch(() => {
      document.documentElement.dataset.shadow = "0";
    });
  watchTheme();
  applyBackdrop();
  navigate("send");
  syncChrome();
  void refreshPeers();
  window.setInterval(() => void refreshPeers(), 6000);

  try {
    const versionNode = document.getElementById("app-version");
    if (versionNode) applyVersion(versionNode, await getVersion());
  } catch {
    /* 浏览器预览环境忽略 */
  }

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

const ROUTES: Route[] = ["send", "receive", "history", "diagnostics", "settings"];
let currentRoute: Route = "send";

function navigate(route: Route): void {
  if (!shell) return;
  const from = ROUTES.indexOf(currentRoute);
  const to = ROUTES.indexOf(route);
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

async function applyBackdrop(): Promise<void> {
  try {
    const info = await api.backdropInfo();
    if (info && info !== "none") {
      document.documentElement.dataset.backdrop = info;
    }
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
