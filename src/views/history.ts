import { getState, subscribe } from "../state/store";
import { clear, el, icons, svgIcon } from "../ui/dom";
import { t } from "../i18n";
import { fmtBytes } from "../ui/format";

export function mountHistory(container: HTMLElement): () => void {
  const render = () => {
    clear(container);
    container.append(build());
  };
  const unsub = subscribe(render);
  render();
  return unsub;
}

function build(): HTMLElement {
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
        ...state.history.map((entry) =>
          el(
            "div",
            { class: "list-row history-row" },
            el("span", { class: `dot ${entry.ok ? "" : "error"}` }),
            el(
              "div",
              { class: "grow" },
              el("div", { class: "name", text: entry.name || t("common.unknown") }),
              el("div", {
                class: "meta",
                text: `${t(entry.direction === "send" ? "history.send" : "history.receive")} · ${t(
                  "history.meta",
                  { files: String(entry.files), size: fmtBytes(entry.bytes) },
                )}`,
              }),
            ),
            el("span", {
              class: "muted",
              text: new Date(entry.time).toLocaleString(),
            }),
            el("span", {
              class: "state-text",
              text: t(entry.ok ? "history.done" : "history.interrupted"),
            }),
          ),
        ),
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
