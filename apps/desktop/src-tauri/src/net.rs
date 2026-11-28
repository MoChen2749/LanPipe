//! 把 `lanpipe-net` 的发现/接收/发送能力桥接成 Tauri 命令与事件。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use lanpipe_net::{
    send_paths, start_discovery, start_receiver, DiscoveryEvent, DiscoveryHandle, PeerInfo,
    ReceiverHandle, TransferEvent,
};
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Default)]
pub struct NetState {
    peers: Mutex<HashMap<String, PeerInfo>>,
    handles: Mutex<Option<NetHandles>>,
}

struct NetHandles {
    discovery: DiscoveryHandle,
    receiver: ReceiverHandle,
    forwarders: Vec<tauri::async_runtime::JoinHandle<()>>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PeerView {
    pub id: String,
    pub name: String,
    pub addr: String,
    /// 设备短 ID：IPv4 最后一段三位数字（如 005），用于快速识别。
    pub short_id: String,
}

/// 由 IP 生成三位短 ID（005、010、123…）。
pub(crate) fn short_id_from_addr(addr: &str) -> String {
    let host = addr.rsplit(':').nth(1).unwrap_or(addr);
    if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
        format!("{:03}", ip.octets()[3])
    } else {
        format!("{:03}", 0)
    }
}

/// 本机短 ID：优先默认路由出口网卡的 IPv4 末段，其次任意非回环 IPv4。
fn local_short_id() -> String {
    if let Some(ip) = primary_local_ip() {
        return format!("{:03}", ip.octets()[3]);
    }
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        for iface in ifaces {
            if iface.is_loopback() {
                continue;
            }
            if let std::net::IpAddr::V4(ip) = iface.ip() {
                return format!("{:03}", ip.octets()[3]);
            }
        }
    }
    "000".to_string()
}

/// 枚举本机所有子网中「最后一段等于 octet」的候选地址。
/// 支持 /24、/22、/16 等（大于 /16 的网段跳过，避免一次探测 6 万个地址）。
pub(crate) fn scan_candidates(octet: u8, port: u16) -> Vec<std::net::SocketAddrV4> {
    let mut out = Vec::new();
    let Ok(ifaces) = if_addrs::get_if_addrs() else {
        return out;
    };
    for iface in ifaces {
        if iface.is_loopback() {
            continue;
        }
        let std::net::IpAddr::V4(ip) = iface.ip() else {
            continue;
        };
        let mask = match iface.addr {
            if_addrs::IfAddr::V4(v4) => u32::from(v4.netmask),
            _ => 0xffff_ff00,
        };
        let network = u32::from(ip) & mask;
        let broadcast = network | !mask;
        if broadcast.saturating_sub(network) > 65_535 {
            continue;
        }
        let mut base = network & 0xffff_ff00;
        loop {
            let candidate = base | octet as u32;
            if candidate > network && candidate < broadcast {
                out.push(std::net::SocketAddrV4::new(
                    std::net::Ipv4Addr::from(candidate),
                    port,
                ));
            }
            if base >= broadcast {
                break;
            }
            base = base.saturating_add(256);
        }
    }
    out.sort();
    out.dedup();
    out
}

fn peers_snapshot(app: &AppHandle) -> Vec<PeerView> {
    let state = app.state::<NetState>();
    let peers = state.peers.lock().unwrap_or_else(|e| e.into_inner());
    let mut list: Vec<PeerView> = peers
        .values()
        .map(|p| {
            let addr = p.addr.to_string();
            PeerView {
                id: p.id.clone(),
                name: p.name.clone(),
                short_id: short_id_from_addr(&addr),
                addr,
            }
        })
        .collect();
    list.sort_by(|a, b| a.name.cmp(&b.name).then(a.addr.cmp(&b.addr)));
    list
}

fn transfer_json(ev: &TransferEvent) -> serde_json::Value {
    match ev {
        TransferEvent::Incoming {
            session_id,
            root_name,
            files,
            bytes,
        } => json!({
            "kind": "incoming", "sessionId": session_id, "rootName": root_name,
            "files": files, "bytes": bytes
        }),
        TransferEvent::ReceiveProgress {
            session_id,
            files_done,
            files_total,
            bytes_done,
            bytes_total,
        } => json!({
            "kind": "receiveProgress", "sessionId": session_id,
            "filesDone": files_done, "filesTotal": files_total,
            "bytesDone": bytes_done, "bytesTotal": bytes_total
        }),
        TransferEvent::FileReceived {
            session_id,
            path,
            size,
        } => json!({
            "kind": "fileReceived", "sessionId": session_id, "path": path, "size": size
        }),
        TransferEvent::Received {
            session_id,
            files,
            bytes,
            elapsed_ms,
            root,
        } => json!({
            "kind": "received", "sessionId": session_id, "files": files,
            "bytes": bytes, "elapsedMs": elapsed_ms, "root": root
        }),
        TransferEvent::SendProgress {
            session_id,
            files_done,
            files_total,
            bytes_done,
            bytes_total,
        } => json!({
            "kind": "sendProgress", "sessionId": session_id,
            "filesDone": files_done, "filesTotal": files_total,
            "bytesDone": bytes_done, "bytesTotal": bytes_total
        }),
        TransferEvent::Sent {
            session_id,
            files,
            bytes,
            elapsed_ms,
        } => json!({
            "kind": "sent", "sessionId": session_id, "files": files,
            "bytes": bytes, "elapsedMs": elapsed_ms
        }),
        TransferEvent::Failed {
            session_id,
            message,
        } => json!({ "kind": "failed", "sessionId": session_id, "message": message }),
    }
}

/// 启动阶段埋点：桌面写日志文件，Android 额外打到 logcat（临时文件目录在 Android 上不可写）。
fn trace(msg: &str) {
    crate::log(msg);
    #[cfg(target_os = "android")]
    eprintln!("[lanpipe] {msg}");
}

/// 启动发现 + 接收监听（会先停掉旧实例）。
pub async fn start(app: &AppHandle) -> Result<(), String> {
    trace("net: start begin");
    stop(app);
    trace("net: stop done");
    let cfg = crate::settings::snapshot(app)?;
    trace(&format!(
        "net: snapshot udp={} tcp={}",
        cfg.listen.udp, cfg.listen.tcp
    ));
    let (discovery, mut discovery_rx) = start_discovery(
        cfg.device_id.clone(),
        cfg.display_name.clone(),
        cfg.listen.udp,
        cfg.listen.tcp,
    )
    .await?;
    trace("net: discovery ready");

    let bind: std::net::SocketAddr = format!("0.0.0.0:{}", cfg.listen.tcp)
        .parse()
        .map_err(|e| format!("TCP 端口无效: {e}"))?;
    trace(&format!("net: receiver binding {bind}"));
    let (receiver, mut transfer_rx) = start_receiver(bind, PathBuf::from(&cfg.download_dir)).await?;
    trace("net: receiver ready");

    let app_disc = app.clone();
    let disc_task = tauri::async_runtime::spawn(async move {
        while let Some(ev) = discovery_rx.recv().await {
            {
                let state = app_disc.state::<NetState>();
                let mut peers = state.peers.lock().unwrap_or_else(|e| e.into_inner());
                match ev {
                    DiscoveryEvent::Up(peer) => {
                        crate::log(&format!("peer up {} {}", peer.name, peer.addr));
                        peers.insert(peer.id.clone(), peer);
                    }
                    DiscoveryEvent::Down(id) => {
                        crate::log(&format!("peer down {id}"));
                        peers.remove(&id);
                    }
                }
            }
            let _ = app_disc.emit("peers:update", peers_snapshot(&app_disc));
        }
    });

    let app_tr = app.clone();
    let transfer_task = tauri::async_runtime::spawn(async move {
        while let Some(ev) = transfer_rx.recv().await {
            // 逐文件的 FileReceived 不送前端：每次 IPC + JSON 序列化在万级文件时会吃掉吞吐。
            if matches!(ev, TransferEvent::FileReceived { .. }) {
                continue;
            }
            let _ = app_tr.emit("transfer:event", transfer_json(&ev));
        }
    });

    // 周期性把当前设备快照推给前端：避免窗口晚加载时错过一次性的 Up 事件。
    let app_poll = app.clone();
    let poll_task = tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let _ = app_poll.emit("peers:update", peers_snapshot(&app_poll));
        }
    });

    let state = app.state::<NetState>();
    *state.handles.lock().unwrap_or_else(|e| e.into_inner()) = Some(NetHandles {
        discovery,
        receiver,
        forwarders: vec![disc_task, transfer_task, poll_task],
    });
    Ok(())
}

/// 停止监听与发现。
pub fn stop(app: &AppHandle) {
    let state = app.state::<NetState>();
    let taken = state.handles.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(mut handles) = taken {
        for task in handles.forwarders.drain(..) {
            task.abort();
        }
        handles.discovery.stop();
        handles.receiver.stop();
    }
    state
        .peers
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
}

/// 诊断页用：逐块网卡（名称 / IPv4 / 掩码 / 前缀 / 短 ID）。
pub(crate) fn interface_details() -> Vec<serde_json::Value> {
    let mut rows = Vec::new();
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        for iface in ifaces {
            if iface.is_loopback() {
                continue;
            }
            if let if_addrs::IfAddr::V4(v4) = iface.addr {
                let prefix = v4.netmask.octets().iter().map(|b| b.count_ones()).sum::<u32>();
                rows.push(json!({
                    "name": iface.name,
                    "ip": v4.ip.to_string(),
                    "netmask": v4.netmask.to_string(),
                    "prefix": prefix,
                    "shortId": format!("{:03}", v4.ip.octets()[3]),
                }));
            }
        }
    }
    rows.sort_by(|a, b| {
        a.get("name")
            .and_then(|v| v.as_str())
            .cmp(&b.get("name").and_then(|v| v.as_str()))
    });
    rows
}

#[tauri::command]
pub fn list_peers(app: AppHandle) -> Vec<PeerView> {
    peers_snapshot(&app)
}

#[tauri::command]
pub fn local_info(app: AppHandle) -> Result<serde_json::Value, String> {
    let cfg = crate::settings::snapshot(&app)?;
    let state = app.state::<NetState>();
    let handles = state.handles.lock().unwrap_or_else(|e| e.into_inner());
    let (udp_port, tcp_port) = match handles.as_ref() {
        Some(h) => (h.discovery.udp_port, h.receiver.local_addr.port()),
        None => (cfg.listen.udp, cfg.listen.tcp),
    };
    Ok(json!({
        "id": cfg.device_id, "name": cfg.display_name,
        "shortId": local_short_id(),
        "udpPort": udp_port, "tcpPort": tcp_port,
        "downloadDir": cfg.download_dir,
        "ipv4": local_ipv4_addrs(),
    }))
}

/// 扫描本机**所有子网**中「最后一段等于 octet」的设备（输入 5 / 005 / 123 均可）。
#[tauri::command]
pub async fn scan_last_octet(
    app: AppHandle,
    octet: u16,
    port: Option<u16>,
) -> Result<Vec<PeerView>, String> {
    if octet > 255 {
        return Err("尾号必须在 0–255 之间".into());
    }
    let cfg = crate::settings::snapshot(&app)?;
    let mut targets = scan_candidates(octet as u8, port.unwrap_or(cfg.listen.tcp));
    if port.is_none() && cfg.listen.tcp != lanpipe_proto::DEFAULT_TCP_PORT {
        targets.extend(scan_candidates(
            octet as u8,
            lanpipe_proto::DEFAULT_TCP_PORT,
        ));
        targets.sort();
        targets.dedup();
    }

    let mut tasks = Vec::new();
    for target in targets {
        tasks.push(tokio::spawn(async move {
            let addr = std::net::SocketAddr::V4(target);
            match tokio::time::timeout(
                std::time::Duration::from_millis(700),
                tokio::net::TcpStream::connect(addr),
            )
            .await
            {
                Ok(Ok(_)) => Some(target),
                _ => None,
            }
        }));
    }
    let short = format!("{octet:03}");
    let mut found = Vec::new();
    for task in tasks {
        if let Ok(Some(addr)) = task.await {
            found.push(PeerView {
                id: format!("direct:{addr}"),
                name: format!("设备 {short}"),
                short_id: short.clone(),
                addr: addr.to_string(),
            });
        }
    }
    Ok(found)
}

#[tauri::command]
pub async fn restart_net(app: AppHandle) -> Result<(), String> {
    start(&app).await
}

#[tauri::command]
pub async fn start_send(
    app: AppHandle,
    target: String,
    paths: Vec<String>,
    streams: Option<u8>,
) -> Result<String, String> {
    if paths.is_empty() {
        return Err("没有待发送的文件".into());
    }
    let cfg = crate::settings::snapshot(&app)?;
    let addr_text = if target.contains(':') {
        target.clone()
    } else {
        format!("{target}:{}", cfg.listen.tcp)
    };
    let addr: std::net::SocketAddr = addr_text
        .parse()
        .map_err(|_| format!("地址格式不正确: {target}"))?;
    let stream_count = streams
        .map(|s| s as usize)
        .unwrap_or(cfg.transfer.streams as usize);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<TransferEvent>();
    let app_ev = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(ev) = rx.recv().await {
            if matches!(ev, TransferEvent::FileReceived { .. }) {
                continue;
            }
            let _ = app_ev.emit("transfer:event", transfer_json(&ev));
        }
    });

    let error_tx = tx.clone();
    let app_task = app.clone();
    tauri::async_runtime::spawn(async move {
        let files: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        if let Err(err) = send_paths(addr, files, stream_count, tx).await {
            crate::log(&format!("send_paths failed: {err}"));
            let _ = app_task.emit(
                "transfer:event",
                transfer_json(&TransferEvent::Failed {
                    session_id: None,
                    message: err,
                }),
            );
        }
        drop(error_tx);
    });
    Ok(addr_text)
}

/// 默认路由出口的本地 IPv4（不实际发包）。
fn primary_local_ip() -> Option<std::net::Ipv4Addr> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    match sock.local_addr().ok()? {
        std::net::SocketAddr::V4(v4) => Some(*v4.ip()),
        _ => None,
    }
}

/// 本机全部非回环 IPv4（默认路由出口地址排在最前），用于「信息」气泡展示。
pub(crate) fn local_ipv4_addrs() -> Vec<String> {
    let primary = primary_local_ip();
    let mut addrs: Vec<std::net::Ipv4Addr> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|iface| !iface.is_loopback())
        .filter_map(|iface| match iface.addr {
            if_addrs::IfAddr::V4(v4) => Some(v4.ip),
            _ => None,
        })
        .collect();
    addrs.sort_by_key(|ip| {
        if Some(*ip) == primary {
            0
        } else if ip.is_private() {
            1
        } else {
            2
        }
    });
    addrs.dedup();
    addrs.into_iter().map(|ip| ip.to_string()).collect()
}