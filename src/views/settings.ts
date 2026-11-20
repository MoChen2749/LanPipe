import { getState } from "../state/store";
import { el } from "../ui/dom";
import { t } from "../i18n";
import { chooseDownloadDir, resetDeviceId, saveSettings } from "../actions";
import { getVersion } from "@tauri-apps/api/app";

let version = "";

export function mountSettings(container: HTMLElement): () => void {
  const root = build();
  container.replaceChildren(root);
  void getVersion()
    .then((v) => {
      version = v;
      const node = document.getElementById("about-version");
      if (node) node.textContent = t("settings.about.version", { version: v });
    })
    .catch(() => undefined);
  return () => undefined;
}

function field(label: string, control: HTMLElement, hint?: string): HTMLElement {
  return el(
    "label",
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

  const nameInput = el("input", {
    class: "input",
    id: "set-name",
    value: settings?.displayName ?? "",
    maxlength: "64",
  }) as HTMLInputElement;

  const dirInput = el("input", {
    class: "input",
    id: "set-dir",
    value: settings?.downloadDir ?? "",
    readonly: true,
  }) as HTMLInputElement;

  const dirRow = el(
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
          if (picked) dirInput.value = picked;
        },
      },
      t("settings.dirPick"),
    ),
  );

  const idInput = el("input", {
    class: "input",
    id: "set-id",
    value: settings?.deviceId ?? "",
    readonly: true,
  }) as HTMLInputElement;

  const idRow = el(
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
          const next = getState().settings?.deviceId ?? "";
          idInput.value = next;
        },
      },
      t("settings.reset"),
    ),
  );

  const udpInput = el("input", {
    class: "input",
    id: "set-udp",
    type: "number",
    min: "1024",
    max: "65535",
    value: String(settings?.listen.udp ?? 53310),
  }) as HTMLInputElement;

  const tcpInput = el("input", {
    class: "input",
    id: "set-tcp",
    type: "number",
    min: "1024",
    max: "65535",
    value: String(settings?.listen.tcp ?? 53311),
  }) as HTMLInputElement;

  const streamsValue = el("b", { text: String(settings?.transfer.streams ?? 4) });
  const streamsInput = el("input", {
    class: "range",
    id: "set-streams",
    type: "range",
    min: "1",
    max: "8",
    step: "1",
    value: String(settings?.transfer.streams ?? 4),
  }) as HTMLInputElement;
  streamsInput.addEventListener("input", () => {
    streamsValue.textContent = streamsInput.value;
  });

  const compressSelect = el(
    "select",
    { class: "select", id: "set-compress" },
    el("option", { value: "off", text: t("settings.compressOff") }),
    el("option", { value: "auto", text: t("settings.compressAuto") }),
  ) as HTMLSelectElement;
  compressSelect.value = settings?.transfer.compress ?? "off";

  const discoveryInput = el("input", {
    class: "checkbox",
    id: "set-discovery",
    type: "checkbox",
    checked: settings?.network.discovery !== false ? true : undefined,
  }) as HTMLInputElement;

  const themeSelect = el(
    "select",
    { class: "select", id: "set-theme" },
    el("option", { value: "system", text: t("settings.themeSystem") }),
    el("option", { value: "light", text: t("settings.themeLight") }),
    el("option", { value: "dark", text: t("settings.themeDark") }),
  ) as HTMLSelectElement;
  themeSelect.value = settings?.theme ?? "system";

  const languageSelect = el(
    "select",
    { class: "select", id: "set-language" },
    el("option", { value: "zh-CN", text: "中文（简体）" }),
  ) as HTMLSelectElement;
  languageSelect.value = settings?.language ?? "zh-CN";

  const saveButton = el(
    "button",
    {
      class: "btn primary",
      type: "button",
      id: "set-save",
      onclick: async () => {
        await saveSettings({
          displayName: nameInput.value.trim(),
          downloadDir: dirInput.value,
          udp: Number(udpInput.value) || 53310,
          tcp: Number(tcpInput.value) || 53311,
          streams: Number(streamsInput.value) || 4,
          compress: compressSelect.value === "auto" ? "auto" : "off",
          discovery: discoveryInput.checked,
          theme: themeSelect.value,
          language: languageSelect.value,
        });
      },
    },
    t("settings.save"),
  );

  const deviceCard = el(
    "section",
    { class: "card settings-card" },
    el("div", { class: "card-title", text: t("settings.group.device") }),
    field(t("settings.name"), nameInput, t("settings.nameHint")),
    field(t("settings.dir"), dirRow),
    el(
      "div",
      { class: "field" },
      el("span", { class: "field-label", text: t("settings.id") }),
      idRow,
      el("small", { class: "muted", id: "set-shortid", text: t("settings.shortId", { id: shortId }) }),
      el("small", { class: "muted", text: t("settings.resetHint") }),
    ),
  );

  const transferCard = el(
    "section",
    { class: "card settings-card" },
    el("div", { class: "card-title", text: t("settings.group.transfer") }),
    el(
      "div",
      { class: "field" },
      el("span", { class: "field-label", text: t("settings.streams") }),
      el("div", { class: "row" }, streamsInput, streamsValue),
      el("small", { class: "muted", text: t("settings.streamsHint", { n: streamsInput.value }) }),
    ),
    field(t("settings.compress"), compressSelect),
  );

  const networkCard = el(
    "section",
    { class: "card settings-card" },
    el("div", { class: "card-title", text: t("settings.group.network") }),
    el(
      "div",
      { class: "grid-2" },
      field(t("settings.udp"), udpInput),
      field(t("settings.tcp"), tcpInput),
    ),
    el("small", { class: "muted", text: t("settings.portsHint") }),
    el(
      "label",
      { class: "switch" },
      discoveryInput,
      el("span", { text: t("settings.discovery") }),
      el("small", { class: "muted", text: t("settings.discoveryHint") }),
    ),
    el(
      "button",
      { class: "btn ghost small", type: "button", disabled: true, title: t("settings.firewallHint") },
      t("settings.firewall"),
    ),
  );

  const appearanceCard = el(
    "section",
    { class: "card settings-card" },
    el("div", { class: "card-title", text: t("settings.group.appearance") }),
    el("div", { class: "grid-2" }, field(t("settings.theme"), themeSelect), field(t("settings.language"), languageSelect)),
  );

  const aboutCard = el(
    "section",
    { class: "card settings-card" },
    el("div", { class: "card-title", text: t("settings.group.about") }),
    el("div", { class: "about-lines" },
      el("div", { id: "about-version", text: version ? t("settings.about.version", { version }) : "" }),
      el("div", { text: t("settings.about.license") }),
      el("div", { text: t("settings.about.docs") }),
    ),
  );

  return el(
    "div",
    { class: "settings-view" },
    el("div", { class: "page-head" }, el("h1", { class: "page-title", text: t("settings.title") })),
    deviceCard,
    transferCard,
    networkCard,
    appearanceCard,
    aboutCard,
    el("div", { class: "settings-actions" }, saveButton),
  );
}
