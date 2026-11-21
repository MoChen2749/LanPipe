//! 设置与身份：config.json 的读写、校验与设备 ID 管理。
//!
//! 约定：UI 只通过命令读写设置，不直接碰配置文件；写入使用临时文件 + rename 保证原子性。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use walkdir::WalkDir;

use lanpipe_proto::{DEFAULT_TCP_PORT, DEFAULT_UDP_PORT};

const CONFIG_VERSION: u32 = 1;

fn default_display_name() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "LanPipe-Device".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    pub config_version: u32,
    pub device_id: String,
    pub display_name: String,
    pub download_dir: String,
    pub listen: ListenConfig,
    pub transfer: TransferConfig,
    pub network: NetworkConfig,
    pub recent_peers: Vec<PeerEntry>,
    pub theme: String,
    pub language: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            config_version: CONFIG_VERSION,
            device_id: uuid::Uuid::new_v4().to_string(),
            display_name: default_display_name(),
            download_dir: String::new(),
            listen: ListenConfig::default(),
            transfer: TransferConfig::default(),
            network: NetworkConfig::default(),
            recent_peers: Vec::new(),
            theme: "system".to_string(),
            language: "zh-CN".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ListenConfig {
    pub udp: u16,
    pub tcp: u16,
}

impl Default for ListenConfig {
    fn default() -> Self {
        Self {
            udp: DEFAULT_UDP_PORT,
            tcp: DEFAULT_TCP_PORT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompressMode {
    #[default]
    Off,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TransferConfig {
    pub streams: u8,
    pub compress: CompressMode,
}

impl Default for TransferConfig {
    fn default() -> Self {
        Self {
            streams: 4,
            compress: CompressMode::Off,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NetworkConfig {
    pub interface: Option<String>,
    pub discovery: bool,
    pub subnet_scan: bool,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            interface: None,
            discovery: true,
            subnet_scan: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerEntry {
    pub addr: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StagedSummary {
    pub files: u64,
    pub bytes: u64,
}

#[derive(Default)]
pub struct SettingsState {
    config: Mutex<AppConfig>,
    path: Mutex<Option<PathBuf>>,
}

impl SettingsState {
    /// 当前生效的配置文件路径（诊断页展示用）。
    pub fn path_string(&self) -> Option<String> {
        self.path
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|path| path.to_string_lossy().to_string()))
    }
}

/// 应用启动时加载配置。优先 `%APPDATA%\LanPipe[-实例]\config.json`；
/// 该位置不可写时（便携模式、受限权限）回退到 exe 同目录的 `config.json`。
/// 损坏的配置会备份为 `config.json.bad` 并回退默认值。
///
/// `instance` 用于同机多实例自测：配置目录加后缀，默认端口整体偏移 `port_offset`。
pub fn bootstrap(
    app: &AppHandle,
    instance: Option<&str>,
    port_offset: u16,
) -> Result<(), String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    let folder = match instance {
        Some(name) if !name.is_empty() => format!("LanPipe-{name}"),
        _ => "LanPipe".to_string(),
    };
    if let Ok(dir) = app.path().config_dir() {
        candidates.push(dir.join(folder).join("config.json"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("config.json"));
        }
    }

    let mut last_err = String::from("找不到可写的配置目录");
    for path in candidates {
        match init_config(app, &path, port_offset) {
            Ok(config) => {
                let state = app.state::<SettingsState>();
                if let Ok(mut slot) = state.config.lock() {
                    *slot = config;
                }
                if let Ok(mut slot) = state.path.lock() {
                    *slot = Some(path);
                }
                return Ok(());
            }
            Err(err) => last_err = format!("{path:?}: {err}"),
        }
    }
    Err(last_err)
}

fn init_config(app: &AppHandle, path: &Path, port_offset: u16) -> Result<AppConfig, String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|err| format!("创建配置目录失败: {err}"))?;
    }

    let mut fresh = false;
    let mut config = match fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<AppConfig>(&text) {
            Ok(cfg) => cfg,
            Err(err) => {
                eprintln!("[lanpipe] config.json 解析失败，回退默认值: {err}");
                let _ = fs::rename(path, path.with_extension("json.bad"));
                fresh = true;
                AppConfig::default()
            }
        },
        Err(_) => {
            fresh = true;
            AppConfig::default()
        }
    };
    if fresh && port_offset > 0 {
        // 只在首次生成配置时偏移 TCP：UDP 发现端口必须与其它实例一致，否则互相收不到公告；
        // 对已存在配置反复加偏移会让端口每次启动都漂移（53321 → 53331 → …）。
        config.listen.tcp = config.listen.tcp.saturating_add(port_offset);
    }

    if config.download_dir.trim().is_empty() {
        config.download_dir = default_download_dir(app)?;
    }
    normalize(&mut config);
    write_atomic(path, &config)?;
    Ok(config)
}

fn default_download_dir(app: &AppHandle) -> Result<String, String> {
    let downloads = app
        .path()
        .download_dir()
        .map_err(|err| format!("获取下载目录失败: {err}"))?;
    Ok(downloads.join("LanPipe").to_string_lossy().to_string())
}

fn normalize(config: &mut AppConfig) {
    config.config_version = CONFIG_VERSION;
    config.display_name = config.display_name.trim().to_string();
    if config.display_name.is_empty() {
        config.display_name = default_display_name();
    }
    if config.display_name.chars().count() > 64 {
        config.display_name = config.display_name.chars().take(64).collect();
    }
    if config.device_id.trim().is_empty() {
        config.device_id = uuid::Uuid::new_v4().to_string();
    }
    config.listen.udp = config.listen.udp.clamp(1024, 65535);
    config.listen.tcp = config.listen.tcp.clamp(1024, 65535);
    config.transfer.streams = config.transfer.streams.clamp(1, 8);
    if config.network.interface.as_deref().map(str::trim) == Some("") {
        config.network.interface = None;
    }
    if config.theme.trim().is_empty() {
        config.theme = "system".to_string();
    }
    if config.language.trim().is_empty() {
        config.language = "zh-CN".to_string();
    }
    config.recent_peers.truncate(5);
}

fn write_atomic(path: &Path, config: &AppConfig) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    let text =
        serde_json::to_string_pretty(config).map_err(|err| format!("序列化配置失败: {err}"))?;
    fs::write(&tmp, text.as_bytes()).map_err(|err| format!("写入临时配置失败: {err}"))?;
    fs::rename(&tmp, path).map_err(|err| format!("替换配置文件失败: {err}"))?;
    Ok(())
}

fn persist(state: &SettingsState, config: &AppConfig) -> Result<(), String> {
    let path = state
        .path
        .lock()
        .map_err(|_| "配置路径锁失败".to_string())?
        .clone()
        .ok_or_else(|| "配置尚未初始化".to_string())?;
    write_atomic(&path, config)
}

fn ensure_writable_dir(path: &str) -> Result<(), String> {
    let dir = PathBuf::from(path.trim());
    if dir.as_os_str().is_empty() {
        return Err("保存目录不能为空".to_string());
    }
    fs::create_dir_all(&dir).map_err(|err| format!("无法创建目录: {err}"))?;
    let probe = dir.join(".lanpipe-write-test");
    fs::write(&probe, b"ok").map_err(|err| format!("目录不可写: {err}"))?;
    let _ = fs::remove_file(&probe);
    Ok(())
}

#[tauri::command]
pub fn get_settings(state: State<'_, SettingsState>) -> Result<AppConfig, String> {
    state
        .config
        .lock()
        .map(|config| config.clone())
        .map_err(|_| "读取配置失败".to_string())
}

#[tauri::command]
pub fn save_settings(
    config: AppConfig,
    state: State<'_, SettingsState>,
) -> Result<AppConfig, String> {
    let mut next = config;
    normalize(&mut next);
    ensure_writable_dir(&next.download_dir)?;
    persist(&state, &next)?;
    *state
        .config
        .lock()
        .map_err(|_| "写入配置失败".to_string())? = next.clone();
    Ok(next)
}

#[tauri::command]
pub fn reset_device_id(state: State<'_, SettingsState>) -> Result<AppConfig, String> {
    let mut next = state
        .config
        .lock()
        .map_err(|_| "读取配置失败".to_string())?
        .clone();
    next.device_id = uuid::Uuid::new_v4().to_string();
    persist(&state, &next)?;
    *state
        .config
        .lock()
        .map_err(|_| "写入配置失败".to_string())? = next.clone();
    Ok(next)
}

#[tauri::command]
pub fn config_location(state: State<'_, SettingsState>) -> Option<String> {
    state
        .path
        .lock()
        .ok()
        .and_then(|path| path.clone())
        .map(|path| path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn check_download_dir(path: String) -> Result<String, String> {
    ensure_writable_dir(&path)?;
    Ok(path)
}

#[tauri::command]
pub fn summarize_paths(paths: Vec<String>) -> Result<StagedSummary, String> {
    let mut summary = StagedSummary { files: 0, bytes: 0 };
    for raw in paths {
        let path = PathBuf::from(raw);
        let meta =
            fs::symlink_metadata(&path).map_err(|err| format!("无法读取 {path:?}: {err}"))?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            for entry in WalkDir::new(&path).follow_links(false) {
                let entry = entry.map_err(|err| format!("遍历失败: {err}"))?;
                let file_type = entry.file_type();
                if file_type.is_dir() || file_type.is_symlink() {
                    continue;
                }
                summary.files += 1;
                summary.bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        } else {
            summary.files += 1;
            summary.bytes += meta.len();
        }
    }
    Ok(summary)
}


/// 供网络层读取当前配置快照。
pub fn snapshot(app: &AppHandle) -> Result<AppConfig, String> {
    let state = app.state::<SettingsState>();
    state
        .config
        .lock()
        .map(|guard| guard.clone())
        .map_err(|_| "配置锁已中毒".to_string())
}
