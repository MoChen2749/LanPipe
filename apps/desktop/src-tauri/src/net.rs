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
pub struct PeerView {
    pub id: String,
    pub name: String,
    pub addr: String,
}

fn peers_snapshot(app: &AppHandle) -> Vec<PeerView> {
    let state = app.state::<NetState>();
    let peers = state.peers.lock().unwrap_or_else(|e| e.into_inner());
    let mut list: Vec<PeerView> = peers
        .values()
        .map(|p| PeerView {
            id: p.id.clone(),
            name: p.name.clone(),
            addr: p.addr.to_string(),
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

/// 启动发现 + 接收监听（会先停掉旧实例）。
pub async fn start(app: &AppHandle) -> Result<(), String> {
    stop(app);
    let cfg = crate::settings::snapshot(app)?;
    let (discovery, mut discovery_rx) = start_discovery(
        cfg.device_id.clone(),
        cfg.display_name.clone(),
        cfg.listen.udp,
        cfg.listen.tcp,
    )
    .await?;

    let bind: std::net::SocketAddr = format!("0.0.0.0:{}", cfg.listen.tcp)
        .parse()
        .map_err(|e| format!("TCP 端口无效: {e}"))?;
    let (receiver, mut transfer_rx) = start_receiver(bind, PathBuf::from(&cfg.download_dir)).await?;

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
        "udpPort": udp_port, "tcpPort": tcp_port,
        "downloadDir": cfg.download_dir,
    }))
}

/// 扫描本地各网段中“最后一段等于 octet”的地址（典型用法：只输入 `5`，扫 x.y.z.5）。
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
    let tcp_port = port.unwrap_or(cfg.listen.tcp);
    let mut targets: Vec<std::net::SocketAddrV4> = Vec::new();
    match if_addrs::get_if_addrs() {
        Ok(ifaces) => {
            for iface in ifaces {
                if iface.is_loopback() {
                    continue;
                }
                if let std::net::IpAddr::V4(ip) = iface.ip() {
                    let o = ip.octets();
                    targets.push(std::net::SocketAddrV4::new(
                        std::net::Ipv4Addr::new(o[0], o[1], o[2], octet as u8),
                        tcp_port,
                    ));
                }
            }
        }
        Err(err) => return Err(format!("读取本机网卡失败: {err}")),
    }
    targets.sort();
    targets.dedup();

    let mut tasks = Vec::new();
    for target in targets {
        tasks.push(tokio::spawn(async move {
            let addr = std::net::SocketAddr::V4(target);
            match tokio::time::timeout(
                std::time::Duration::from_millis(600),
                tokio::net::TcpStream::connect(addr),
            )
            .await
            {
                Ok(Ok(_)) => Some(target),
                _ => None,
            }
        }));
    }
    let mut found = Vec::new();
    for task in tasks {
        if let Ok(Some(addr)) = task.await {
            found.push(PeerView {
                id: format!("direct:{addr}"),
                name: "IP 直连设备".to_string(),
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
