//! 局域网发现：UDP 多播 + 受限广播，周期公告；对端超时下线。

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::{Duration, Instant};

use lanpipe_proto::{DEFAULT_UDP_PORT, MULTICAST_GROUP};
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio::time;

pub const ANNOUNCE_INTERVAL_MS: u64 = 2000;
pub const PEER_TIMEOUT_MS: u64 = 5000;

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub id: String,
    pub name: String,
    pub addr: SocketAddrV4,
}

#[derive(Debug)]
pub enum DiscoveryEvent {
    Up(PeerInfo),
    Down(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct Announce {
    #[serde(default)]
    addrs: Vec<String>,
    v: u8,
    id: String,
    name: String,
    port: u16,
}

pub struct DiscoveryHandle {
    pub udp_port: u16,
    tasks: Vec<JoinHandle<()>>,
}

impl Drop for DiscoveryHandle {
    fn drop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
    }
}

impl DiscoveryHandle {
    pub fn stop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
    }
}

/// 启动发现服务：监听 `udp_port`，周期向多播组与受限广播公告自身。
pub async fn start_discovery(
    device_id: String,
    device_name: String,
    udp_port: u16,
    control_port: u16,
) -> Result<(DiscoveryHandle, mpsc::UnboundedReceiver<DiscoveryEvent>), String> {
    let sock = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )
    .map_err(|e| format!("创建 UDP socket 失败: {e}"))?;
    sock.set_reuse_address(true).ok();
    sock.set_broadcast(true)
        .map_err(|e| format!("开启广播失败: {e}"))?;
    let bind_addr: SocketAddr = SocketAddr::from(([0, 0, 0, 0], udp_port));
    sock.bind(&bind_addr.into())
        .map_err(|e| format!("绑定 UDP {udp_port} 失败: {e}"))?;
    let std_sock: std::net::UdpSocket = sock.into();
    let udp = UdpSocket::from_std(std_sock).map_err(|e| format!("转为异步 socket 失败: {e}"))?;
    udp.set_multicast_loop_v4(true).ok();
    let group = Ipv4Addr::from(MULTICAST_GROUP);
    if let Err(e) = udp.join_multicast_v4(group, Ipv4Addr::UNSPECIFIED) {
        // 多播加入失败不致命，仍可用广播与手动 IP
        eprintln!("[lanpipe] 加入多播组失败(默认接口): {e}");
    }
    // 每块非回环网卡都加入多播组：Windows 上默认只挑一个接口，跨网段/VPN 多网卡时会漏收。
    let local_ifaces = local_ipv4_ifaces();
    for iface in &local_ifaces {
        if let Err(e) = udp.join_multicast_v4(group, *iface) {
            eprintln!("[lanpipe] 加入多播组失败({iface}): {e}");
        }
    }

    let udp = Arc::new(udp);
    let peers: Arc<Mutex<HashMap<String, (PeerInfo, Instant)>>> = Arc::new(Mutex::new(HashMap::new()));
    let (tx, rx) = mpsc::unbounded_channel();

    // 公告循环
    let announce_sock = Arc::clone(&udp);
    let announce_id = device_id.clone();
    let announce_name = device_name.clone();
    let announce = tokio::spawn(async move {
        let addrs = local_ipv4_ifaces()
            .into_iter()
            .map(|ip| ip.to_string())
            .collect();
        let payload = serde_json::to_vec(&Announce {
            v: 1,
            addrs,
            id: announce_id,
            name: announce_name,
            port: control_port,
        })
        .unwrap_or_default();
        let senders = build_iface_senders(udp_port);
        loop {
            if senders.is_empty() {
                // 兜底：没有任何可用网卡时仍试着走默认路由
                let mcast = SocketAddr::from((MULTICAST_GROUP, udp_port));
                let bcast = SocketAddr::from(([255, 255, 255, 255], udp_port));
                let _ = announce_sock.send_to(&payload, mcast).await;
                let _ = announce_sock.send_to(&payload, bcast).await;
            } else {
                for sender in &senders {
                    for target in &sender.targets {
                        let _ = sender.sock.send_to(&payload, target);
                    }
                }
            }
            time::sleep(Duration::from_millis(ANNOUNCE_INTERVAL_MS)).await;
        }
    });

    // 接收循环
    let recv_sock = Arc::clone(&udp);
    let recv_peers = Arc::clone(&peers);
    let recv_tx = tx.clone();
    let self_id = device_id.clone();
    let recv = tokio::spawn(async move {
        let mut buf = [0u8; 2048];
        loop {
            let (n, from) = match recv_sock.recv_from(&mut buf).await {
                Ok(v) => v,
                Err(_) => continue,
            };
            let Ok(msg) = serde_json::from_slice::<Announce>(&buf[..n]) else {
                continue;
            };
            if msg.id == self_id {
                continue;
            }
            let SocketAddr::V4(v4) = from else { continue };
            let peer = PeerInfo {
                id: msg.id.clone(),
                name: msg.name,
                addr: pick_peer_addr(*v4.ip(), &msg.addrs, msg.port),
            };
            let mut map = recv_peers.lock().await;
            let is_new = !map.contains_key(&msg.id);
            map.insert(msg.id, (peer.clone(), Instant::now()));
            drop(map);
            if is_new {
                let _ = recv_tx.send(DiscoveryEvent::Up(peer));
            }
        }
    });

    // 超时清扫
    let sweep_peers = Arc::clone(&peers);
    let sweep_tx = tx;
    let sweep = tokio::spawn(async move {
        loop {
            time::sleep(Duration::from_millis(1000)).await;
            let now = Instant::now();
            let expired: Vec<String> = {
                let mut map = sweep_peers.lock().await;
                let ids: Vec<String> = map
                    .iter()
                    .filter(|(_, (_, seen))| now.duration_since(*seen).as_millis() as u64 > PEER_TIMEOUT_MS)
                    .map(|(id, _)| id.clone())
                    .collect();
                for id in &ids {
                    map.remove(id);
                }
                ids
            };
            for id in expired {
                let _ = sweep_tx.send(DiscoveryEvent::Down(id));
            }
        }
    });

    Ok((
        DiscoveryHandle {
            udp_port,
            tasks: vec![announce, recv, sweep],
        },
        rx,
    ))
}

/// 默认发现端口。
pub const fn default_udp_port() -> u16 {
    DEFAULT_UDP_PORT
}

/// 枚举非回环 IPv4 网卡地址。
fn local_ipv4_ifaces() -> Vec<Ipv4Addr> {
    let mut out = Vec::new();
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        for iface in ifaces {
            if iface.is_loopback() {
                continue;
            }
            if let std::net::IpAddr::V4(ip) = iface.ip() {
                out.push(ip);
            }
        }
    }
    out.dedup();
    out
}

struct IfaceSender {
    sock: std::net::UdpSocket,
    targets: Vec<SocketAddr>,
}

/// 为每块网卡建一个发送 socket，并把多播出口固定到该网卡上（避免公告全从 WSL/VPN 虚拟网卡发出）。
fn build_iface_senders(udp_port: u16) -> Vec<IfaceSender> {
    let mut senders = Vec::new();
    let Ok(ifaces) = if_addrs::get_if_addrs() else {
        return senders;
    };
    let group = SocketAddr::from((MULTICAST_GROUP, udp_port));
    let limited_bcast = SocketAddr::from(([255, 255, 255, 255], udp_port));
    for iface in ifaces {
        if iface.is_loopback() {
            continue;
        }
        let std::net::IpAddr::V4(ip) = iface.ip() else {
            continue;
        };
        let sock = match socket2::Socket::new(
            socket2::Domain::IPV4,
            socket2::Type::DGRAM,
            Some(socket2::Protocol::UDP),
        ) {
            Ok(s) => s,
            Err(_) => continue,
        };
        sock.set_reuse_address(true).ok();
        sock.set_broadcast(true).ok();
        if sock.bind(&SocketAddr::from(([0, 0, 0, 0], 0)).into()).is_err() {
            continue;
        }
        if sock.set_multicast_if_v4(&ip).is_err() {
            continue;
        }
        sock.set_multicast_loop_v4(true).ok();
        let std_sock = std::net::UdpSocket::from(sock);
        let mut targets = vec![group];
        if let Some(bcast) = subnet_broadcast(iface) {
            targets.push(SocketAddr::V4(SocketAddrV4::new(bcast, udp_port)));
        }
        targets.push(limited_bcast);
        senders.push(IfaceSender {
            sock: std_sock,
            targets,
        });
    }
    senders
}

/// 由网卡掩码推算子网定向广播地址。
fn subnet_broadcast(iface: if_addrs::Interface) -> Option<Ipv4Addr> {
    match iface.addr {
        if_addrs::IfAddr::V4(v4) => {
            let ip = u32::from(v4.ip);
            let mask = u32::from(v4.netmask);
            Some(Ipv4Addr::from(ip | !mask))
        }
        _ => None,
    }
}

/// 选择对端地址：优先与“主用网卡（默认路由出口）”同 /24 的候选，
/// 其次与任意本地网卡同 /24 的候选，最后退回公告包的源地址。
fn pick_peer_addr(src: Ipv4Addr, advertised: &[String], port: u16) -> SocketAddrV4 {
    let locals = local_ipv4_ifaces();
    let primary = primary_local_ip();
    let mut parsed: Vec<Ipv4Addr> = advertised
        .iter()
        .filter_map(|s| s.parse::<Ipv4Addr>().ok())
        .collect();
    if !parsed.contains(&src) {
        parsed.push(src);
    }
    if let Some(primary) = primary {
        if let Some(ip) = parsed.iter().find(|ip| same_24(**ip, primary)) {
            return SocketAddrV4::new(*ip, port);
        }
    }
    if let Some(ip) = parsed
        .iter()
        .find(|ip| locals.iter().any(|local| same_24(**ip, *local)))
    {
        return SocketAddrV4::new(*ip, port);
    }
    SocketAddrV4::new(src, port)
}

fn same_24(a: Ipv4Addr, b: Ipv4Addr) -> bool {
    let (a, b) = (a.octets(), b.octets());
    a[0] == b[0] && a[1] == b[1] && a[2] == b[2]
}

/// 默认路由出口的本地 IPv4（不实际发包）。
fn primary_local_ip() -> Option<Ipv4Addr> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    match sock.local_addr().ok()? {
        SocketAddr::V4(v4) => Some(*v4.ip()),
        _ => None,
    }
}
