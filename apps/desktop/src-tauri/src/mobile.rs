//! 移动端（Android / iOS）接入点。
//!
//! 桌面端沿用 `lib.rs` 里的手动建窗路径（无边框 + transparent + Mica/Acrylic 材质），
//! 移动端只有一个 Activity 内嵌 WebView 窗口，因此：
//! - 不调用 `transparent(true)` / `decorations(false)`（移动端没有窗口装饰，这些调用会失败）；
//! - 只创建主窗口 `main`，加载内置的 `index.html`；
//! - 配置加载与网络启动依赖 Android 主线程插件调用（`app.path().config_dir()` 等），
//!   放到后台线程执行，避免在 setup（主线程）里阻塞等待主线程回包。
//!
//! 原生能力（权限 / 多播锁 / SAF 文件选择 / 前台服务 / WindowInsets / 返回键）由 Android 侧的
//! `LanPipeNativePlugin`（Kotlin）实现，本文件负责：
//! - 注册该 Kotlin 插件（`android_plugin`）；
//! - 暴露前端约定的 `android_*` 命令（`generate_handler!` 在 `lib.rs` 中注册）；
//! - 轮询 WindowInsets 并以前端监听的 `android:insets` 事件转发。
//!
//! 前端约定的命令：`android_insets`、`android_pick_files`、`android_pick_folder`、
//! `android_open_path`、`android_exit`、`android_foreground_start`、`android_foreground_stop`。
//! 桌面端保留同名占位实现（除 `android_insets` 返回 0 之外都返回错误），这样
//! `generate_handler!` 无需按平台加 `cfg`。

use serde::{Deserialize, Serialize};

/// Android 端 WindowInsets，单位 dp（前端按 1dp ≈ 1 CSS px 使用）。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct AndroidInsets {
    #[serde(default)]
    pub top: f64,
    #[serde(default)]
    pub bottom: f64,
    #[serde(default)]
    pub ime: f64,
}

impl AndroidInsets {
    /// 四舍五入后的整数值，用于判断是否真的变化（避免每 300ms 都向前端发事件）。
    #[cfg_attr(desktop, allow(dead_code))]
    fn key(&self) -> (i32, i32, i32) {
        (
            self.top.round() as i32,
            self.bottom.round() as i32,
            self.ime.round() as i32,
        )
    }
}

/// Kotlin 侧 SAF 命令的返回体：拷贝到 `cacheDir/imports/<ts>/` 后的绝对路径列表。
#[cfg_attr(desktop, allow(dead_code))]
#[derive(Debug, Default, Deserialize)]
struct PathsResponse {
    #[serde(default)]
    paths: Vec<String>,
}

/// 已注册的 Kotlin 原生插件句柄（仅 Android）。
#[cfg(target_os = "android")]
#[derive(Clone)]
pub struct NativeBridge {
    handle: tauri::plugin::PluginHandle<tauri::Wry>,
}

/// 把 Rust 侧日志同时写到 Android logcat（`lanpipe-native` tag）。
///
/// `crate::log` 只写文件，而 Android 上 `std::env::temp_dir()` 指向应用不可写的
/// `/data/local/tmp`，日志会静默丢失；移动端关键路径需要可观测的 logcat 证据。
#[cfg(target_os = "android")]
pub(crate) fn native_log(app: &tauri::AppHandle, msg: &str) {
    use tauri::Manager as _;
    let Some(bridge) = app.try_state::<NativeBridge>() else {
        return;
    };
    let handle = bridge.inner().handle.clone();
    let _ = handle.run_mobile_plugin::<()>("log", serde_json::json!({ "msg": msg }));
}

#[cfg(not(target_os = "android"))]
pub(crate) fn native_log(_app: &tauri::AppHandle, _msg: &str) {}

/// 注册 Android 原生插件：Kotlin 类 `com.lanpipe.desktop.LanPipeNativePlugin`。
#[cfg(target_os = "android")]
fn android_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri::Manager as _;
    tauri::plugin::Builder::new("lanpipe-native")
        .setup(|app, api| {
            let handle = api.register_android_plugin("com.lanpipe.desktop", "LanPipeNativePlugin")?;
            app.manage(NativeBridge { handle });
            crate::log("mobile: LanPipeNativePlugin registered");
            Ok(())
        })
        .build()
}

/// 移动端 Builder 预装配钩子：注册 Android 原生插件（桌面端 no-op，保持签名一致）。
///
/// 桌面端也会编译并调用该函数（no-op），这样 Windows 的 `cargo check` 就能覆盖签名。
pub fn init(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    #[cfg(target_os = "android")]
    let builder = builder.plugin(android_plugin());
    builder
}

/// 移动端 setup：建主窗口 + 后台完成配置加载与网络启动 + 启动 insets 轮询。
#[cfg_attr(desktop, allow(dead_code))]
pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // 移动端由 `tauri::mobile_entry_point` 决定生命周期，不需要桌面那套托盘/关闭收托盘逻辑。
    let _window = tauri::WebviewWindowBuilder::new(
        app,
        "main",
        tauri::WebviewUrl::App("index.html".into()),
    )
    .build()?;

    spawn_insets_watcher(app.handle().clone());

    let handle = app.handle().clone();
    let log_handle = handle.clone();
    std::thread::spawn(move || {
        match crate::settings::bootstrap(&handle, None, 0) {
            Ok(()) => {
                crate::log("mobile setup: settings ok");
                native_log(&log_handle, "mobile setup: settings ok");
            }
            Err(err) => {
                crate::log(&format!("mobile setup: settings failed: {err}"));
                native_log(&log_handle, &format!("mobile setup: settings failed: {err}"));
            }
        }
        native_log(&log_handle, "mobile setup: net start invoked");
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let watchdog_done = done.clone();
        let watchdog_log = log_handle.clone();
        std::thread::spawn(move || {
            for _ in 0..4 {
                std::thread::sleep(std::time::Duration::from_secs(5));
                if watchdog_done.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                native_log(&watchdog_log, "mobile setup: net start still pending");
            }
        });
        let net_handle = handle.clone();
        let net_log = log_handle.clone();
        let net_started = std::time::Instant::now();
        tauri::async_runtime::spawn(async move {
            native_log(&net_log, "mobile setup: net task running");
            match crate::net::start(&net_handle).await {
                Ok(()) => {
                    crate::log("mobile setup: net started");
                    native_log(
                        &net_log,
                        &format!("mobile setup: net started ({:?})", net_started.elapsed()),
                    );
                }
                Err(err) => {
                    crate::log(&format!("mobile setup: net failed: {err}"));
                    native_log(&net_log, &format!("mobile setup: net failed: {err}"));
                }
            }
            done.store(true, std::sync::atomic::Ordering::Relaxed);
        });
    });

    Ok(())
}

/// 后台轮询原生 WindowInsets，并在变化时向前端派发 `android:insets`。
///
/// Tauri 插件的 Kotlin `trigger()` 只会送到插件 `registerListener` 的 Channel，
/// 到不了前端 `@tauri-apps/api/event` 的 `listen()`，因此事件必须由 Rust 侧 emit。
/// 每次轮询是一次 JNI 往返（约几十微秒），300ms 的间隔对电量可以忽略。
#[cfg(target_os = "android")]
fn spawn_insets_watcher(app: tauri::AppHandle) {
    use tauri::{Emitter, Manager};
    std::thread::spawn(move || {
        let mut last = AndroidInsets::default().key();
        let mut failures: u32 = 0;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(300));
            let Some(bridge) = app.try_state::<NativeBridge>() else {
                return;
            };
            let handle = bridge.inner().handle.clone();
            match handle.run_mobile_plugin::<AndroidInsets>("getInsets", ()) {
                Ok(insets) => {
                    failures = 0;
                    if insets.key() != last {
                        last = insets.key();
                        crate::log(&format!(
                            "mobile insets: top={} bottom={} ime={}",
                            last.0, last.1, last.2
                        ));
                        let _ = app.emit("android:insets", insets);
                    }
                }
                Err(err) => {
                    failures += 1;
                    if failures > 40 {
                        crate::log(&format!("mobile insets: polling stopped: {err}"));
                        return;
                    }
                }
            }
        }
    });
}

#[cfg(not(target_os = "android"))]
fn spawn_insets_watcher(app: tauri::AppHandle) {
    let _ = app;
}

// ---------------------------------------------------------------------------
// 前端命令（移动端能力）
// ---------------------------------------------------------------------------

/// 当前 WindowInsets（首帧同步取值，之后靠 `android:insets` 事件更新）。
#[cfg(target_os = "android")]
#[tauri::command]
pub async fn android_insets(app: tauri::AppHandle) -> Result<AndroidInsets, String> {
    use tauri::Manager as _;
    let handle = app.state::<NativeBridge>().inner().handle.clone();
    handle
        .run_mobile_plugin_async::<AndroidInsets>("getInsets", ())
        .await
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
pub async fn android_insets(_app: tauri::AppHandle) -> Result<AndroidInsets, String> {
    Ok(AndroidInsets::default())
}

/// SAF 多选文件：返回拷贝到 `cacheDir/imports/<ts>/` 后的绝对路径。
#[cfg(target_os = "android")]
#[tauri::command]
pub async fn android_pick_files(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    use tauri::Manager as _;
    let handle = app.state::<NativeBridge>().inner().handle.clone();
    let resp = handle
        .run_mobile_plugin_async::<PathsResponse>("pickFiles", serde_json::json!({}))
        .await
        .map_err(|e| e.to_string())?;
    Ok(resp.paths)
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
pub async fn android_pick_files(_app: tauri::AppHandle) -> Result<Vec<String>, String> {
    Err("android_pick_files: 仅在 Android 可用".into())
}

/// SAF 选择目录：把整个目录树拷贝到 `cacheDir/imports/<ts>/<目录名>/` 后返回该目录绝对路径。
#[cfg(target_os = "android")]
#[tauri::command]
pub async fn android_pick_folder(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    use tauri::Manager as _;
    let handle = app.state::<NativeBridge>().inner().handle.clone();
    let resp = handle
        .run_mobile_plugin_async::<PathsResponse>("pickFolder", serde_json::json!({}))
        .await
        .map_err(|e| e.to_string())?;
    Ok(resp.paths)
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
pub async fn android_pick_folder(_app: tauri::AppHandle) -> Result<Vec<String>, String> {
    Err("android_pick_folder: 仅在 Android 可用".into())
}

/// 用系统应用打开接收到的文件 / 目录（`mode` 为 `file` 或 `folder`）。
#[cfg(target_os = "android")]
#[tauri::command]
pub async fn android_open_path(
    app: tauri::AppHandle,
    path: String,
    mode: Option<String>,
) -> Result<(), String> {
    use tauri::Manager as _;
    let handle = app.state::<NativeBridge>().inner().handle.clone();
    handle
        .run_mobile_plugin_async::<()>(
            "openPath",
            serde_json::json!({ "path": path, "mode": mode.unwrap_or_else(|| "file".into()) }),
        )
        .await
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
pub async fn android_open_path(
    _app: tauri::AppHandle,
    _path: String,
    _mode: Option<String>,
) -> Result<(), String> {
    Err("android_open_path: 仅在 Android 可用".into())
}

/// 退出应用（Android 上等价于 finish Activity；延迟 150ms 让前端 promise 有机会 resolve）。
#[cfg(target_os = "android")]
#[tauri::command]
pub async fn android_exit(app: tauri::AppHandle) -> Result<(), String> {
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(150));
        handle.exit(0);
    });
    Ok(())
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
pub async fn android_exit(_app: tauri::AppHandle) -> Result<(), String> {
    Err("android_exit: 仅在 Android 可用".into())
}

/// 启动 dataSync 前台服务（保持传输在后台不被系统冻结），通知栏显示常驻通知。
#[cfg(target_os = "android")]
#[tauri::command]
pub async fn android_foreground_start(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager as _;
    let handle = app.state::<NativeBridge>().inner().handle.clone();
    handle
        .run_mobile_plugin_async::<()>("startForeground", ())
        .await
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
pub async fn android_foreground_start(_app: tauri::AppHandle) -> Result<(), String> {
    Err("android_foreground_start: 仅在 Android 可用".into())
}

/// 停止 dataSync 前台服务。
#[cfg(target_os = "android")]
#[tauri::command]
pub async fn android_foreground_stop(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager as _;
    let handle = app.state::<NativeBridge>().inner().handle.clone();
    handle
        .run_mobile_plugin_async::<()>("stopForeground", ())
        .await
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
pub async fn android_foreground_stop(_app: tauri::AppHandle) -> Result<(), String> {
    Err("android_foreground_stop: 仅在 Android 可用".into())
}
