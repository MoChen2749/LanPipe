//! LanPipe 无界面 CLI：用于在 Linux/远程机器上做接收端与发送端自测。
//!
//! 用法：
//!   lanpipe-cli recv --dest /tmp/recv [--udp 53310] [--tcp 53311] [--name host] [--id id]
//!   lanpipe-cli send <ip:port> <path>... [--streams 4]
//!
//! 事件以行文本打印，便于脚本/日志比对。

use std::path::PathBuf;
use std::process::exit;
use std::time::{SystemTime, UNIX_EPOCH};

use lanpipe_net::{
    send_paths, start_discovery, start_receiver, DiscoveryEvent, TransferEvent,
};

fn usage() {
    eprintln!(
        "用法:\n  lanpipe-cli recv --dest <目录> [--udp 53310] [--tcp 53311] [--name 名称] [--id ID]\n  lanpipe-cli send <ip:port> <路径>... [--streams N]"
    );
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        usage();
        exit(2);
    };
    let cmd_name = cmd.clone();
    let args_tail: Vec<String> = args[1..].to_vec();
    let worker = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .thread_stack_size(16 * 1024 * 1024)
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(err) => {
                    eprintln!("创建运行时失败: {err}");
                    return 2;
                }
            };
            match cmd_name.as_str() {
                "recv" => rt.block_on(recv(&args_tail)),
                "send" => rt.block_on(send(&args_tail)),
                _ => {
                    usage();
                    2
                }
            }
        })
        .expect("创建 worker 线程失败");
    let code = worker.join().unwrap_or(2);
    exit(code);
}

async fn recv(args: &[String]) -> i32 {
    let Some(dest) = flag(args, "--dest") else {
        eprintln!("recv 需要 --dest");
        return 2;
    };
    let udp: u16 = flag(args, "--udp")
        .and_then(|v| v.parse().ok())
        .unwrap_or(53310);
    let tcp: u16 = flag(args, "--tcp")
        .and_then(|v| v.parse().ok())
        .unwrap_or(53311);
    let name = flag(args, "--name").unwrap_or_else(|| "lanpipe-cli".to_string());
    let id = flag(args, "--id").unwrap_or_else(|| {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("cli-{nanos:x}")
    });

    let (mut disc, mut disc_rx) = match start_discovery(id.clone(), name.clone(), udp, tcp).await {
        Ok(v) => v,
        Err(err) => {
            eprintln!("启动发现失败: {err}");
            return 3;
        }
    };
    let (mut receiver, mut events) = match start_receiver(
        std::net::SocketAddr::from(([0, 0, 0, 0], tcp)),
        PathBuf::from(&dest),
    )
    .await
    {
        Ok(v) => v,
        Err(err) => {
            eprintln!("启动接收端失败: {err}");
            return 3;
        }
    };
    println!(
        "LOCAL id={id} name={name} udp={} tcp={} dest={dest}",
        disc.udp_port,
        receiver.local_addr.port()
    );
    println!("READY");
    loop {
        tokio::select! {
            Some(ev) = disc_rx.recv() => match ev {
                DiscoveryEvent::Up(peer) => println!("PEER-UP id={} name={} addr={}", peer.id, peer.name, peer.addr),
                DiscoveryEvent::Down(id) => println!("PEER-DOWN id={id}"),
            },
            Some(ev) = events.recv() => match ev {
                TransferEvent::Incoming { root_name, files, bytes, .. } => println!("INCOMING root={root_name} files={files} bytes={bytes}"),
                TransferEvent::ReceiveProgress { files_done, files_total, .. } => println!("PROGRESS files={files_done}/{files_total}"),
                TransferEvent::Received { files, bytes, elapsed_ms, .. } => println!("RECEIVED files={files} bytes={bytes} elapsed_ms={elapsed_ms}"),
                TransferEvent::Failed { message, .. } => println!("FAILED {message}"),
                _ => {}
            },
            else => break,
        }
    }
    disc.stop();
    receiver.stop();
    0
}

async fn send(args: &[String]) -> i32 {
    let Some(target) = args.first() else {
        eprintln!("send 需要 <ip:port> 与路径");
        return 2;
    };
    let target = if target.contains(':') {
        target.clone()
    } else {
        format!("{target}:53311")
    };
    let addr: std::net::SocketAddr = match target.parse() {
        Ok(addr) => addr,
        Err(_) => {
            eprintln!("目标地址不合法: {target}");
            return 2;
        }
    };
    let streams: usize = flag(args, "--streams")
        .and_then(|v| v.parse().ok())
        .unwrap_or(4)
        .clamp(1, 16);
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut skip = false;
    for (i, a) in args.iter().enumerate() {
        if skip {
            skip = false;
            continue;
        }
        if a == "--streams" {
            skip = true;
            continue;
        }
        if i == 0 {
            continue;
        }
        paths.push(PathBuf::from(a));
    }
    if paths.is_empty() {
        eprintln!("没有要发送的路径");
        return 2;
    }
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let printer = tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            match ev {
                TransferEvent::SendProgress { files_done, files_total, bytes_done, bytes_total, .. } => {
                    println!("PROGRESS files={files_done}/{files_total} bytes={bytes_done}/{bytes_total}");
                }
                TransferEvent::Sent { files, bytes, elapsed_ms, .. } => {
                    println!("SENT files={files} bytes={bytes} elapsed_ms={elapsed_ms}");
                }
                TransferEvent::Failed { message, .. } => println!("FAILED {message}"),
                _ => {}
            }
        }
    });
    let result = send_paths(addr, paths, streams, tx).await;
    let _ = printer.await;
    match result {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("发送失败: {err}");
            1
        }
    }
}
