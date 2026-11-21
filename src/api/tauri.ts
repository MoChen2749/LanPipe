import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface AppConfig {
  configVersion: number;
  deviceId: string;
  displayName: string;
  downloadDir: string;
  listen: { udp: number; tcp: number };
  transfer: { streams: number; compress: "off" | "auto" };
  network: { interface: string | null; discovery: boolean; subnetScan: boolean };
  recentPeers: { addr: string; name: string }[];
  theme: string;
  language: string;
  /** 运行期补充：本机短 ID（由 local_info 填充，不落盘） */
  shortId?: string;
}

export interface PeerView {
  id: string;
  name: string;
  addr: string;
  shortId: string;
}

export interface LocalInfo {
  id: string;
  name: string;
  udpPort: number;
  tcpPort: number;
  downloadDir: string;
  shortId?: string;
  ipv4?: string[];
}

export interface PathSummary {
  files: number;
  bytes: number;
}

export type TransferEvent =
  | { kind: "incoming"; sessionId: string; rootName: string; files: number; bytes: number }
  | {
      kind: "receiveProgress";
      sessionId: string;
      filesDone: number;
      filesTotal: number;
      bytesDone: number;
      bytesTotal: number;
    }
  | { kind: "fileReceived"; sessionId: string; path: string; size: number }
  | {
      kind: "received";
      sessionId: string;
      files: number;
      bytes: number;
      elapsedMs: number;
      root: string;
    }
  | {
      kind: "sendProgress";
      sessionId: string;
      filesDone: number;
      filesTotal: number;
      bytesDone: number;
      bytesTotal: number;
    }
  | { kind: "sent"; sessionId: string; files: number; bytes: number; elapsedMs: number }
  | { kind: "failed"; sessionId: string | null; message: string };

export const api = {
  getSettings: () => invoke<AppConfig>("get_settings"),
  saveSettings: (config: AppConfig) => invoke<void>("save_settings", { config }),
  resetDeviceId: () => invoke<string>("reset_device_id"),
  configLocation: () => invoke<string | null>("config_location"),
  checkDownloadDir: (dir: string) => invoke<string>("check_download_dir", { dir }),
  summarizePaths: (paths: string[]) => invoke<PathSummary>("summarize_paths", { paths }),
  listPeers: () => invoke<PeerView[]>("list_peers"),
  localInfo: () => invoke<LocalInfo>("local_info"),
  restartNet: () => invoke<void>("restart_net"),
  startSend: (target: string, paths: string[], streams?: number) =>
    invoke<string>("start_send", { target, paths, streams }),
  scanLastOctet: (octet: number, port?: number) =>
    invoke<PeerView[]>("scan_last_octet", { octet, port }),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  backdropInfo: () => invoke<string>("backdrop_info"),
  windowShadow: () => invoke<boolean>("window_shadow"),
};

export const events = {
  peers: (handler: (peers: PeerView[]) => void): Promise<UnlistenFn> =>
    listen<PeerView[]>("peers:update", (event) => handler(event.payload)),
  transfer: (handler: (event: TransferEvent) => void): Promise<UnlistenFn> =>
    listen<TransferEvent>("transfer:event", (event) => handler(event.payload)),
};
