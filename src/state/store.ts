import type { AppConfig, LocalInfo, PeerView, TransferEvent } from "../api/tauri";

export interface StagedSummary {
  files: number;
  bytes: number;
}

export interface TransferView {
  direction: "send" | "receive";
  sessionId: string;
  peerName: string;
  rootName: string;
  filesDone: number;
  filesTotal: number;
  bytesDone: number;
  bytesTotal: number;
  startedAt: number;
  updatedAt: number;
  speed: number;
  state: "active" | "done" | "failed";
  message?: string;
}

export interface HistoryEntry {
  id: string;
  direction: "send" | "receive";
  peerName: string;
  name: string;
  files: number;
  bytes: number;
  ok: boolean;
  time: number;
  message?: string;
}

export interface AppState {
  settings: AppConfig | null;
  local: LocalInfo | null;
  peers: PeerView[];
  staged: string[];
  stagedSummary: StagedSummary | null;
  transfer: TransferView | null;
  history: HistoryEntry[];
  configPath: string | null;
}

type Listener = () => void;

const state: AppState = {
  settings: null,
  local: null,
  peers: [],
  staged: [],
  stagedSummary: null,
  transfer: null,
  history: [],
  configPath: null,
};

const listeners = new Set<Listener>();

export function getState(): AppState {
  return state;
}

export function subscribe(fn: Listener): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function notify(): void {
  for (const fn of listeners) fn();
}

export function setSettings(settings: AppConfig): void {
  state.settings = settings;
  notify();
}

export function setLocal(local: LocalInfo): void {
  state.local = local;
  if (local.shortId && state.settings) state.settings.shortId = local.shortId;
  notify();
}

export function setPeers(peers: PeerView[]): void {
  state.peers = peers;
  notify();
}

export function setStaged(paths: string[], summary: StagedSummary | null): void {
  state.staged = paths;
  state.stagedSummary = summary;
  notify();
}

export function removeStaged(path: string): void {
  state.staged = state.staged.filter((p) => p !== path);
  notify();
}

export function setTransfer(transfer: TransferView | null): void {
  state.transfer = transfer;
  notify();
}

export function setConfigPath(path: string | null): void {
  state.configPath = path;
  notify();
}

export function pushHistory(entry: HistoryEntry): void {
  state.history = [entry, ...state.history].slice(0, 100);
  notify();
}

function inferPeerName(sessionId: string): string {
  const peer = state.peers.find((p) => p.id === sessionId);
  return peer?.name ?? "";
}

/** 把后端传输事件折叠进 UI 状态（进度节流由后端保证）。 */
export function applyTransferEvent(event: TransferEvent): void {
  const now = Date.now();
  switch (event.kind) {
    case "incoming":
      setTransfer({
        direction: "receive",
        sessionId: event.sessionId,
        peerName: inferPeerName(event.sessionId),
        rootName: event.rootName,
        filesDone: 0,
        filesTotal: event.files,
        bytesDone: 0,
        bytesTotal: event.bytes,
        startedAt: now,
        updatedAt: now,
        speed: 0,
        state: "active",
      });
      return;
    case "receiveProgress":
    case "sendProgress": {
      const current = state.transfer;
      const direction = event.kind === "receiveProgress" ? "receive" : "send";
      const isSame =
        current &&
        current.direction === direction &&
        current.sessionId === event.sessionId;
      const deltaBytes = isSame ? event.bytesDone - current.bytesDone : 0;
      const deltaMs = isSame ? now - current.updatedAt : 0;
      const speed = deltaMs > 200 ? (deltaBytes / deltaMs) * 1000 : (current?.speed ?? 0);
      setTransfer({
        direction,
        sessionId: event.sessionId,
        peerName: current?.peerName ?? inferPeerName(event.sessionId),
        rootName: current?.rootName ?? "",
        filesDone: event.filesDone,
        filesTotal: event.filesTotal,
        bytesDone: event.bytesDone,
        bytesTotal: event.bytesTotal,
        startedAt: current?.startedAt ?? now,
        updatedAt: now,
        speed,
        state: "active",
      });
      return;
    }
    case "received": {
      const current = state.transfer;
      setTransfer({
        direction: "receive",
        sessionId: event.sessionId,
        peerName: current?.peerName ?? inferPeerName(event.sessionId),
        rootName: current?.rootName ?? event.root,
        filesDone: event.files,
        filesTotal: event.files,
        bytesDone: event.bytes,
        bytesTotal: event.bytes,
        startedAt: current?.startedAt ?? now,
        updatedAt: now,
        speed: current?.speed ?? 0,
        state: "done",
      });
      pushHistory({
        id: event.sessionId,
        direction: "receive",
        peerName: current?.peerName ?? inferPeerName(event.sessionId),
        name: current?.rootName ?? event.root.split(/[\\/]/).pop() ?? "",
        files: event.files,
        bytes: event.bytes,
        ok: true,
        time: now,
      });
      return;
    }
    case "sent": {
      const current = state.transfer;
      setTransfer({
        direction: "send",
        sessionId: event.sessionId,
        peerName: current?.peerName ?? "",
        rootName: current?.rootName ?? "",
        filesDone: event.files,
        filesTotal: event.files,
        bytesDone: event.bytes,
        bytesTotal: event.bytes,
        startedAt: current?.startedAt ?? now,
        updatedAt: now,
        speed: current?.speed ?? 0,
        state: "done",
      });
      pushHistory({
        id: event.sessionId,
        direction: "send",
        peerName: current?.peerName ?? "",
        name: current?.rootName ?? "",
        files: event.files,
        bytes: event.bytes,
        ok: true,
        time: now,
      });
      return;
    }
    case "failed": {
      const current = state.transfer;
      setTransfer({
        direction: current?.direction ?? "receive",
        sessionId: event.sessionId ?? "",
        peerName: current?.peerName ?? "",
        rootName: current?.rootName ?? "",
        filesDone: current?.filesDone ?? 0,
        filesTotal: current?.filesTotal ?? 0,
        bytesDone: current?.bytesDone ?? 0,
        bytesTotal: current?.bytesTotal ?? 0,
        startedAt: current?.startedAt ?? now,
        updatedAt: now,
        speed: 0,
        state: "failed",
        message: event.message,
      });
      pushHistory({
        id: event.sessionId ?? `failed-${now}`,
        direction: current?.direction ?? "receive",
        peerName: current?.peerName ?? "",
        name: current?.rootName ?? "",
        files: current?.filesDone ?? 0,
        bytes: current?.bytesDone ?? 0,
        ok: false,
        time: now,
        message: event.message,
      });
      return;
    }
    default:
      return;
  }
}
