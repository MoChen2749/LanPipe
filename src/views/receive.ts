import { getState, subscribe } from "../state/store";
import { clear, el, icons, svgIcon, withFocusPreserved } from "../ui/dom";
import { t } from "../i18n";
import { fmtBytes, fmtSpeed, fmtTime } from "../ui/format";
import { chooseDownloadDir, revealPath, saveSettings } from "../actions";
import type { Route } from "../layout/shell";

export function mountReceive(container: HTMLElement, navigate: (route: Route) => void): () => void {
  const render = () =>
    withFocusPreserved(() => {
      clear(container);
      container.append(build(navigate));
    });
  const unsub = subscribe(render);
  render();
  return unsub;
}

function build(navigate: (route: Route) => void): HTMLElement {
  const state = getState();
  const local = state.local;
  const settings = state.settings;
  const transfer = state.transfer && state.transfer.direction === "receive" ? state.transfer : null;

  const head = el(
    "div",
    { class: "page-head" },
    el("h1", { class: "page-title", text: t("receive.title") }),
    el("p", {
      class: "page-sub",
      text: t("receive.subtitle", {
        name: local?.name ?? settings?.displayName ?? "",
        port: String(local?.tcpPort ?? settings?.listen.tcp ?? 53311),
      }),
    }),
  );

  const banner =
    transfer && transfer.state === "failed" && transfer.filesTotal > 0
      ? el(
          "section",
          { class: "banner" },
          svgIcon(icons.alert, 20),
          el(
            "div",
            { class: "grow" },
            el("div", { class: "banner-title", text: t("receive.incomplete.title") }),
            el("div", {
              class: "banner-desc",
              text: t("receive.incomplete.desc", {
                name: transfer.peerName || t("common.unknown"),
                done: String(transfer.filesDone),
                total: String(transfer.filesTotal),
                percent: String(percentOf(transfer.filesDone, transfer.filesTotal)),
              }),
            }),
            el("div", { class: "banner-hint", text: t("receive.incomplete.hint") }),
          ),
        )
      : null;

  const current = transfer ? transferCard(transfer) : idleCard();

  const dir = settings?.downloadDir ?? "";
  const saveCard = el(
    "section",
    { class: "card save-row" },
    el(
      "div",
      { class: "grow" },
      el("div", { class: "card-title", text: t("receive.saved.title") }),
      el("div", { class: "path", title: dir, text: dir }),
    ),
    el(
      "button",
      {
        class: "btn ghost small",
        type: "button",
        onclick: async () => {
          const picked = await chooseDownloadDir(dir);
          if (picked) {
            await saveSettings({
              displayName: settings?.displayName ?? "",
              downloadDir: picked,
              udp: settings?.listen.udp ?? 53310,
              tcp: settings?.listen.tcp ?? 53311,
              streams: settings?.transfer.streams ?? 4,
              compress: settings?.transfer.compress ?? "off",
              discovery: settings?.network.discovery ?? true,
              theme: settings?.theme ?? "system",
              language: settings?.language ?? "zh-CN",
            });
          }
        },
      },
      t("receive.change"),
    ),
    el(
      "button",
      {
        class: "btn ghost small",
        type: "button",
        onclick: () => void revealPath(state.transfer?.rootName ? dir : dir),
      },
      t("receive.open"),
    ),
  );

  const receives = state.history.filter((h) => h.direction === "receive").slice(0, 5);
  const recent = el(
    "section",
    { class: "card" },
    el(
      "div",
      { class: "card-head" },
      el("span", { class: "card-title", text: t("receive.recent.title") }),
      el("span", { class: "spacer" }),
      el(
        "button",
        { class: "btn ghost small", type: "button", onclick: () => navigate("history") },
        t("receive.recent.all"),
      ),
    ),
    receives.length
      ? el("div", { class: "list" }, ...receives.map((entry) => historyRow(entry)))
      : el("div", { class: "empty", text: t("history.empty") }),
  );

  return el("div", { class: "receive-view" }, head, banner, current, saveCard, recent);
}

function percentOf(done: number, total: number): number {
  if (total <= 0) return 0;
  return Math.min(100, Math.round((done / total) * 100));
}

function transferCard(tr: NonNullable<ReturnType<typeof getState>["transfer"]>): HTMLElement {
  const percent =
    tr.filesTotal > 0 ? percentOf(tr.filesDone, tr.filesTotal) : percentOf(tr.bytesDone, tr.bytesTotal);
  const title =
    tr.state === "done"
      ? t("status.done")
      : tr.state === "failed"
        ? t("status.failed")
        : t("status.receiving");
  return el(
    "section",
    { class: "card" },
    el(
      "div",
      { class: "card-head" },
      el("span", { class: "card-title", text: title }),
      el("span", { class: "spacer" }),
      el("span", {
        class: "muted",
        text: t("receive.current.from", { name: tr.peerName || t("common.unknown") }),
      }),
    ),
    el(
      "div",
      { class: "progress" },
      el("div", { class: "bar", style: `width:${percent}%` }),
    ),
    el(
      "div",
      { class: "progress-meta" },
      el("span", { text: `${percent}%` }),
      el("span", {
        text: t("receive.current.files", {
          done: String(tr.filesDone),
          total: String(tr.filesTotal),
        }),
      }),
      el("span", { text: t("receive.current.bytes", { done: fmtBytes(tr.bytesDone), total: fmtBytes(tr.bytesTotal) }) }),
      tr.state === "active"
        ? el("span", { text: t("receive.current.speed", { speed: fmtSpeed(tr.speed), eta: fmtTime(0) }) })
        : null,
    ),
    tr.message ? el("p", { class: "hint error", text: tr.message }) : null,
  );
}

function idleCard(): HTMLElement {
  return el(
    "section",
    { class: "card idle-card" },
    el("div", { class: "title", text: t("receive.idle") }),
    el("div", { class: "hint", text: t("receive.idleHint") }),
  );
}

function historyRow(entry: {
  name: string;
  files: number;
  bytes: number;
  ok: boolean;
  time: number;
}): HTMLElement {
  return el(
    "div",
    { class: "list-row" },
    el("span", { class: `dot ${entry.ok ? "" : "error"}` }),
    el(
      "div",
      { class: "grow" },
      el("div", { class: "name", text: entry.name || t("common.unknown") }),
      el("div", {
        class: "meta",
        text: t("history.meta", { files: String(entry.files), size: fmtBytes(entry.bytes) }),
      }),
    ),
    el("span", { class: "muted", text: new Date(entry.time).toLocaleTimeString() }),
  );
}
