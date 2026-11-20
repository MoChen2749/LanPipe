import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import "./styles.css";

interface ListenConfig {
  udp: number;
  tcp: number;
}
interface TransferConfig {
  streams: number;
  compress: "off" | "auto";
}
interface NetworkConfig {
  interface: string | null;
  discovery: boolean;
  subnetScan: boolean;
}
interface PeerEntry {
  addr: string;
  name: string;
}
interface AppConfig {
  configVersion: number;
  deviceId: string;
  displayName: string;
  downloadDir: string;
  listen: ListenConfig;
  transfer: TransferConfig;
  network: NetworkConfig;
  recentPeers: PeerEntry[];
  theme: string;
  language: string;
}
interface StagedSummary {
  files: number;
  bytes: number;
}
interface PeerView {
  id: string;
  name: string;
  addr: string;
  shortId: string;
}
type TransferEvent =
  | { kind: "incoming"; sessionId: string; rootName: string; files: number; bytes: number }
  | { kind: "receiveProgress"; filesDone: number; filesTotal: number; bytesDone: number; bytesTotal: number }
  | { kind: "fileReceived"; path: string; size: number }
  | { kind: "received"; files: number; bytes: number; elapsedMs: number; root: string }
  | { kind: "sendProgress"; filesDone: number; filesTotal: number; bytesDone: number; bytesTotal: number }
  | { kind: "sent"; files: number; bytes: number; elapsedMs: number }
  | { kind: "failed"; sessionId: string | null; message: string };

let config: AppConfig;
let stagedPaths: string[] = [];
let stagedSummary: StagedSummary | null = null;
let peers: PeerView[] = [];
let configPath: string | null = null;
let localShortId = "000";

const $ = <T extends HTMLElement>(id: string): T => {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing element #${id}`);
  return el as T;
};

const fmtBytes = (n: number): string => {
  if (n < 1024) return `${n} B`;
  const units = ["KiB", "MiB", "GiB", "TiB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  return `${v.toFixed(v >= 100 ? 0 : 1)} ${units[i]}`;
};

const fmtSpeed = (bytes: number, ms: number): string => {
  if (ms <= 0) return "";
  return `${fmtBytes((bytes / ms) * 1000)}/s`;
};

const setStatus = (text: string, isError = false): void => {
  const el = $("status");
  el.textContent = text;
  el.classList.toggle("err", isError);
  if (text) {
    window.setTimeout(() => {
      if (el.textContent === text) el.textContent = "";
    }, 5000);
  }
};

const shortId = (id: string): string => (id.length > 8 ? `${id.slice(0, 8)}…` : id);

async function loadSettings(): Promise<void> {
  [config, configPath] = await Promise.all([
    invoke<AppConfig>("get_settings"),
    invoke<string | null>("config_location").catch(() => null)
  ]);
  try {
    const info = await invoke<{ shortId?: string }>("local_info");
    localShortId = info.shortId ?? "000";
  } catch {
    localShortId = "000";
  }
  render();
}

function render(): void {
  $("self-name").textContent = config.displayName;
  const selfId = $("self-id");
  selfId.textContent = shortId(config.deviceId);
  selfId.dataset.full = config.deviceId;

  ($("display-name") as HTMLInputElement).value = config.displayName;
  ($("download-dir") as HTMLInputElement).value = config.downloadDir;
  ($("device-id") as HTMLInputElement).value = config.deviceId;
  const shortEl = document.getElementById("device-short-id");
  if (shortEl) shortEl.textContent = `#${localShortId}`;
  ($("port-udp") as HTMLInputElement).value = String(config.listen.udp);
  ($("port-tcp") as HTMLInputElement).value = String(config.listen.tcp);
  ($("streams") as HTMLInputElement).value = String(config.transfer.streams);
  $("streams-value").textContent = String(config.transfer.streams);
  ($("compress") as HTMLSelectElement).value = config.transfer.compress;
  $("diag").textContent = JSON.stringify(
    { configPath, ...config, deviceId: shortId(config.deviceId), stagedPaths, peers },
    null,
    2
  );
}

function collect(): AppConfig {
  return {
    ...config,
    displayName: ($("display-name") as HTMLInputElement).value.trim() || config.displayName,
    downloadDir: ($("download-dir") as HTMLInputElement).value.trim(),
    listen: {
      udp: clampPort(Number(($("port-udp") as HTMLInputElement).value)),
      tcp: clampPort(Number(($("port-tcp") as HTMLInputElement).value))
    },
    transfer: {
      streams: Number(($("streams") as HTMLInputElement).value),
      compress: ($("compress") as HTMLSelectElement).value as "off" | "auto"
    }
  };
}

const clampPort = (n: number): number =>
  Number.isFinite(n) ? Math.min(65535, Math.max(1024, Math.trunc(n))) : 53310;

async function save(): Promise<void> {
  try {
    config = await invoke<AppConfig>("save_settings", { config: collect() });
    await invoke("restart_net");
    render();
    setStatus("设置已保存，监听已重启");
    await refreshPeers();
  } catch (err) {
    setStatus(String(err), true);
  }
}

async function pickDir(): Promise<void> {
  const picked = await open({ directory: true, multiple: false, title: "选择接收保存目录" });
  if (typeof picked === "string" && picked.length > 0) {
    ($("download-dir") as HTMLInputElement).value = picked;
    await validateDir(picked);
  }
}

async function validateDir(path: string): Promise<void> {
  const el = $("dir-status");
  el.textContent = "检查中…";
  try {
    await invoke("check_download_dir", { path });
    el.textContent = "目录可用，将写入此位置";
  } catch (err) {
    el.textContent = `目录不可用：${err}`;
  }
}

async function resetId(): Promise<void> {
  if (!window.confirm("重置设备 ID？其他设备会把它当作一台新设备，历史缓存失效。")) return;
  try {
    config = await invoke<AppConfig>("reset_device_id");
    await invoke("restart_net");
    render();
    setStatus("设备 ID 已重置，监听已重启");
  } catch (err) {
    setStatus(String(err), true);
  }
}

async function stagePaths(paths: string[]): Promise<void> {
  stagedPaths = paths;
  const el = $("staged");
  if (paths.length === 0) {
    stagedSummary = null;
    el.textContent = "尚未选择内容";
    renderPeers();
    return;
  }
  el.textContent = "统计中…";
  try {
    stagedSummary = await invoke<StagedSummary>("summarize_paths", { paths });
    el.textContent = `已选择 ${paths.length} 个路径 · ${stagedSummary.files} 个文件 · ${fmtBytes(stagedSummary.bytes)}`;
  } catch (err) {
    stagedSummary = null;
    el.textContent = `统计失败：${err}`;
  }
  renderPeers();
  render();
}

function renderPeers(): void {
  const list = $("device-list");
  list.innerHTML = "";
  if (peers.length === 0) {
    const li = document.createElement("li");
    li.className = "empty";
    li.textContent = "正在搜索局域网设备…（也可以直接用 IP 直连）";
    list.appendChild(li);
    return;
  }
  for (const peer of peers) {
    const li = document.createElement("li");
    const btn = document.createElement("button");
    btn.className = "device";
    btn.title = stagedPaths.length
      ? `发送到 ${peer.name}（#${peer.shortId} · ${peer.addr}）`
      : "先拖入文件或文件夹";
    btn.disabled = stagedPaths.length === 0;

    const name = document.createElement("span");
    name.className = "device-name";
    name.textContent = peer.name;
    const addr = document.createElement("span");
    addr.className = "device-addr";
    addr.textContent = `#${peer.shortId} · ${peer.addr}`;
    addr.title = `完整 ID：${peer.id}`;
    btn.append(name, addr);
    btn.addEventListener("click", () => void sendTo(peer.addr, peer.name));
    li.appendChild(btn);
    list.appendChild(li);
  }
}

async function sendTo(target: string, label?: string): Promise<void> {
  if (stagedPaths.length === 0) {
    setStatus("先拖入文件或文件夹", true);
    return;
  }
  setTransfer(`正在连接 ${label ?? target}…`, 0, "等待接收端接受…");
  try {
    await invoke<string>("start_send", {
      target,
      paths: stagedPaths,
      streams: config.transfer.streams
    });
  } catch (err) {
    setStatus(String(err), true);
    setTransfer("发送失败", 0, String(err));
  }
}

function setTransfer(title: string, percent: number, hint: string): void {
  $("transfer-title").textContent = title;
  $("transfer-percent").textContent = percent > 0 ? `${Math.min(100, percent).toFixed(0)}%` : "";
  ($("progress-bar") as HTMLElement).style.width = `${Math.min(100, Math.max(0, percent))}%`;
  $("transfer-hint").textContent = hint;
}

function handleTransfer(ev: TransferEvent): void {
  switch (ev.kind) {
    case "incoming":
      setTransfer(`接收中：${ev.rootName}`, 0, `共 ${ev.files} 个文件 · ${fmtBytes(ev.bytes)}`);
      break;
    case "receiveProgress":
    case "sendProgress": {
      const pct = ev.bytesTotal > 0 ? (ev.bytesDone / ev.bytesTotal) * 100 : 0;
      const tag = ev.kind === "receiveProgress" ? "接收中" : "发送中";
      setTransfer(
        `${tag} ${ev.filesDone}/${ev.filesTotal} 个文件`,
        pct,
        `${fmtBytes(ev.bytesDone)} / ${fmtBytes(ev.bytesTotal)}`
      );
      break;
    }
    case "received":
      setTransfer("接收完成", 100, `${ev.files} 个文件 · ${fmtBytes(ev.bytes)} · ${(ev.elapsedMs / 1000).toFixed(1)}s · ${fmtSpeed(ev.bytes, ev.elapsedMs)}`);
      setStatus(`接收完成：${fmtBytes(ev.bytes)}`);
      void refreshPeers();
      break;
    case "sent":
      setTransfer("发送完成", 100, `${ev.files} 个文件 · ${fmtBytes(ev.bytes)} · ${(ev.elapsedMs / 1000).toFixed(1)}s · ${fmtSpeed(ev.bytes, ev.elapsedMs)}`);
      setStatus(`发送完成：${fmtBytes(ev.bytes)}`);
      break;
    case "failed":
      setTransfer("传输失败", 0, ev.message);
      setStatus(ev.message, true);
      break;
    default:
      break;
  }
}

async function scanOctet(octet: number): Promise<void> {
  if (octet < 0 || octet > 255) {
    $("direct-hint").textContent = "尾号必须在 0–255 之间";
    return;
  }
  $("direct-hint").textContent = `正在扫描本机各网段 *.${octet} …`;
  try {
    const found = await invoke<PeerView[]>("scan_last_octet", { octet });
    if (found.length === 0) {
      $("direct-hint").textContent = `网段 *.${octet} 未发现设备（确认对方已启动、防火墙已放行）`;
      return;
    }
    const added: PeerView[] = [];
    for (const peer of found) {
      if (!peers.some((x) => x.addr === peer.addr)) {
        peers.push(peer);
        added.push(peer);
      }
    }
    renderPeers();
    $("direct-hint").textContent = `扫描到 ${found.length} 台：${found
      .map((p) => `#${p.shortId} ${p.addr}`)
      .join(", ")}${added.length ? "（已加入上方设备列表，点击即可发送）" : ""}`;
  } catch (err) {
    $("direct-hint").textContent = `扫描失败：${err}`;
  }
}

async function refreshPeers(): Promise<void> {
  try {
    peers = await invoke<PeerView[]>("list_peers");
    renderPeers();
  } catch {
    /* 忽略：网络层未启动时保持空列表 */
  }
}

function bind(): void {
  $("save").addEventListener("click", () => void save());
  $("pick-dir").addEventListener("click", () => void pickDir());
  $("reset-id").addEventListener("click", () => void resetId());
  $("self-id").addEventListener("click", () => {
    const full = $("self-id").dataset.full ?? "";
    void navigator.clipboard.writeText(full).then(() => setStatus("设备 ID 已复制"));
  });

  $("streams").addEventListener("input", () => {
    $("streams-value").textContent = ($("streams") as HTMLInputElement).value;
  });

  $("download-dir").addEventListener("change", (e) => {
    void validateDir((e.target as HTMLInputElement).value);
  });

  $("refresh-peers").addEventListener("click", () => {
    void refreshPeers();
    setStatus("已刷新设备列表");
  });

  $("direct-connect").addEventListener("click", () => {
    const raw = ($("direct-ip") as HTMLInputElement).value.trim();
    if (/^\d{1,3}$/.test(raw)) {
      void scanOctet(Number(raw));
      return;
    }
    const ok = /^(\d{1,3}\.){3}\d{1,3}(:\d{2,5})?$/.test(raw);
    if (!ok) {
      $("direct-hint").textContent = "格式不正确：可只输尾号（如 5），或 192.168.1.5 / 192.168.1.5:53311";
      return;
    }
    $("direct-hint").textContent = `目标 ${raw}`;
    void sendTo(raw, raw);
  });

  // Tauri v2 的窗口级拖放事件（HTML5 drop 拿不到本地路径）
  void getCurrentWebview().onDragDropEvent((event) => {
    const zone = $("dropzone");
    if (event.payload.type === "over") {
      zone.classList.add("dragover");
    } else if (event.payload.type === "drop") {
      zone.classList.remove("dragover");
      void stagePaths(event.payload.paths ?? []);
    } else {
      zone.classList.remove("dragover");
    }
  });

  void listen<PeerView[]>("peers:update", (event) => {
    peers = event.payload;
    renderPeers();
  });
  void listen<TransferEvent>("transfer:event", (event) => handleTransfer(event.payload));
}

async function main(): Promise<void> {
  bind();
  await loadSettings();
  await validateDir(config.downloadDir);
  await refreshPeers();
  window.setInterval(() => void refreshPeers(), 4000);
  if (import.meta.env.DEV) console.info("LanPipe dev build");
}

void main().catch((err) => {
  document.body.innerHTML = `<pre style="padding:24px;color:#c0392b">LanPipe 启动失败：${String(err)}</pre>`;
});
