//! LanPipe 桌面入口：只负责启动核心库；平台差异（窗口/托盘/单实例/更新/CLI）都在
//! `lanpipe_lib` 里按 `cfg(desktop)` / `cfg(mobile)` 隔离，移动端由库里的
//! `tauri::mobile_entry_point` 生成原生入口，不经过本文件。
//!
//! Android 构建时 cargo 仍会编译这个 bin 目标，因此这里只保留与平台无关的一行调用。

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    lanpipe_lib::run();
}
