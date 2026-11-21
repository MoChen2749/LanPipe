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
            let mut builder = tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title(title)
            .inner_size(1040.0, 760.0)
            .min_inner_size(820.0, 560.0)
            .transparent(true)
            // 自绘标题栏（WinUI 风格）：去掉系统标题栏，Mica/Acrylic 才能铺满整个窗口。
            .decorations(false)
            .center()
            .on_page_load(|_window, payload| {
                log(&format!("page_load {:?} {}", payload.event(), payload.url()));
            });

            // 仅用于自动化验收：设置 LANPIPE_CDP=<port> 时开放 DevTools 协议端口。
            if let Ok(port) = std::env::var("LANPIPE_CDP") {
                if !port.is_empty() {
                    builder = builder
                        .additional_browser_args(&format!(
                            "--remote-debugging-port={port} --remote-allow-origins=http://127.0.0.1:{port}"
                        ));
                }
            }

            let window = builder.build();

            match window {
                Ok(window) => {
                    tune_window_frame(&window);
                    // 老系统（无 DWM 圆角）在尺寸变化后重算窗口圆角区域
                    let region_window = window.clone();
                    window.on_window_event(move |event| {
                        if matches!(
                            event,
                            tauri::WindowEvent::Resized(_) | tauri::WindowEvent::ScaleFactorChanged { .. }
                        ) {
                            apply_round_region(&region_window);
                        }
                    });
                    let backdrop = apply_backdrop(&window);
                    log(&format!(
                        "setup: window built, url={:?}, visible={:?}, backdrop={backdrop}",
                        window.url(),
                        window.is_visible()
                    ));
                    let _ = tauri::Manager::manage(app, BackdropState(std::sync::Mutex::new(backdrop.to_string())));
                }
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
            reveal_path,
            backdrop_info,
            window_shadow,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run LanPipe");
}


/// 在系统文件管理器中显示一个路径（接收目录、已接收文件）。
#[tauri::command]
fn reveal_path(path: String) -> Result<(), String> {
    let target = std::path::PathBuf::from(&path);
    if !target.exists() {
        return Err(format!("路径不存在: {path}"));
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // 0x08000000 = CREATE_NO_WINDOW，避免弹出命令行窗口。
        std::process::Command::new("explorer")
            .arg(&path)
            .creation_flags(0x0800_0000)
            .spawn()
            .map_err(|err| format!("打开失败: {err}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|err| format!("打开失败: {err}"))?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|err| format!("打开失败: {err}"))?;
    }
    Ok(())
}

/// 当前窗口实际生效的背景材质（mica / acrylic / none）。
pub struct BackdropState(pub std::sync::Mutex<String>);

/// 当前系统支持的材质列表（按优先级）。
///
/// 规则来自微软文档：
/// - Mica 需要 Windows 11（build >= 22000），且不支持时系统只回退纯色；
/// - Acrylic 需要 Windows 10 1809（build >= 17763）以上；
/// - 不支持的一律用纯色，避免出现"透明窗口直接看到桌面"的问题。
fn backdrop_candidates() -> Vec<(&'static str, tauri::window::Effect)> {
    use tauri::window::Effect;
    let forced = std::env::var("LANPIPE_BACKDROP").unwrap_or_default();
    match forced.as_str() {
        "none" => return Vec::new(),
        "mica" => return vec![("mica", Effect::Mica)],
        "acrylic" => return vec![("acrylic", Effect::Acrylic)],
        _ => {}
    }
    let build = windows_version::OsVersion::current().build;
    log(&format!("backdrop: os build {build}"));
    if build >= 22000 {
        vec![("mica", Effect::Mica), ("acrylic", Effect::Acrylic)]
    } else if build >= 17763 {
        vec![("acrylic", Effect::Acrylic)]
    } else {
        Vec::new()
    }
}

/// 尝试给窗口应用系统材质，返回实际生效的材质名。
fn apply_backdrop(window: &tauri::WebviewWindow) -> &'static str {
    #[cfg(target_os = "windows")]
    {
        use tauri::window::EffectsBuilder;
        for (name, effect) in backdrop_candidates() {
            let config = EffectsBuilder::new().effect(effect).build();
            match window.set_effects(Some(config)) {
                Ok(()) => {
                    log(&format!("backdrop: {name} applied"));
                    return name;
                }
                Err(err) => log(&format!("backdrop: {name} failed: {err}")),
            }
        }
        log("backdrop: no material, use solid background");
    }
    let _ = window;
    "none"
}

#[repr(C)]
struct DwmMargins {
    cx_left_width: i32,
    cx_right_width: i32,
    cy_top_height: i32,
    cy_bottom_height: i32,
}

#[link(name = "dwmapi")]
extern "system" {
    fn DwmExtendFrameIntoClientArea(hwnd: isize, margins: *const DwmMargins) -> i32;

    fn DwmGetWindowAttribute(
        hwnd: isize,
        attr: u32,
        value: *mut core::ffi::c_void,
        size: u32,
    ) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateRoundRectRgn(
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        width_ellipse: i32,
        height_ellipse: i32,
    ) -> isize;
}

#[link(name = "user32")]
extern "system" {
    fn SetWindowRgn(hwnd: isize, rgn: isize, redraw: i32) -> i32;

    fn DwmSetWindowAttribute(
        hwnd: isize,
        attr: u32,
        value: *const core::ffi::c_void,
        size: u32,
    ) -> i32;
}

/// 调窗口外框：去掉无边框窗口那圈生硬的 1px 黑边（保留柔和阴影），Win11 用圆角。
fn tune_window_frame(window: &tauri::WebviewWindow) {
    const DWMWA_BORDER_COLOR: u32 = 34;
    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWCP_ROUND: u32 = 2;
    const DWMWA_COLOR_NONE: u32 = 0xFFFF_FFFE;

    let Ok(hwnd) = window.hwnd() else { return };
    let hwnd = hwnd.0 as isize;
    unsafe {
        // 无边框窗口必须把 DWM 边框向内扩 1px，系统才会绘制柔和阴影；
        // 边框颜色随后设为"无"，避免出现那圈生硬的 1px 黑/白线。
        let margins = DwmMargins {
            cx_left_width: 1,
            cx_right_width: 1,
            cy_top_height: 1,
            cy_bottom_height: 1,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        let none = DWMWA_COLOR_NONE;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &none as *const u32 as *const core::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        );
        let round = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &round as *const u32 as *const core::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        );
    }
    apply_round_region(window);
}

/// Win10/Server（<22000）没有 DWM 圆角，用窗口区域自己裁圆角，保证三端视觉一致。
fn apply_round_region(window: &tauri::WebviewWindow) {
    if windows_version::OsVersion::current().build >= 22000 {
        return;
    }
    let Ok(hwnd) = window.hwnd() else { return };
    let hwnd = hwnd.0 as isize;
    let maximized = window.is_maximized().unwrap_or(false);
    if maximized {
        // 最大化时去掉区域，避免边缘漏出
        unsafe {
            let _ = SetWindowRgn(hwnd, 0, 1);
        }
        return;
    }
    let Ok(size) = window.inner_size() else { return };
    let (w, h) = (size.width as i32, size.height as i32);
    if w <= 0 || h <= 0 {
        return;
    }
    unsafe {
        let rgn = CreateRoundRectRgn(0, 0, w + 1, h + 1, 16, 16);
        let _ = SetWindowRgn(hwnd, rgn, 1);
    }
}

/// 当前环境是否使用系统窗口阴影（Win11 为 true）。
#[tauri::command]
fn window_shadow() -> bool {
    windows_version::OsVersion::current().build >= 22000
}

/// 前端启动后读取，用于启用半透明层级样式。
#[tauri::command]
fn backdrop_info(state: tauri::State<'_, BackdropState>) -> String {
    state
        .0
        .lock()
        .map(|value| value.clone())
        .unwrap_or_else(|_| "none".to_string())
}
