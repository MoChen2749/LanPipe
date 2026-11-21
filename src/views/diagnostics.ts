import { el } from "../ui/dom";
import { t } from "../i18n";
import { api } from "../api/tauri";
import { toast } from "../actions";

interface InterfaceRow {
  name: string;
  ip: string;
  netmask: string;
  prefix: number;
  shortId: string;
}

interface Diagnostics {
  version: string;
  osBuild: number;
  builtAt?: number;
  backdrop: string;
  configPath?: string | null;
  stateDir: string;
  journalCount: number;
  listen: { udp?: number; tcp?: number };
  discovery: boolean;
  streams: number;
  downloadDir: string;
  shortId?: string;
  deviceId: string;
  interfaces: InterfaceRow[];
  peers: { id: string; name: string; addr: string; shortId: string }[];
  firewall: boolean;
}

const row = (label: string, value: string) =>
  el("div", { class: "diag-row" }, el("span", { class: "diag-label", text: label }), el("span", { class: "diag-value", text: value }));

const band = (title: string, ...children: (Node | string)[]) =>
  el("section", { class: "card diagnostics-card" }, el("div", { class: "card-head" }, el("span", { class: "card-title", text: title })), ...children);

export function mountDiagnostics(container: HTMLElement): () => void {
  let disposed = false;

  const build = async () => {
    let data: Diagnostics | null = null;
    let error = "";
    try {
      data = await api.diagnostics();
    } catch (err) {
      error = String(err);
    }
    if (disposed) return;
    container.replaceChildren();

    const head = el(
      "div",
      { class: "page-head" },
      el("h1", { class: "page-title", text: t("diagnostics.title") }),
      el("div", { class: "page-actions" }, el("button", { class: "btn ghost small", type: "button", onclick: refresh, title: t("diagnostics.refresh") }, t("diagnostics.refresh"))),
    );

    if (error || !data) {
      container.append(
        el("div", { class: "diagnostics-view" }, head, band(t("diagnostics.error"), row(t("diagnostics.errorHint"), error))),
      );
      return;
    }

    const view = el("div", { class: "diagnostics-view" }, head);

    view.append(
      band(
        t("diagnostics.overview"),
        row(t("diagnostics.version"), data.version),
        row(t("diagnostics.osBuild"), String(data.osBuild)),
    row(t("diagnostics.builtAt"), data.builtAt ? new Date(data.builtAt * 1000).toLocaleString() : "-"),
        row(t("diagnostics.backdrop"), data.backdrop || "none"),
        row(t("diagnostics.shortId"), `#${data.shortId ?? "000"}`),
        row(t("diagnostics.deviceId"), data.deviceId),
        row(t("diagnostics.config"), data.configPath ?? "-"),
        row(t("diagnostics.stateDir"), `${data.stateDir}（${data.journalCount} ${t("diagnostics.journals")}）`),
      ),
      band(
        t("diagnostics.network"),
        ...(data.interfaces.length
          ? data.interfaces.map((iface) =>
              el(
                "div",
                { class: "diag-row" },
                el("span", { class: "diag-label", text: iface.name }),
                el("span", { class: "diag-value", text: `${iface.ip}/${iface.prefix} · ${iface.netmask} · #${iface.shortId}` }),
              ),
            )
          : [row(t("diagnostics.network"), t("diagnostics.noInterface"))]),
        row(t("diagnostics.ports"), `UDP ${data.listen.udp ?? "-"} / TCP ${data.listen.tcp ?? "-"}`),
        row(t("diagnostics.discovery"), data.discovery ? t("diagnostics.on") : t("diagnostics.off")),
        row(t("diagnostics.streams"), String(data.streams)),
        row(t("diagnostics.firewall"), data.firewall ? t("settings.firewallOk") : t("settings.firewallNone")),
        row(t("diagnostics.downloadDir"), data.downloadDir),
      ),
      band(
        t("diagnostics.peers"),
        ...(data.peers.length
          ? data.peers.map((peer) =>
              el(
                "div",
                { class: "diag-row" },
                el("span", { class: "diag-label", text: peer.name }),
                el("span", { class: "diag-value", text: `#${peer.shortId} · ${peer.addr}` }),
              ),
            )
          : [row(t("diagnostics.peers"), t("diagnostics.noPeers"))]),
      ),
    );

    const copy = el(
      "button",
      {
        class: "btn primary",
        type: "button",
        onclick: () => {
          void navigator.clipboard
            .writeText(JSON.stringify(data, null, 2))
            .then(() => toast(t("diagnostics.copied")))
            .catch((err) => toast(String(err)));
        },
      },
      t("diagnostics.copy"),
    );
    view.append(el("div", { class: "page-actions" }, copy));
    container.append(view);
  };

  const refresh = () => {
    void build();
  };

  refresh();
  return () => {
    disposed = true;
  };
}
