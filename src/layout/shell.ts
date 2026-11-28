import { clear, el, icons, svgIcon } from "../ui/dom";
import { t } from "../i18n";
import { api } from "../api/tauri";
import { toast } from "../actions";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isAndroid } from "../platform";

export type Route = "send" | "receive" | "history" | "diagnostics" | "settings";

const NAV: { route: Route; icon: keyof typeof icons; label: string }[] = [
  { route: "send", icon: "send", label: "nav.send" },
  { route: "receive", icon: "receive", label: "nav.receive" },
  { route: "history", icon: "history", label: "nav.history" },
  { route: "diagnostics", icon: "pulse", label: "nav.diagnostics" },
  { route: "settings", icon: "settings", label: "nav.settings" },
];

export interface Shell {
  content: HTMLElement;
  setActive(route: Route): void;
  setIdentity(name: string, shortId: string): void;
  setStatus(text: string, kind: "ok" | "busy" | "error"): void;
  showToast(text: string): void;
}

export function buildShell(root: HTMLElement, onNavigate: (route: Route) => void): Shell {
  const navButtons = new Map<Route, HTMLButtonElement>();
  // Android：无系统标题栏按钮、无拖拽/双击最大化、无状态栏，导航改底部 Tab。
  const isDesktop = !isAndroid;

  const identity = el("div", { class: "identity", id: "identity-chip", title: "" });

  // ---------- 桌面专属：局域网信息气泡 ----------
  let infoPop: HTMLElement | null = null;
  let infoBtn: HTMLElement | null = null;
  if (isDesktop) {
    const pop = el("div", { class: "info-pop hidden", id: "info-pop" });
    infoPop = pop;
    const renderInfo = async () => {
      pop.replaceChildren(el("div", { class: "info-title", text: t("info.title") }));
      try {
        const info = await api.localInfo();
        const list = info.ipv4 && info.ipv4.length > 0 ? info.ipv4 : [];
        if (list.length === 0) {
          pop.append(el("div", { class: "muted", text: t("info.empty") }));
          return;
        }
        for (const ip of list) {
          pop.append(
            el("button", {
              class: "info-ip",
              type: "button",
              title: t("info.copy"),
              text: ip,
              onclick: () => {
                void navigator.clipboard.writeText(ip).then(() => toast(t("toast.copied", { text: ip })));
              },
            }),
          );
        }
        pop.append(el("div", { class: "muted info-hint", text: t("info.hint") }));
      } catch (err) {
        pop.append(el("div", { class: "muted", text: String(err) }));
      }
    };
    const btn = el(
      "button",
      {
        class: "icon-btn",
        id: "info-btn",
        title: t("info.title"),
        "aria-label": t("info.title"),
        onclick: (event: MouseEvent) => {
          event.stopPropagation();
          const hidden = pop.classList.toggle("hidden");
          if (!hidden) void renderInfo();
        },
      },
      svgIcon(icons.info, 16),
    );
    infoBtn = btn;
    document.addEventListener("click", (event) => {
      const target = event.target as Node | null;
      if (!target) return;
      if (!pop.contains(target) && target !== btn && !btn.contains(target)) {
        pop.classList.add("hidden");
      }
    });
  }

  // ---------- 桌面专属：自绘窗口按钮（窗口用 decorations(false)） ----------
  let win: ReturnType<typeof getCurrentWindow> | null = null;
  let winControls: HTMLElement | null = null;
  if (isDesktop) {
    const currentWin = getCurrentWindow();
    win = currentWin;
    const maxIcon = () =>
      document.documentElement.dataset.maximized === "1" ? icons.restore : icons.maximize;
    const maxBtn = el(
      "button",
      {
        class: "win-btn",
        type: "button",
        title: t("window.maximize"),
        "aria-label": t("window.maximize"),
        onclick: () => void currentWin.toggleMaximize(),
      },
      svgIcon(maxIcon(), 12),
    );
    winControls = el(
      "div",
      { class: "win-controls" },
      el(
        "button",
        {
          class: "win-btn",
          type: "button",
          title: t("window.minimize"),
          "aria-label": t("window.minimize"),
          onclick: () => void currentWin.minimize(),
        },
        svgIcon(icons.minimize, 12),
      ),
      maxBtn,
      el(
        "button",
        {
          class: "win-btn close",
          type: "button",
          title: t("window.close"),
          "aria-label": t("window.close"),
          onclick: () => void currentWin.close(),
        },
        svgIcon(icons.close, 12),
      ),
    );
    const syncMaximized = async () => {
      try {
        const maximized = await currentWin.isMaximized();
        document.documentElement.dataset.maximized = maximized ? "1" : "0";
        maxBtn.replaceChildren(svgIcon(maxIcon(), 12));
        maxBtn.title = maximized ? t("window.restore") : t("window.maximize");
        maxBtn.setAttribute("aria-label", maxBtn.title);
      } catch {
        /* 忽略：非 Tauri 环境 */
      }
    };
    void syncMaximized();
    window.addEventListener("resize", () => void syncMaximized());
  }

  // ---------- 标题栏 ----------
  const titlebarProps: Record<string, string> = { class: "titlebar" };
  const brandProps: Record<string, string> = { class: "brand" };
  const spacerProps: Record<string, string> = { class: "titlebar-spacer" };
  if (isDesktop) {
    titlebarProps["data-tauri-drag-region"] = "";
    brandProps["data-tauri-drag-region"] = "";
    spacerProps["data-tauri-drag-region"] = "";
  }
  const titlebar = el(
    "header",
    titlebarProps,
    el(
      "div",
      brandProps,
      el("div", { class: "brand-logo", text: "LP" }),
      el(
        "div",
        { class: "brand-text" },
        el("div", { class: "brand-name", text: t("app.name") }),
        el("div", { class: "brand-sub", text: t("app.subtitle") }),
      ),
    ),
    el("div", spacerProps),
    identity,
    infoBtn,
    infoPop,
    winControls,
  );

  if (win) {
    const desktopWin = win;
    // 双击标题栏空白处 = 最大化/还原
    titlebar.addEventListener("dblclick", (event) => {
      if ((event.target as HTMLElement).closest(".win-controls")) return;
      void desktopWin.toggleMaximize();
    });
  }

  // ---------- 导航 ----------
  const nav = el("nav", { class: "nav", "aria-label": "primary" });
  let indicatorReady = false;
  let activeRoute: Route = "send";
  const navIndicator = el("span", { class: "nav-indicator", "aria-hidden": "true" });
  nav.append(navIndicator);

  const placeIndicator = (animate: boolean) => {
    if (isAndroid) return;
    const active = navButtons.get(activeRoute);
    if (!active) return;
    if (!animate) navIndicator.classList.add("no-anim");
    const horizontal = window.matchMedia("(max-width: 640px)").matches;
    if (horizontal) {
      const offset = active.offsetLeft + active.offsetWidth / 2 - 12;
      navIndicator.style.transform = `translateX(${offset}px)`;
    } else {
      const offset = active.offsetTop + active.offsetHeight / 2 - 8;
      navIndicator.style.transform = `translateY(${offset}px)`;
    }
    navIndicator.classList.add("visible");
    if (!animate) {
      requestAnimationFrame(() => navIndicator.classList.remove("no-anim"));
    }
  };

  if (!isAndroid) window.addEventListener("resize", () => placeIndicator(false));

  for (const item of NAV) {
    const icon = isAndroid
      ? el("span", { class: "nav-ico" }, svgIcon(icons[item.icon], 24))
      : svgIcon(icons[item.icon], 20);
    const button = el(
      "button",
      {
        class: "nav-item",
        type: "button",
        onclick: () => onNavigate(item.route),
      },
      icon,
      el("span", { text: t(item.label) }),
    );
    navButtons.set(item.route, button);
    nav.append(button);
  }

  // ---------- 内容区 + 状态栏（Android 无状态栏） ----------
  const content = el("main", { class: "content" });
  let statusText: HTMLElement | null = null;
  let statusDot: HTMLElement | null = null;
  let statusbar: HTMLElement | null = null;
  if (isDesktop) {
    statusText = el("span", { id: "status-text", text: t("status.ready") });
    statusDot = el("span", { class: "statusdot" });
    const version = el("span", { class: "muted", id: "app-version", text: "" });
    statusbar = el(
      "footer",
      { class: "statusbar" },
      statusDot,
      statusText,
      el("span", { class: "statusbar-spacer" }),
      version,
    );
  }

  // ---------- Android 浮层 Toast（没有状态栏可显示提示） ----------
  let toastNode: HTMLElement | null = null;
  let toastTimer = 0;
  const showAndroidToast = (text: string) => {
    if (!toastNode) {
      toastNode = el("div", { class: "toast", role: "status", "aria-live": "polite" });
      document.body.append(toastNode);
    }
    toastNode.textContent = text;
    toastNode.classList.add("visible");
    window.clearTimeout(toastTimer);
    toastTimer = window.setTimeout(() => toastNode?.classList.remove("visible"), 2600);
  };

  clear(root);
  root.className = "app-root";
  root.append(titlebar, nav, content);
  if (statusbar) root.append(statusbar);

  return {
    content,
    setActive(route: Route) {
      activeRoute = route;
      navButtons.forEach((button, key) => {
        button.setAttribute("aria-current", key === route ? "page" : "false");
      });
      // Android 用 .nav-ico 胶囊底色表示选中，不需要滑动指示器
      if (isAndroid) return;
      requestAnimationFrame(() => {
        if (!indicatorReady) {
          placeIndicator(false);
          indicatorReady = true;
          return;
        }
        placeIndicator(true);
      });
    },
    setIdentity(name: string, shortId: string) {
      identity.textContent = t("identity.local", { name: `${name} · #${shortId}` });
      identity.title = `${name} · #${shortId}`;
    },
    setStatus(text: string, kind: "ok" | "busy" | "error") {
      if (!statusText || !statusDot) return;
      statusText.textContent = text;
      statusDot.className = `statusdot${kind === "busy" ? " busy" : kind === "error" ? " error" : ""}`;
    },
    showToast(text: string) {
      if (!isDesktop) {
        showAndroidToast(text);
        return;
      }
      if (!statusText || !statusDot) return;
      statusText.textContent = text;
      statusDot.className = "statusdot busy";
      window.setTimeout(() => {
        if (!statusText || !statusDot) return;
        statusText.textContent = t("status.ready");
        statusDot.className = "statusdot";
      }, 2600);
    },
  };
}

export function applyVersion(el: HTMLElement, version: string): void {
  el.textContent = t("app.version", { version });
}
