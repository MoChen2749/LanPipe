import { clear, el, icons, svgIcon } from "../ui/dom";
import { t } from "../i18n";
import { api } from "../api/tauri";
import { toast } from "../actions";

export type Route = "send" | "receive" | "history" | "settings";

const NAV: { route: Route; icon: keyof typeof icons; label: string }[] = [
  { route: "send", icon: "send", label: "nav.send" },
  { route: "receive", icon: "receive", label: "nav.receive" },
  { route: "history", icon: "history", label: "nav.history" },
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

  const identity = el("div", { class: "identity", id: "identity-chip", title: "" });
  const infoPop = el("div", { class: "info-pop hidden", id: "info-pop" });
  const renderInfo = async () => {
    infoPop.replaceChildren(el("div", { class: "info-title", text: t("info.title") }));
    try {
      const info = await api.localInfo();
      const list = info.ipv4 && info.ipv4.length > 0 ? info.ipv4 : [];
      if (list.length === 0) {
        infoPop.append(el("div", { class: "muted", text: t("info.empty") }));
        return;
      }
      for (const ip of list) {
        infoPop.append(
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
      infoPop.append(el("div", { class: "muted info-hint", text: t("info.hint") }));
    } catch (err) {
      infoPop.append(el("div", { class: "muted", text: String(err) }));
    }
  };
  const infoBtn = el(
    "button",
    {
      class: "icon-btn",
      id: "info-btn",
      title: t("info.title"),
      "aria-label": t("info.title"),
      onclick: (event: MouseEvent) => {
        event.stopPropagation();
        const hidden = infoPop.classList.toggle("hidden");
        if (!hidden) void renderInfo();
      },
    },
    svgIcon(icons.info, 16),
  );
  document.addEventListener("click", (event) => {
    const target = event.target as Node | null;
    if (!target) return;
    if (!infoPop.contains(target) && target !== infoBtn && !infoBtn.contains(target)) {
      infoPop.classList.add("hidden");
    }
  });

  const titlebar = el(
    "header",
    { class: "titlebar", "data-tauri-drag-region": "" },
    el(
      "div",
      { class: "brand", "data-tauri-drag-region": "" },
      el("div", { class: "brand-logo", text: "LP" }),
      el(
        "div",
        { class: "brand-text" },
        el("div", { class: "brand-name", text: t("app.name") }),
        el("div", { class: "brand-sub", text: t("app.subtitle") }),
      ),
    ),
    el("div", { class: "titlebar-spacer", "data-tauri-drag-region": "" }),
    identity,
    infoBtn,
    infoPop,
  );

  const nav = el("nav", { class: "nav", "aria-label": "primary" });
  const navIndicator = el("span", { class: "nav-indicator", "aria-hidden": "true" });
  nav.append(navIndicator);
  for (const item of NAV) {
    const button = el(
      "button",
      {
        class: "nav-item",
        type: "button",
        onclick: () => onNavigate(item.route),
      },
      svgIcon(icons[item.icon], 20),
      el("span", { text: t(item.label) }),
    );
    navButtons.set(item.route, button);
    nav.append(button);
  }

  const content = el("main", { class: "content" });
  const statusText = el("span", { id: "status-text", text: t("status.ready") });
  const statusDot = el("span", { class: "statusdot" });
  const version = el("span", { class: "muted", id: "app-version", text: "" });
  const statusbar = el(
    "footer",
    { class: "statusbar" },
    statusDot,
    statusText,
    el("span", { class: "statusbar-spacer" }),
    version,
  );

  clear(root);
  root.className = "app-root";
  root.append(titlebar, nav, content, statusbar);

  return {
    content,
    setActive(route: Route) {
      navButtons.forEach((button, key) => {
        button.setAttribute("aria-current", key === route ? "page" : "false");
      });
      const active = navButtons.get(route);
      if (!active) return;
      requestAnimationFrame(() => {
        const offset = active.offsetTop + active.offsetHeight / 2 - 8;
        navIndicator.style.transform = `translateY(${offset}px)`;
        navIndicator.classList.toggle("visible", true);
      });
    },
    setIdentity(name: string, shortId: string) {
      identity.textContent = t("identity.local", { name: `${name} · #${shortId}` });
      identity.title = `${name} · #${shortId}`;
    },
    setStatus(text: string, kind: "ok" | "busy" | "error") {
      statusText.textContent = text;
      statusDot.className = `statusdot${kind === "busy" ? " busy" : kind === "error" ? " error" : ""}`;
    },
    showToast(text: string) {
      statusText.textContent = text;
      statusDot.className = "statusdot busy";
      window.setTimeout(() => {
        statusText.textContent = t("status.ready");
        statusDot.className = "statusdot";
      }, 2600);
    },
  };
}

export function applyVersion(el: HTMLElement, version: string): void {
  el.textContent = t("app.version", { version });
}
