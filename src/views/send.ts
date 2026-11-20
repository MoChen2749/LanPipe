import { getState, subscribe } from "../state/store";
import { clear, el, icons, svgIcon, withFocusPreserved } from "../ui/dom";
import { t } from "../i18n";
import { baseName, fmtBytes, fmtCount } from "../ui/format";
import {
  clearStaged,
  pickFiles,
  pickFolder,
  refreshPeers,
  removeStaged,
  scanOctet,
  sendTo,
  sendToAddr,
  toast,
} from "../actions";

let ipHint = "";
const IPV4 = /^(\d{1,3}\.){3}\d{1,3}(:\d{2,5})?$/;

export function mountSend(container: HTMLElement): () => void {
  const render = () =>
    withFocusPreserved(() => {
      clear(container);
      container.append(build(render));
    });
  const unsub = subscribe(render);
  render();
  return unsub;
}

function build(rerender: () => void): HTMLElement {
  const state = getState();
  const staged = state.staged;
  const summary = state.stagedSummary;
  const hasStaged = staged.length > 0;

  const head = el(
    "div",
    { class: "page-head" },
    el("h1", { class: "page-title", text: t("send.title") }),
    el("p", { class: "page-sub", text: t("send.subtitle") }),
  );

  const dropzone = el(
    "div",
    { class: "dropzone", id: "dropzone" },
    el("div", { class: "icon" }, svgIcon(icons.up, 32)),
    el("div", { class: "title", text: t("send.drop.title") }),
    el("div", { class: "hint", text: t("send.drop.hint") }),
    el(
      "div",
      { class: "actions" },
      el(
        "button",
        { class: "btn primary", type: "button", onclick: () => void pickFiles() },
        t("send.pickFiles"),
      ),
      el(
        "button",
        { class: "btn", type: "button", onclick: () => void pickFolder() },
        t("send.pickFolder"),
      ),
    ),
  );

  const stagedCard = el(
    "section",
    { class: "card staged-card" },
    el(
      "div",
      { class: "card-head" },
      el("span", {
        class: "card-title",
        text: hasStaged
          ? t("send.staged.title", {
              count: staged.length,
              size: fmtBytes(summary?.bytes ?? 0),
            })
          : t("send.empty"),
      }),
      el("span", { class: "spacer" }),
      hasStaged
        ? el(
            "button",
            { class: "btn ghost small", type: "button", onclick: () => clearStaged() },
            t("send.clear"),
          )
        : null,
    ),
    hasStaged
      ? el(
          "div",
          { class: "list" },
          ...staged.map((path) =>
            el(
              "div",
              { class: "list-row" },
              svgIcon(icons.folder, 18),
              el(
                "div",
                { class: "grow" },
                el("div", { class: "name", text: baseName(path) }),
                el("div", {
                  class: "meta",
                  title: path,
                  text: summary
                    ? `${fmtCount(summary.files, "文件")} · ${fmtBytes(summary.bytes)}`
                    : path,
                }),
              ),
              el(
                "button",
                {
                  class: "btn ghost small",
                  type: "button",
                  onclick: () => removeStaged(path),
                },
                t("send.remove"),
              ),
            ),
          ),
        )
      : el("div", { class: "empty", text: t("send.staged.empty") }),
    el(
      "div",
      { class: "primary-row" },
      el(
        "button",
        {
          class: "btn primary",
          type: "button",
          disabled: !hasStaged,
          onclick: () => {
            const peers = getState().peers;
            if (peers.length === 1) void sendTo(peers[0]);
            else {
              const panel = document.getElementById("devices-card");
              panel?.scrollIntoView({ behavior: "smooth", block: "center" });
            }
          },
        },
        t("send.primary"),
      ),
      el("span", { class: "hint", text: t("send.primaryHint") }),
    ),
  );

  const deviceCard = el(
    "section",
    { class: "card card-devices", id: "devices-card" },
    el(
      "div",
      { class: "card-head" },
      el("span", { class: "card-title", text: t("devices.title") }),
      el("span", { class: "spacer" }),
      el(
        "button",
        { class: "btn ghost small", type: "button", onclick: () => void refreshPeers() },
        svgIcon(icons.refresh, 14),
        t("devices.refresh"),
      ),
    ),
    state.peers.length > 0
      ? el("div", { class: "device-list" }, ...state.peers.map((peer) => deviceRow(peer)))
      : el(
          "div",
          { class: "empty" },
          el("div", { text: t("devices.empty") }),
          el("div", { class: "meta", text: t("devices.emptyHint") }),
        ),
  );

  const input = el("input", {
    class: "input",
    id: "direct-input",
    placeholder: t("direct.placeholder"),
    autocomplete: "off",
    spellcheck: "false",
  }) as HTMLInputElement;

  const connect = async () => {
    const raw = input.value.trim();
    if (/^\d{1,3}$/.test(raw)) {
      const octet = Number(raw);
      if (octet > 255) {
        ipHint = t("direct.invalid");
      } else {
        ipHint = t("direct.scanning", { target: `*.${String(octet).padStart(3, "0")}` });
        const found = await scanOctet(octet);
        ipHint = found.length
          ? t("direct.found", { count: found.length })
          : t("direct.none");
      }
      rerender();
      return;
    }
    if (!IPV4.test(raw)) {
      ipHint = t("direct.invalid");
      rerender();
      return;
    }
    const tcp = getState().settings?.listen.tcp ?? 53311;
    const addr = raw.includes(":") ? raw : `${raw}:${tcp}`;
    ipHint = addr;
    await sendToAddr(addr);
    rerender();
  };
  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter") void connect();
  });

  const directCard = el(
    "section",
    { class: "card card-direct" },
    el("div", { class: "card-head" }, el("span", { class: "card-title", text: t("direct.title") })),
    el(
      "div",
      { class: "direct-row" },
      input,
      el("button", { class: "btn", type: "button", onclick: () => void connect() }, t("direct.connect")),
    ),
    el("p", { class: "hint", text: ipHint || t("direct.hint") }),
  );

  const sideCol = el("div", { class: "device-panel" }, deviceCard, directCard);
  return el("div", { class: "send-grid" }, head, dropzone, stagedCard, sideCol);
}

function deviceRow(peer: { name: string; addr: string; shortId: string; id: string }): HTMLElement {
  const state = getState();
  const disabled = state.staged.length === 0;
  return el(
    "div",
    { class: "device", title: `完整 ID：${peer.id}` },
    el("div", { class: "icon" }, svgIcon(icons.device, 18)),
    el(
      "div",
      { class: "grow" },
      el("div", { class: "name", text: peer.name }),
      el("div", { class: "addr", text: `#${peer.shortId} · ${peer.addr}` }),
    ),
    el(
      "div",
      { class: "state" },
      el("span", { class: "dot" }),
      el("span", { text: t("devices.online") }),
    ),
    el(
      "button",
      {
        class: "send",
        type: "button",
        disabled,
        onclick: () => void sendTo(peer),
      },
      t("devices.send"),
    ),
  );
}

export function notifySendHint(text: string): void {
  ipHint = text;
  const node = document.getElementById("direct-hint");
  if (node) node.textContent = text;
  else toast(text);
}
