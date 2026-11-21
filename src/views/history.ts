import { getState, subscribe } from "../state/store";
import { clear, el, icons, svgIcon } from "../ui/dom";
import { t } from "../i18n";
import { fmtBytes } from "../ui/format";

export function mountHistory(container: HTMLElement): () => void {
  let expanded: string | null = null;
  const toggle = (id: string) => {
    expanded = expanded === id ? null : id;
    render();
  };
  const render = () => {
    clear(container);
    container.append(build(expanded, toggle));
  };
  const unsub = subscribe(render);
  render();
  return unsub;
}

function build(expanded: string | null, toggle: (id: string) => void): HTMLElement {
  const state = getState();
  const head = el(
    "div",
    { class: "page-head" },
    el("h1", { class: "page-title", text: t("history.title") }),
  );

  const list = state.history.length
    ? el(
        "div",
        { class: "list history-list" },
        ...state.history.map((entry) => row(entry, expanded === entry.id, toggle)),
      )
    : el(
        "div",
        { class: "empty empty-illustrated" },
        svgIcon(icons.history, 28),
        el("div", { class: "empty-title", text: t("history.empty") }),
        el("div", { class: "empty-hint", text: t("history.emptyHint") }),
      );

  return el("div", { class: "history-view" }, head, el("section", { class: "card" }, list));
}

function row(
  entry: ReturnType<typeof getState>["history"][number],
  open: boolean,
  toggle: (id: string) => void,
): HTMLElement {
  const direction = t(entry.direction === "send" ? "history.send" : "history.receive");
  const summary = el(
    "div",
    {
      class: "list-row history-row clickable",
      role: "button",
      tabindex: "0",
      "aria-expanded": open ? "true" : "false",
      onclick: () => toggle(entry.id),
      onkeydown: (event: KeyboardEvent) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          toggle(entry.id);
        }
      },
    },
    el("span", { class: `dot ${entry.ok ? "" : "error"}` }),
    el(
      "div",
      { class: "grow" },
      el("div", { class: "name", text: entry.name || t("common.unknown") }),
      el("div", {
        class: "meta",
        text: `${direction} · ${t("history.meta", { files: String(entry.files), size: fmtBytes(entry.bytes) })}`,
      }),
    ),
    el("span", { class: "muted", text: new Date(entry.time).toLocaleString() }),
    el("span", { class: "state-text", text: t(entry.ok ? "history.done" : "history.interrupted") }),
    el("span", { class: `chevron ${open ? "open" : ""}` }, svgIcon(icons.chevronDown, 14)),
  );

  if (!open) return summary;

  const detail = el(
    "div",
    { class: "history-detail" },
    detailItem(t("history.direction"), direction),
    detailItem(t("history.peer"), entry.peerName || t("common.unknown")),
    detailItem(
      t("history.size"),
      t("history.meta", { files: String(entry.files), size: fmtBytes(entry.bytes) }),
    ),
    detailItem(t("history.time"), new Date(entry.time).toLocaleString()),
    detailItem(t("history.state"), t(entry.ok ? "history.done" : "history.interrupted")),
    entry.message ? detailItem(t("history.reason"), entry.message) : "",
  );

  return el("div", { class: "history-item open" }, summary, detail);
}

const detailItem = (label: string, value: string) =>
  el(
    "div",
    { class: "history-detail-row" },
    el("span", { class: "muted", text: label }),
    el("span", { text: value }),
  );
