import { getState } from "../state/store";
import { el } from "../ui/dom";
import { dropdown } from "../ui/dropdown";
import { stepper } from "../ui/stepper";
import { t } from "../i18n";
import { chooseDownloadDir, resetDeviceId, saveSettings, toast } from "../actions";
import { api } from "../api/tauri";
import { getVersion } from "@tauri-apps/api/app";

let version = "";

export function mountSettings(container: HTMLElement): () => void {
  container.replaceChildren(build());
  const firewallState = container.querySelector<HTMLElement>("#firewall-state");
  const firewallButton = container.querySelector<HTMLButtonElement>("#firewall-btn");
  const setFirewallState = (allowed: boolean | null) => {
    if (!firewallState) return;
    firewallState.textContent =
      allowed === null
        ? t("settings.firewallUnknown")
        : allowed
          ? t("settings.firewallOk")
          : t("settings.firewallNone");
    firewallState.classList.toggle("ok", allowed === true);
  };
  const refreshFirewall = async () => {
    try {
      setFirewallState(await api.firewallStatus());
    } catch {
      setFirewallState(null);
    }
  };
  void refreshFirewall();
  firewallButton?.addEventListener("click", async () => {
    firewallButton.disabled = true;
    try {
      const allowed = await api.firewallAllow();
      setFirewallState(allowed);
      toast(allowed ? t("toast.firewallOk") : t("toast.firewallFail"));
    } catch {
      toast(t("toast.firewallFail"));
      void refreshFirewall();
    } finally {
      firewallButton.disabled = false;
    }
  });
  void getVersion()
    .then((value) => {
      version = value;
      const node = document.getElementById("about-version");
      if (node) node.textContent = t("settings.about.version", { version: value });
      const upd = document.getElementById("update-state");
      if (upd && !upd.textContent) upd.textContent = t("settings.updateIdle", { version: value });
    })
    .catch(() => undefined);
  return () => undefined;
}

function field(label: string, control: HTMLElement, hint?: string): HTMLElement {
  return el(
    "div",
    { class: "field" },
    el("span", { class: "field-label", text: label }),
    control,
    hint ? el("small", { class: "muted", text: hint }) : null,
  );
}

function build(): HTMLElement {
  const state = getState();
  const settings = state.settings;
  const shortId = state.local?.shortId ?? settings?.shortId ?? "000";

  let displayName = settings?.displayName ?? "";
  let downloadDir = settings?.downloadDir ?? "";
  let udp = settings?.listen.udp ?? 53310;
  let tcp = settings?.listen.tcp ?? 53311;
  let streams = settings?.transfer.streams ?? 4;
  let compress: "off" | "auto" = settings?.transfer.compress === "auto" ? "auto" : "off";
  let discovery = settings?.network.discovery !== false;
  let theme = settings?.theme ?? "system";
  let language = settings?.language ?? "zh-CN";

  const nameInput = el("input", {
    class: "input",
    id: "set-name",
    value: displayName,
    maxlength: "64",
    oninput: (event: Event) => {
      displayName = (event.target as HTMLInputElement).value;
    },
  }) as HTMLInputElement;

  const dirInput = el("input", {
    class: "input",
    id: "set-dir",
    value: downloadDir,
    readonly: true,
  }) as HTMLInputElement;

  const idInput = el("input", {
    class: "input",
    id: "set-id",
    value: settings?.deviceId ?? "",
    readonly: true,
  }) as HTMLInputElement;

  const streamsValue = el("b", { class: "range-value", text: String(streams) });
  const streamsInput = el("input", {
    class: "range",
    id: "set-streams",
    type: "range",
    min: "1",
    max: "8",
    step: "1",
    value: String(streams),
    oninput: (event: Event) => {
      streams = Number((event.target as HTMLInputElement).value);
      streamsValue.textContent = String(streams);
    },
  }) as HTMLInputElement;

  const compressControl = dropdown({
    id: "set-compress",
    value: compress,
    options: [
      { value: "off", label: t("settings.compressOff") },
      { value: "auto", label: t("settings.compressAuto") },
    ],
    onChange: (value) => {
      compress = value === "auto" ? "auto" : "off";
    },
  });

  const themeControl = dropdown({
    id: "set-theme",
    value: theme,
    options: [
      { value: "system", label: t("settings.themeSystem") },
      { value: "light", label: t("settings.themeLight") },
      { value: "dark", label: t("settings.themeDark") },
    ],
    onChange: (value) => {
      theme = value;
    },
  });

  const languageControl = dropdown({
    id: "set-language",
    value: language,
    options: [{ value: "zh-CN", label: "中文（简体）" }],
    onChange: (value) => {
      language = value;
    },
  });

  const discoveryInput = el("input", {
    type: "checkbox",
    id: "set-discovery",
    checked: discovery ? true : undefined,
    onchange: (event: Event) => {
      discovery = (event.target as HTMLInputElement).checked;
    },
  }) as HTMLInputElement;

  const saveButton = el(
    "button",
    {
      class: "btn primary",
      type: "button",
      id: "set-save",
      onclick: () =>
        void saveSettings({
          displayName: nameInput.value.trim(),
          downloadDir,
          udp,
          tcp,
          streams,
          compress,
          discovery,
          theme,
          language,
        }),
    },
    t("settings.save"),
  );

  const deviceCard = el(
    "section",
    { class: "card settings-card" },
    el("div", { class: "card-head" }, el("span", { class: "card-title", text: t("settings.group.device") })),
    field(t("settings.name"), nameInput, t("settings.nameHint")),
    field(
      t("settings.dir"),
      el(
        "div",
        { class: "row" },
        dirInput,
        el(
          "button",
          {
            class: "btn",
            type: "button",
            onclick: async () => {
              const picked = await chooseDownloadDir(dirInput.value);
              if (picked) {
                downloadDir = picked;
                dirInput.value = picked;
              }
            },
          },
          t("settings.dirPick"),
        ),
      ),
    ),
    field(
      t("settings.id"),
      el(
        "div",
        { class: "row" },
        idInput,
        el(
          "button",
          {
            class: "btn ghost",
            type: "button",
            onclick: async () => {
              if (!window.confirm(t("settings.resetHint"))) return;
              await resetDeviceId();
              idInput.value = getState().settings?.deviceId ?? "";
            },
          },
          t("settings.reset"),
        ),
      ),
      t("settings.shortId", { id: shortId }),
    ),
  );

  const transferCard = el(
    "section",
    { class: "card settings-card" },
    el("div", { class: "card-head" }, el("span", { class: "card-title", text: t("settings.group.transfer") })),
    el(
      "div",
      { class: "field" },
      el("span", { class: "field-label", text: t("settings.streams") }),
      el("div", { class: "range-row" }, streamsInput, streamsValue),
      el("small", { class: "muted", text: t("settings.streamsHint", { n: String(streams) }) }),
    ),
    field(t("settings.compress"), compressControl),
  );

  const networkCard = el(
    "section",
    { class: "card settings-card" },
    el("div", { class: "card-head" }, el("span", { class: "card-title", text: t("settings.group.network") })),
    el(
      "div",
      { class: "grid-2" },
      field(t("settings.udp"), stepper({ id: "set-udp", value: udp, min: 1024, max: 65535, onChange: (v) => (udp = v) })),
      field(t("settings.tcp"), stepper({ id: "set-tcp", value: tcp, min: 1024, max: 65535, onChange: (v) => (tcp = v) })),
    ),
    el("p", { class: "field-hint", text: t("settings.portsHint") }),
    el(
      "label",
      { class: "switch" },
      discoveryInput,
      el("span", { class: "track" }),
      el(
        "span",
        { class: "switch-text" },
        el("span", { text: t("settings.discovery") }),
        el("small", { class: "muted", text: t("settings.discoveryHint") }),
      ),
    ),
    el(
      "div",
      { class: "card-actions" },
      el(
        "span",
        { class: "muted", id: "firewall-state", text: t("settings.firewallUnknown") },
      ),
      el(
        "button",
        {
          class: "btn ghost small",
          type: "button",
          id: "firewall-btn",
          title: t("settings.firewallHint"),
        },
        t("settings.firewall"),
      ),
    ),
  );

  const autostartInput = el("input", { type: "checkbox", id: "set-autostart" }) as HTMLInputElement;
  void api.autostartEnabled().then((on) => {
    autostartInput.checked = on;
  });
  autostartInput.addEventListener("change", async () => {
    try {
      await api.autostartSet(autostartInput.checked);
      toast(autostartInput.checked ? t("toast.autostartOn") : t("toast.autostartOff"));
    } catch (err) {
      autostartInput.checked = !autostartInput.checked;
      toast(String(err));
    }
  });

  const startupCard = el(
    "section",
    { class: "card settings-card" },
    el("div", { class: "card-head" }, el("span", { class: "card-title", text: t("settings.group.startup") })),
    el(
      "label",
      { class: "switch" },
      autostartInput,
      el("span", { class: "track" }),
      el(
        "span",
        { class: "switch-text" },
        el("span", { text: t("settings.autostart") }),
        el("small", { class: "muted", text: t("settings.autostartHint") }),
      ),
    ),
  );

  const appearanceCard = el(
    "section",
    { class: "card settings-card" },
    el("div", { class: "card-head" }, el("span", { class: "card-title", text: t("settings.group.appearance") })),
    el("div", { class: "grid-2" }, field(t("settings.theme"), themeControl), field(t("settings.language"), languageControl)),
  );

  const aboutCard = el(
    "section",
    { class: "card settings-card" },
    el("div", { class: "card-head" }, el("span", { class: "card-title", text: t("settings.group.about") })),
    el(
      "div",
      { class: "about-lines" },
      el("div", { id: "about-version", text: version ? t("settings.about.version", { version }) : "" }),
      el("div", { text: t("settings.about.license") }),
      el("div", { text: t("settings.about.docs") }),
    ),
  );

  const updateBtn = el("button", {
    class: "btn small",
    id: "update-btn",
    text: t("settings.checkUpdate"),
  });
  const updateState = el("span", { class: "muted", id: "update-state" });
  updateBtn.addEventListener("click", async () => {
    updateBtn.setAttribute("disabled", "true");
    updateState.textContent = t("settings.updateChecking");
    try {
      const result = await api.checkUpdate();
      updateState.textContent = result
        ? t("settings.updateFound", { version: result.version })
        : t("settings.updateLatest");
    } catch (err) {
      updateState.textContent = t("settings.updateFailed");
      void err;
      toast(t("toast.updateFailed"));
    } finally {
      updateBtn.removeAttribute("disabled");
    }
  });
  aboutCard.appendChild(el("div", { class: "row", style: "margin-top:12px" }, updateBtn, updateState));

  return el(
    "div",
    { class: "settings-view" },
    el("div", { class: "page-head" }, el("h1", { class: "page-title", text: t("settings.title") })),
    el("div", { class: "settings-grid" }, deviceCard, transferCard, networkCard, startupCard, appearanceCard, aboutCard),
    el("div", { class: "settings-actions" }, saveButton),
  );
}
