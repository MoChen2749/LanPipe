#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod net;
mod settings;

use std::io::Write;

use lanpipe_net::{send_paths, TransferEvent};

static LOG_FILE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn log_path() -> std::path::PathBuf {
    let name = LOG_FILE
        .get()
        .map(String::as_str)
        .unwrap_or("lanpipe.log");
    std::env::temp_dir().join("LanPipe").join(name)
}

pub(crate) fn log(msg: &str) {
    let path = log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(file, "{msg}");
    }
}

/// 同机多实例自测：`LanPipe.exe --instance b`
fn instance_name() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--instance" {
            return iter.next().cloned().filter(|s| !s.is_empty());
        }
        if let Some(rest) = arg.strip_prefix("--instance=") {
            return Some(rest.to_string()).filter(|s| !s.is_empty());
        }
    }
    None
}

/// 实例对应的默认端口偏移（间隔 10，避免同机冲突）。
/// `--scan <尾号>`：扫描本机各网段里最后一段等于该值的设备（无界面自测用）。
fn cli_scan_args() -> Option<u16> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let pos = args.iter().position(|a| a == "--scan")?;
    args.get(pos + 1)?.parse::<u16>().ok()
}

fn run_cli_scan(octet: u16) -> i32 {
    if octet > 255 {
        eprintln!("尾号必须在 0-255 之间（可输 5 或 005）");
        return 2;
    }
    let mut targets = net::scan_candidates(octet as u8, lanpipe_proto::DEFAULT_TCP_PORT);
    targets.extend(net::scan_candidates(
        octet as u8,
        lanpipe_proto::DEFAULT_TCP_PORT,
    ));
    targets.sort();
    targets.dedup();
    let short = format!("{octet:03}");
    println!(
        "扫描本机所有子网 *.{}:{}（{} 个候选，短 ID #{short}）",
        octet,
        lanpipe_proto::DEFAULT_TCP_PORT,
        targets.len()
    );
    let mut found = 0;
    for target in targets {
        let addr = std::net::SocketAddr::V4(target);
        if std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(700))
            .is_ok()
        {
            println!("found {addr} shortId=#{short}");
            found += 1;
        }
    }
    println!("共找到 {found} 台设备");
    0
}

fn port_offset(instance: Option<&str>) -> u16 {
    match instance {
        None => 0,
        Some(name) => match name.to_ascii_lowercase().as_str() {
            "a" => 0,
            "b" => 10,
            "c" => 20,
            "d" => 30,
            other => 10 * (((other.len() as u16) % 8) + 1),
        },
    }
}

/// 无界面发送模式（自测/脚本用）：
/// `LanPipe.exe --send 192.168.1.5:53311 <路径...> [--streams 4]`
fn cli_send_args() -> Option<(String, Vec<String>, usize)> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut target: Option<String> = None;
    let mut streams = 4usize;
    let mut paths = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--send" => {
                i += 1;
                target = args.get(i).cloned();
            }
            "--streams" => {
                i += 1;
                if let Some(v) = args.get(i).and_then(|s| s.parse::<usize>().ok()) {
                    streams = v.clamp(1, 8);
                }
            }
            other if other.starts_with("--") => {}
            other => paths.push(other.to_string()),
        }
        i += 1;
    }
    let target = target?;
    if paths.is_empty() {
        return None;
    }
    Some((target, paths, streams))
}

fn run_cli_send(target: &str, paths: &[String], streams: usize) -> i32 {
    log(&format!("cli-send start target={target} streams={streams} paths={paths:?}"));
    let target = target.to_string();
    let paths: Vec<std::path::PathBuf> = paths.iter().map(std::path::PathBuf::from).collect();
    // 大目录（几万文件）的清单序列化/反序列化需要更深的栈，放到独立大栈线程里跑。
    let worker = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || -> i32 {
            let addr: std::net::SocketAddr = match target.parse() {
                Ok(addr) => addr,
                Err(err) => {
                    log(&format!("cli-send bad target: {err}"));
                    return 2;
                }
            };
            let started = std::time::Instant::now();
            let result = tauri::async_runtime::block_on(async move {
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<TransferEvent>();
                let drain = tokio::spawn(async move {
                    while let Some(ev) = rx.recv().await {
                        match &ev {
                            TransferEvent::SendProgress {
                                files_done,
                                files_total,
                                bytes_done,
                                bytes_total,
                                ..
                            } => log(&format!(
                                "cli-progress files={files_done}/{files_total} bytes={bytes_done}/{bytes_total}"
                            )),
                            TransferEvent::Sent {
                                files,
                                bytes,
                                elapsed_ms,
                                ..
                            } => log(&format!(
                                "cli-sent files={files} bytes={bytes} elapsed_ms={elapsed_ms}"
                            )),
                            TransferEvent::Failed { message, .. } => {
                                log(&format!("cli-failed {message}"))
                            }
                            _ => {}
                        }
                    }
                });
                let res = send_paths(addr, paths, streams, tx).await;
                let _ = drain.await;
                res
            });
            let elapsed = started.elapsed();
            match result {
                Ok(()) => {
                    log(&format!("cli-send ok in {:.3}s", elapsed.as_secs_f64()));
                    0
                }
                Err(err) => {
                    log(&format!("cli-send error: {err}"));
                    1
                }
            }
        })
        .expect("创建发送线程失败");
    worker.join().unwrap_or(1)
}

fn main() {
    let cli = cli_send_args();
    let _ = LOG_FILE.set(if cli.is_some() {
        "cli-send.log".to_string()
    } else {
        "lanpipe.log".to_string()
    });
    log(&format!("LanPipe {} start", env!("CARGO_PKG_VERSION")));
    std::panic::set_hook(Box::new(|info| {
        log(&format!("PANIC: {info}"));
        log(&format!("{:?}", std::backtrace::Backtrace::force_capture()));
    }));

    // 大目录的清单可能几万条，统一提供更大的运行时栈（接收端解析 Offer 也受益）。
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .expect("创建 tokio 运行时失败");
    tauri::async_runtime::set(runtime.handle().clone());
    std::mem::forget(runtime);

    if let Some(octet) = cli_scan_args() {
        std::process::exit(run_cli_scan(octet));
    }

    if let Some((target, paths, streams)) = cli {
        let code = run_cli_send(&target, &paths, streams);
        std::process::exit(code);
    }

    let instance = instance_name();
    let offset = port_offset(instance.as_deref());
    log(&format!("instance={instance:?} port_offset={offset}"));

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(settings::SettingsState::default())
        .manage(net::NetState::default())
        .setup(move |app| {
            log("setup: enter");
            match settings::bootstrap(app.handle(), instance.as_deref(), offset) {
                Ok(()) => log("setup: settings ok"),
                Err(err) => log(&format!("setup: settings failed: {err}")),
            }

            match tauri::async_runtime::block_on(net::start(app.handle())) {
                Ok(()) => log("setup: net started"),
                Err(err) => log(&format!("setup: net failed: {err}")),
            }

            let title = match instance.as_deref() {
                Some(name) => format!("LanPipe ({name})"),
                None => "LanPipe".to_string(),
            };
            let window = tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title(title)
            .inner_size(1040.0, 760.0)
            .min_inner_size(820.0, 560.0)
            .center()
            .on_page_load(|_window, payload| {
                log(&format!("page_load {:?} {}", payload.event(), payload.url()));
            })
            .build();

            match window {
                Ok(window) => log(&format!(
                    "setup: window built, url={:?}, visible={:?}",
                    window.url(),
                    window.is_visible()
                )),
                Err(err) => log(&format!("setup: window build FAILED: {err}")),
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            settings::get_settings,
            settings::save_settings,
            settings::reset_device_id,
            settings::config_location,
            settings::check_download_dir,
            settings::summarize_paths,
            net::list_peers,
            net::scan_last_octet,
            net::local_info,
            net::restart_net,
            net::start_send,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run LanPipe");
}

