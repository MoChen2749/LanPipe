import { open } from "@tauri-apps/plugin-dialog";
import { api, type PeerView } from "./api/tauri";
import {
  applyTransferEvent,
  getState,
  notify,
  setConfigPath,
  setLocal,
  setPeers,
  setSettings,
  setStaged,
  setTransfer,
} from "./state/store";
import { t } from "./i18n";
import { isAndroid } from "./platform";
import { baseName } from "./ui/format";

let toastHandler: (text: string) => void = () => undefined;

export function setToastHandler(fn: (text: string) => void): void {
  toastHandler = fn;
}

export function toast(text: string): void {
  toastHandler(text);
}

export async function loadBootstrap(): Promise<void> {
  const [settings, local, configPath] = await Promise.all([
    api.getSettings(),
    api.localInfo(),
    api.configLocation().catch(() => null),
  ]);
  setSettings(settings);
  setLocal(local);
  setConfigPath(configPath);
  await refreshPeers();
}

export async function refreshPeers(): Promise<void> {
  try {
    setPeers(await api.listPeers());
  } catch {
    /* 发现服务重启期间忽略 */
  }
}

export function handleTransferEvent(event: Parameters<typeof applyTransferEvent>[0]): void {
  applyTransferEvent(event);
}

export async function stagePaths(paths: string[]): Promise<void> {
  const incoming = paths.filter(Boolean);
  if (incoming.length === 0) return;
  const merged = [...new Set([...getState().staged, ...incoming])];
  try {
    const summary = await api.summarizePaths(merged);
    setStaged(merged, summary);
  } catch (err) {
    toast(String(err));
  }
}

export function clearStaged(): void {
  setStaged([], null);
}

export function removeStaged(path: string): void {
  const next = getState().staged.filter((p) => p !== path);
  if (next.length === 0) {
    setStaged([], null);
    return;
  }
  void api.summarizePaths(next).then((summary) => setStaged(next, summary));
}

export async function pickFiles(): Promise<void> {
  if (isAndroid) {
    try {
      const paths = await api.androidPickFiles();
      if (paths.length > 0) await stagePaths(paths);
      return;
    } catch (err) {
      console.warn("[lanpipe] android_pick_files failed, fallback to dialog plugin", err);
    }
  }
  const picked = await open({ multiple: true, directory: false });
  if (!picked) return;
  await stagePaths(Array.isArray(picked) ? picked : [picked]);
}

export async function pickFolder(): Promise<void> {
  if (isAndroid) {
    try {
      const paths = await api.androidPickFolder();
      if (paths.length > 0) await stagePaths(paths);
      return;
    } catch (err) {
      console.warn("[lanpipe] android_pick_folder failed, fallback to dialog plugin", err);
    }
  }
  const picked = await open({ multiple: false, directory: true });
  if (!picked) return;
  await stagePaths(Array.isArray(picked) ? picked : [picked]);
}

export async function sendTo(peer: PeerView): Promise<void> {
  const state = getState();
  if (state.staged.length === 0) {
    toast(t("toast.needContent"));
    return;
  }
  const name = peer.name || peer.shortId;
  setTransfer({
    direction: "send",
    sessionId: `send-${Date.now()}`,
    peerName: name,
    rootName: state.staged.map((p) => baseName(p)).join("、"),
    filesDone: 0,
    filesTotal: 0,
    bytesDone: 0,
    bytesTotal: 0,
    startedAt: Date.now(),
    updatedAt: Date.now(),
    speed: 0,
    state: "active",
  });
  try {
    await api.startSend(peer.addr, state.staged, state.settings?.transfer.streams);
    toast(t("toast.sendStarted"));
  } catch (err) {
    toast(String(err));
  }
}

export async function sendToAddr(addr: string): Promise<void> {
  const state = getState();
  if (state.staged.length === 0) {
    toast(t("toast.needContent"));
    return;
  }
  setTransfer({
    direction: "send",
    sessionId: `send-${Date.now()}`,
    peerName: addr,
    rootName: state.staged.map((p) => baseName(p)).join("、"),
    filesDone: 0,
    filesTotal: 0,
    bytesDone: 0,
    bytesTotal: 0,
    startedAt: Date.now(),
    updatedAt: Date.now(),
    speed: 0,
    state: "active",
  });
  try {
    await api.startSend(addr, state.staged, state.settings?.transfer.streams);
    toast(t("toast.sendStarted"));
  } catch (err) {
    toast(String(err));
  }
}

export async function revealPath(path: string): Promise<void> {
  try {
    await api.revealPath(path);
  } catch (err) {
    toast(String(err));
  }
}

export async function scanOctet(octet: number): Promise<PeerView[]> {
  const tcp = getState().settings?.listen.tcp;
  const found = await api.scanLastOctet(octet, tcp);
  if (found.length > 0) {
    const merged = [...getState().peers];
    for (const peer of found) {
      if (!merged.some((p) => p.addr === peer.addr)) merged.push(peer);
    }
    setPeers(merged);
  }
  return found;
}

export async function saveSettings(patch: {
  displayName: string;
  downloadDir: string;
  udp: number;
  tcp: number;
  streams: number;
  compress: "off" | "auto";
  discovery: boolean;
  theme: string;
  language: string;
}): Promise<void> {
  const current = getState().settings;
  if (!current) return;
  const next = {
    ...current,
    displayName: patch.displayName,
    downloadDir: patch.downloadDir,
    listen: { udp: patch.udp, tcp: patch.tcp },
    transfer: { ...current.transfer, streams: patch.streams, compress: patch.compress },
    network: { ...current.network, discovery: patch.discovery },
    theme: patch.theme,
    language: patch.language,
  };
  await api.saveSettings(next);
  setSettings(next);
  await api.restartNet();
  await refreshPeers();
  toast(t("toast.saved"));
}

export async function resetDeviceId(): Promise<void> {
  const id = await api.resetDeviceId();
  const settings = getState().settings;
  if (settings) setSettings({ ...settings, deviceId: id });
  const local = getState().local;
  if (local) setLocal({ ...local, id });
  toast(t("toast.reset"));
}

export async function chooseDownloadDir(current: string): Promise<string | null> {
  const picked = await open({ multiple: false, directory: true, defaultPath: current });
  if (!picked) return null;
  const dir = Array.isArray(picked) ? picked[0] : picked;
  try {
    await api.checkDownloadDir(dir);
    toast(t("toast.dirOk"));
    return dir;
  } catch (err) {
    toast(String(err));
    return null;
  }
}

export function tick(): void {
  notify();
}
