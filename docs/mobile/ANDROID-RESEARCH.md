# LanPipe Android 平台调研：Rust on Android 与 Tauri v2 WebView 能力边界

- 调研日期：2026-11-28
- 调研目标：判断 LanPipe（Tauri v2 + Rust 的局域网传输工具）移植到 Android 的可行性，明确工具链版本、WebView 能力边界、必须新增的原生桥接工作与最大风险。
- 证据标记约定：
  - **已验证**：直接阅读官方文档 / 源码 / GitHub API 得到，附 URL。
  - **推测**：基于间接证据或常识推断，未逐页核验，落地前必须实测。
- 项目基线（本次读到的仓库事实）：Tauri `2.12.7`（Rust）/ `@tauri-apps/cli 2.12.5`，`tauri.conf.json` 的 `identifier = "com.lanpipe.desktop"`、`bundle.targets = ["msi"]`，Windows 专属逻辑集中在 `apps/desktop/src-tauri/src/main.rs`，传输核心在 `crates/lanpipe-net`（tokio UDP 发现 + TCP 流），协议在 `crates/lanpipe-proto`。

---

## 0. 一句话结论

**可行，但不是"换 target 重编"就能跑。** 传输内核（tokio + UDP 多播 + TCP）在 Rust 层几乎可以原样复用，真正的工作量在两层：
1. **桌面耦合代码必须做平台隔离**：`main.rs` 里的托盘、无边框透明窗口、Mica/亚克力、DWM 圆角阴影、单实例、自启动、`reveal_path`、`COMPUTERNAME` 设备名、`std::env::temp_dir()` 日志与状态目录，在 Android 上要么不存在、要么直接导致白屏。
2. **必须新写 Android 原生桥接**（Kotlin 插件 + JNI）：Wi-Fi 多播锁、前台服务保活、SAF 文件选择与 `content://` 到本地路径的落地、接收目录、设备名/IP/接口名、返回键与生命周期、通知权限。这部分是主要成本与最大不确定性来源。

按调研任务关心的四个问题，直接回答：

| 问题 | 结论 | 依据 |
| --- | --- | --- |
| 能否复用现有 Rust 传输核心？ | **能，且改动很小**。`crates/lanpipe-net`（tokio、socket2、UDP 广播/多播、TCP 流、xxh64 校验）没有平台专属 API；`lanpipe-proto` 纯逻辑。需要动的只是**数据落地的路径策略**（接收目录、`journal.rs` 状态目录）与**运行时资源参数**（线程栈） | 源码阅读 + Rust Android Tier 2（https://doc.rust-lang.org/rustc/platform-support/android.html） |
| WebView 前端方案是否可行？ | **可行**。同一套 `invoke()` 与前端代码可跑；但毛玻璃/无边框/托盘等桌面视觉在 Android 全失效，需按平台降级；`transparent(true)` 必须去掉（否则白屏） | https://v2.tauri.app/reference/webview-versions/ ；https://github.com/tauri-apps/tauri/issues/9408 |
| 必须新增哪些原生桥接？ | 多播锁、前台服务+通知、SAF 选择与 `content://` 落地、接收目录/导出、生命周期与返回键、设备名/网络信息、权限申请（详见第 4 节） | https://v2.tauri.app/develop/plugins/develop-mobile/ 等 |
| 最大风险是什么？ | ①Android 16 本地网络权限与 Wi-Fi 多播在各 ROM/省电策略下的行为；②后台/Doze/前台服务 6h 限额导致接收端不能真正常驻；③`content://` 与现有 `std::fs` 管线的冲突 | 见第 6 节风险矩阵 |

---

## 1. 工具链与环境版本要求

### 1.1 版本清单（已验证 + 关键推测）

| 组件 | 要求 / 本项目现状 | 证据 |
| --- | --- | --- |
| Rust | stable 即可。本机 `rustc 1.100.0`，Android target 为 **Tier 2**（有 std）。Rust 支持最新 LTS NDK，默认支持该 NDK 覆盖的所有 API level | https://doc.rust-lang.org/rustc/platform-support/android.html |
| Rust targets | 必须 `rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android`（真机 arm64、模拟器 x86_64）。本机目前只装了 windows-msvc 与 linux-musl，**四个 Android target 全缺** | https://v2.tauri.app/start/prerequisites/ |
| JDK | 用 Android Studio 自带 JBR（Windows 默认 `C:\Program Files\Android\Android Studio\jbr`），设 `JAVA_HOME`。本机另有 JDK 17 | https://v2.tauri.app/start/prerequisites/ |
| Android SDK | SDK Manager 安装：Android SDK Platform、Platform-Tools、**NDK (Side by side)**、Build-Tools、Command-line Tools；设 `ANDROID_HOME`、`NDK_HOME` | https://v2.tauri.app/start/prerequisites/ |
| compileSdk / targetSdk | Tauri 当前模板固定 `compileSdk = 37`、`targetSdk = 37` | https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri-cli/templates/mobile/android/app/build.gradle.kts |
| minSdk | Tauri 最低支持 Android 7.0 / SDK 24，可用 `tauri.conf.json > bundle.android.minSdkVersion` 提高（字段默认值 24 来自分发页表述与社区摘要，**推测**：默认 = 24，落地时以生成工程的 `tauri.properties` 为准） | https://v2.tauri.app/distribute/google-play/ ；https://docs.rs/tauri-utils/latest/tauri_utils/config/struct.AndroidConfig.html |
| NDK 版本 | 建议 **r28 / r29 LTS**：① Rust 策略是支持最新 LTS NDK；② Google Play 的 16 KB page size 要求"NDK 28 及以上会自动生成满足要求的包" | https://doc.rust-lang.org/rustc/platform-support/android.html ；https://v2.tauri.app/develop/plugins/develop-mobile/ ；https://developer.android.com/guide/practices/page-sizes |
| Gradle | 由 Tauri 模板生成并固定：**Gradle 9.9.2**（`gradle-9.9.2-bin.zip`） | https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri-cli/templates/mobile/android/gradle/wrapper/gradle-wrapper.properties |
| AGP | 模板固定 **`com.android.tools.build:gradle:9.5.2`**，因此 **AGP 版本不用自己选** | https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri-cli/templates/mobile/android/build.gradle.kts |
| Kotlin | 模板固定 **`kotlin-gradle-plugin:2.2.10`**；Java/Kotlin 编译目标为 **1.8**；AGP 9.x 自身要求较新的 JDK（**推测**：JDK 17+，本机 JDK 17 与 Studio JBR 满足） | 同上 |
| 16 KB page | NDK ≥ 28 自动满足；老 NDK 也可在 `.cargo/config.toml` 加 `[target.aarch64-linux-android] rustflags = ["-C", "link-arg=-Wl,-z,max-page-size=16384"]` | https://v2.tauri.app/develop/plugins/develop-mobile/ |
| cargo-ndk | **非必须**（Tauri 通过 Gradle 插件直接调 cargo）；作为交叉编译/排障备份可选，当前 4.x | https://github.com/bbqsrc/cargo-ndk |

> 结论：**JDK / Gradle / AGP / Kotlin 的版本不需要本项目拍板**——`tauri android init` 会生成整套工程并把版本钉死在模板里。真正需要手动安装并配环境变量的是 **NDK 与 cmdline-tools**，以及四个 Rust target。

### 1.2 Rust on Android 的官方支持现状（背景）

| 项 | 事实 | 证据 |
| --- | --- | --- |
| rustup target | Android 是 **Tier 2** 目标（有 std、无宿主保证），官方 target 列表含 `aarch64-linux-android`、`armv7-linux-androideabi`、`i686-linux-android`、`x86_64-linux-android`、`arm-linux-androideabi`、`thumbv7neon-linux-androideabi`；`riscv64-linux-android` 为 Tier 3 | https://doc.rust-lang.org/rustc/platform-support/android.html |
| NDK 与 rustc 的关系 | Rust 的 Android target 由 rust-lang 提供，**NDK 不提供 rustc**；NDK 提供 sysroot / clang 链接器。Rust 官方策略是"支持最新 LTS NDK，默认支持该 NDK 覆盖的所有 API level" | 同上 |
| cargo-ndk | 第三方 cargo 子命令，负责把 NDK 路径、linker、target 环境变量配好（"handles all the environment configuration needed for successfully building libraries or binaries for Android"），能自动探测最新 NDK。当前 4.0.3。**本项目用 Tauri 时不需要它**（Tauri 的 Gradle 插件自己调 cargo），仅在脱离 Tauri 做交叉编译或排障时有用 | https://crates.io/crates/cargo-ndk/4.0.3 ；https://github.com/bbqsrc/cargo-ndk |
| AOSP 侧 Rust | AOSP 自 Android 12/13 起把 Rust 作为平台语言之一，Soong 支持 `rust_library_rlib` / `rust_ffi`（cdylib/rlib/binaries），并持续扩大"Rust 模块"支持范围（官方页标注多项能力 "Effective in 2026"）。这属于**系统平台构建**，不是 App 开发路径，但说明 Android 生态对 Rust 的支持方向明确 | https://source.android.com/docs/setup/build/rust ；https://source.android.com/docs/setup/build/rust/building-rust-modules/library-modules |
| 对本项目的含义 | `crates/lanpipe-net` / `lanpipe-proto` 是纯 Rust、无 C 依赖，交叉编译到 Android 属于**常规路径**：装 target + NDK 即可，不需要为 Android 单独维护一套传输实现 | 综合上表 |

> 协议提醒：LP/1 用 **UDP 53310 + TCP 53311**（见 `crates/lanpipe-proto` 与 `net.rs`）。在 Android 上绑定/监听端口本身不需要特权，但**能否收到 UDP 广播/多播取决于 MulticastLock**（见 4.1），**能否持续监听取决于前台服务**（见 4.2）。

### 1.3 本机环境差距（用于排期，不替代 CI）

- 已有：JDK 17、`%LOCALAPPDATA%\Android\Sdk`（build-tools 36.0.0、platforms android-35/-37、platform-tools、emulator）。
- 缺：**NDK**、**cmdline-tools**、**system-images**（无模拟器镜像）。
- 需要执行（**推测**，路径按实际 SDK 调整）：
  ```powershell
  rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
  # 安装 NDK (Side by side) 与 cmdline-tools：Android Studio SDK Manager 或 sdkmanager
  $env:ANDROID_HOME = "$env:LOCALAPPDATA\Android\Sdk"
  $env:NDK_HOME   = "$env:ANDROID_HOME\ndk\<版本>"
  $env:JAVA_HOME  = "C:\Program Files\Android\Android Studio\jbr"
  ```
- 生成 Android 工程：`npx tauri android init`；本机 CLI 2.12.5 的子命令已确认存在：`init / dev / build / run`（`android build` 产出 APK 与 AAB）。

---

## 2. 本项目桌面代码的平台耦合清单（迁移必须处理）

以下行号基于当前 `apps/desktop/src-tauri/src/main.rs`（854 行）。

| 位置 | 现状（Windows/桌面） | Android 上的问题 | 处置建议 |
| --- | --- | --- | --- |
| `main.rs:1` | `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` | 无害（`windows_subsystem` 仅 Windows 生效） | 保持，或加 `cfg(windows)` 更明确 |
| `main.rs:3-20` | `#[cfg(windows)] mod firewall;` + 非 Windows 桩（`status()->true` / `is_elevated()->true` / `allow()->Ok(true)`） | Android 会走桩，前端"防火墙状态/放行"页面会显示假数据 | 增加 `mobile` 分支返回"不支持"，前端按平台隐藏入口 |
| `main.rs:33-39` | `log_path()` = `std::env::temp_dir().join("LanPipe")` | Android `temp_dir()` 指向 app cache，**系统可在低存储时清理**；且多进程/多实例语义不同 | 改用 `app.path().app_log_dir()` / `app_data_dir()/logs` |
| `main.rs:221-250` | CLI 模式 `--scan/--send/--instance`（同机多实例端口偏移 0/10/20/30） | Android 无命令行入口；`--instance` 概念无意义 | `#[cfg(desktop)]` 包起来 |
| `main.rs:235-241` | 自建 16MB 栈的 tokio multi-thread runtime + `mem::forget` | 移动端内存更紧，16MB/线程栈偏大 | Android 降栈（如 2–4MB）或保持并压线程数 |
| `main.rs:256-366` | Builder 里注册 `single_instance`、`updater`、`dialog` 插件 | **single-instance / updater 不支持 Android**（插件官方平台表 Android 为 ✗） | `#[cfg(desktop)]` 注册；`Cargo.toml` 用 `[target."cfg(not(any(target_os = \"android\", target_os = \"ios\")))".dependencies]` |
| `main.rs:256-366` | `WebviewWindowBuilder ... .transparent(true).decorations(false).center()` | **`transparent(true)` 在 Android 直接白屏**（官方 issue 已确认；修复后文档仍未声明支持） | Android 不设 transparent / decorations，改全屏常规窗口 |
| `main.rs:256-366` | 窗口 `CloseRequested` → `prevent_close + hide`（收托盘），`build_tray(app)` 托盘菜单 | Android 没有托盘、没有"关窗到托盘"；Activity 生命周期取代窗口 | Android 用 `onPause` 语义，退出走返回键/系统手势 |
| `main.rs:460-521` | autostart 写 `HKCU\...\Run`，非 Windows 返回 `Err("当前平台暂不支持")` | Android 无此概念；`tauri-plugin-autostart` 也不支持 Android | 保持错误返回，前端隐藏；若需开机自启要另写 `BOOT_COMPLETED` 接收器（**推测**，需自研） |
| `main.rs:524-555` | `reveal_path`：explorer / open / xdg-open | Android 无文件管理器路径语义，需 `Intent.ACTION_VIEW` + `FileProvider`，或 `content://` | 新增 Android 实现：落地文件 + FileProvider + Intent |
| `main.rs:558-580` | `webview_runtime()` = `tauri::webview_version()`；`transparency_regressed()` 判断 WebView2 主版本 145/154 | Android 无 WebView2；`tauri::webview_version()` 在 Android 的行为需实测 | 平台隔离；Android 改用 WebView provider 版本（chrome://inspect 或 PackageManager） |
| `main.rs:582-652` | `BackdropState` / `mica_effect` / `backdrop_candidates` / `apply_backdrop`（Mica/Acrylic） | Android 无窗口合成效果；`window_effects` 官方文档未列 Android 支持 | 平台隔离，返回"不支持" |
| `main.rs:666-774` | user32/dwmapi：`WS_THICKFRAME` 圆角阴影、`DWMWA_BORDER_COLOR`/`DWMWA_WINDOW_CORNER_PREFERENCE` | Android 无这些 API | `#[cfg(windows)]` 已包裹则天然安全，确认无交叉引用 |
| `main.rs` `os_build()` | `windows_version::OsVersion::current().build`，非 Windows 返回 0 | 可用，但前端"系统版本"展示会为 0 | Android 返回 `Build.VERSION.SDK_INT`（桥接） |
| `settings.rs` | 配置路径优先 `app.path().config_dir()`，回退 exe 同目录；下载目录 `download_dir()`；设备名读 `COMPUTERNAME` | `COMPUTERNAME` 在 Android 为空 → 设备名空；`config_dir/download_dir` 在 Android 指向 app 私有目录（可用但不直观） | 设备名走 `Settings.Global.DEVICE_NAME` / `Build.MODEL`；下载目录改为 SAF 选择 + 私有目录兜底 |
| `journal.rs:34-53` | 状态目录顺序 `LANPIPE_STATE_DIR` → `APPDATA` → `XDG_STATE_HOME` → `~/.local/state` → `temp_dir()/lanpipe-state` | Android 上全部环境变量缺失，**最终落到 temp_dir，可能被系统清理**（断点续传 manifest 丢失） | 增加 Android 分支：`app.path().app_data_dir()/state` |
| `net.rs:208` | `start_receiver(bind, PathBuf::from(&cfg.download_dir))` | 依赖 `std::fs` 写本地路径，与 `content://` 不兼容 | 接收端必须写 app 私有目录或通过 MediaStore/SAF 落地 |
| `capabilities/default.json` | `windows: ["main"]`，schema 指向 `desktop-schema.json` | 移动端需要 `mobile-schema.json`，并可用 `platforms` 字段限定 | 新建移动端 capability，`"platforms": ["android"]` |
| `tauri.conf.json` | `bundle.targets: ["msi"]`；`plugins.updater` 已配置 | Android 产物是 APK/AAB；**updater 不支持 Android** | 配置 `bundle.android.minSdkVersion` 等；updater 仅桌面注册 |
| `Cargo.toml` 依赖 | `if-addrs`、`walkdir`、`socket2`、`windows-version` | `windows-version` 已有 `cfg(windows)` 保护；`if-addrs`/`socket2` 在 Android 的可用性需实测（**推测**：`if-addrs` 依赖 getifaddrs，NDK 有该符号，通常可用，但接口名/权限不同） | 先在真机跑 `if-addrs` 枚举验证 |

---

## 3. WebView2 与 Android System WebView 的关键差异

Tauri 官方对 Android 的定性（**已验证**，原文"Tauri uses the system Android WebView, which is based on Chromium. Tauri does not bundle a WebView with your app, so the runtime version depends on the device's currently selected WebView provider."）：
https://v2.tauri.app/reference/webview-versions/

| 维度 | Windows（WebView2） | Android（System WebView） | 对本项目影响 |
| --- | --- | --- | --- |
| 内核对齐 | 可自更新，"相对较新的 Chromium 构建" | 随设备/厂商/可选 provider 变化，可能是很老的 Chromium | 前端不能用激进新特性；需按最低 WebView 版本做降级 |
| **透明窗口** | 支持（本项目用于无边框+Mica） | **不支持且有害**：设 `transparent: true` 会白屏；issue 已 closed 但文档仍未把 Android 列为支持 | **必须按平台关掉**（`main.rs` 当前 `.transparent(true)`） |
| **模糊 / 背景效果** | Mica、Acrylic（`window_effects`，Windows 专属） | 无 `window_effects` 支持；CSS `backdrop-filter` 在 Android WebView 上可用性依赖 Chromium 版本，自绘模糊性能差（**推测**，需真机验证） | 视觉层"毛玻璃"需重做或退化为半透明纯色 |
| 无边框/圆角/阴影 | `decorations(false)` + DWM 圆角阴影 workaround | 无 decorations 概念（全屏 Activity + 系统栏）；圆角由系统/主题决定 | 直接去掉窗口装饰相关代码 |
| 文件选择 | 系统对话框返回本地路径 | SAF：`ACTION_GET_CONTENT` 返回 **`content://` URI**（不是路径），dialog 插件 v2.0.0-beta.4 才修复"返回 Content URI 而非文件路径"的 bug；即便如此，官方文档仍说明 Android 上无法直接按路径读取，推荐用 fs 插件 | 发送端"选文件"必须走 SAF 并把 URI 拷贝到私有目录后再交给 `lanpipe-net` |
| 文件系统 | 任意路径（受用户权限约束） | 分区存储：app 私有目录自由读写，共享目录需 MediaStore/SAF | 接收目录策略必须重做 |
| 下载目录 | `download_dir()` 直写 | 无"公共 Downloads 直写"权限，需 MediaStore | 同上 |
| 托盘/单实例 | 有（本项目已用） | 无托盘、无单实例插件支持 | 删除/隔离 |
| 返回键 | 无 | 有；Tauri 2 已合并"Android 返回键事件"（PR 已 merged，默认行为是 `webView.goBack`） | 前端需监听返回键做路由返回，避免误退 |
| 窗口尺寸/多窗口 | 任意 | 单 Activity 居多；`on_new_window` 在 Android 永远拒绝 | 前端别依赖弹窗 |
| 开发调试 | WebView2 DevTools | `chrome://inspect` + USB 调试；`devtools` 字段在 Android 走 chrome://inspect | 调试流程要换 |
| **JS ↔ Rust invoke** | `invoke()` 同一套 API | 同一套 `invoke()` 仍可用（走 Tauri 移动端消息通道），自定义命令无需改写；但**原生命令默认在主线程执行**，耗时操作必须扔协程 | 现有 22 个命令签名可保留，实现按平台分叉 |
| **拖放（drag & drop）** | 窗口级文件拖放（`dragDropEnabled`） | Android 无桌面拖放语义；"拖文件进来"不存在（最多是系统分享 Intent）。**推测**：`dragDropEnabled` 在 Android 无效 | 前端隐藏"拖入文件"，改用 SAF 选择 / 分享 Intent |
| **软键盘** | 无 | WebView 随键盘 `adjustResize`/`adjustPan` 变化，输入框聚焦需自己滚动可见；**推测**：Tauri 不额外处理，靠 Android 默认 `windowSoftInputMode` | 表单页需按 `visualViewport` 适配与滚动兜底 |
| **安全区域 / edge-to-edge** | 无 | Android 15 targetSdk 35 起默认 edge-to-edge，内容延伸到系统栏下，需 `env(safe-area-inset-*)` 或 insets 处理 | 前端布局加 safe-area padding；生成工程的 inset 配置以实际为准（**推测**） |
| 生命周期/节流 | 桌面窗口常在 | Activity 前后台切换；**`background_throttling` 在 Android 不支持**（默认浏览器策略约 5 分钟后节流定时器甚至卸载视图，"pause all tasks until the documents visibility state changes back from hidden to visible"） | 传输进度 UI 依赖的 JS 定时器在后台会停；保活/进度必须下沉到 Rust/前台服务 |
| 自定义协议 | `http(s)://<scheme>.localhost` | 默认也是 `http://<scheme>.localhost`；`use_https_scheme` 可切 https；Android 还可设 `app.androidTauriProtocolHostname`，此时 app 固定从 `https://<host>` 提供 | 资源加载本身没问题；但 release 模板把 `usesCleartextTraffic=false`，涉及明文请求要确认 |
| 安全模型 | 能力系统同套 | 同套，但官方警告："On Linux and Android, Tauri is unable to distinguish between requests from an embedded `<iframe>` and the window itself." | 前端不要引入第三方 iframe |

证据：
- WebView 版本策略：https://v2.tauri.app/reference/webview-versions/
- `transparent` 白屏：https://github.com/tauri-apps/tauri/issues/9408 （state=closed，2024-04-30 关闭；body 原文 "setting transparent to true in the WindowConfig will cause a white screen on android, this is not supposed to do anything on android"）
- `WindowConfig` 平台注记：https://docs.rs/tauri-utils/latest/tauri_utils/config/struct.WindowConfig.html
- 返回键事件：https://github.com/tauri-apps/tauri/pull/14133 （merged 2025-10-15，merge_commit `3397fd9`）
- 能力系统与 iframe 警告：https://v2.tauri.app/security/capabilities/
- dialog 返回 Content URI 的修复记录：https://v2.tauri.app/release/dialog/v2.0.0-beta.4/
- fs 插件对 `content://` 的修复：https://v2.tauri.app/release/fs/v2.7.0/

---

## 4. 必须新增的原生桥接 / 插件工作（工作量主体）

原生插件写法（**已验证**）：Kotlin 类继承 `app.tauri.plugin.Plugin`，类注解 `@TauriPlugin`，方法注解 `@Command`，参数用 `@InvokeArg` + `invoke.parseArgs(...)`；Rust 侧用 `PluginHandle::run_mobile_plugin("xxx", payload)` 调用；事件用 `trigger(...)` 发、前端 `addPluginListener` 收；需要能力项 `<plugin>:default`。
反向（Kotlin → Rust）用 `System.loadLibrary("app_lib")` + `external fun`，Rust 侧 `#[no_mangle] pub extern "system" fn Java_...`，依赖 `jni = "0.21"`。
证据：https://v2.tauri.app/develop/plugins/develop-mobile/

> ⚠️ 官方提醒：原生命令默认跑在 **Android 主线程**，长耗时操作必须放协程（`Dispatchers.IO`），否则 ANR。这对"文件拷贝 / 哈希 / 大目录扫描"是硬约束。

### 4.1 Wi-Fi 多播锁（发现功能的关键）
- 现象：Android Wi-Fi 栈默认**过滤组播/广播包**，不收 `MulticastLock` 会收不到 UDP 发现报文。官方原文："Allows an application to receive Wifi Multicast packets. Normally the Wifi stack filters..."（https://developer.android.com/reference/android/net/wifi/WifiManager.MulticastLock）
- 需要权限 `android.permission.CHANGE_WIFI_MULTICAST_STATE`。
- 实现：acquire 于"发现页进入/传输中"，release 于"离开前台"，避免耗电。**推测**：同时需要 `ACCESS_NETWORK_STATE`、`ACCESS_WIFI_STATE`、`INTERNET`。
- 注意：本项目 `crates/lanpipe-net/src/discovery.rs` 已 `set_broadcast(true)` + `join_multicast_v4`，Rust 侧不用改，缺的就是这个锁。

### 4.2 前台服务保活（传输不能死）
- 长传输必须 `startForeground` + 通知，否则退到后台容易被杀/被冻结。
- Android 14+ 需声明 **foregroundServiceType**（传输场景对应 `dataSync`），并加 `FOREGROUND_SERVICE_DATA_SYNC` 权限；Android 13+ 通知需 `POST_NOTIFICATIONS` 运行时授权。
- 预算限制（官方原文）："The system permits `dataSync` and `mediaProcessing` foreground services to run for a total of 6 hours in a 24-hour period"（https://developer.android.com/develop/background-work/services/fgs/timeout）→ 超大文件/长时间挂机的传输要设计降级与续传提示。
- Android 15 起**后台启动前台服务受限**，违规抛 `ForegroundServiceStartNotAllowedException`；可用 `adb shell am compat enable FGS_SAW_RESTRICTIONS <package>` 测试（https://developer.android.com/about/versions/15/behavior-changes-15）。
- 结论：**"接收监听常驻"在 Android 上不能像桌面那样简单 `thread::spawn`**，要么常驻 FGS（耗电、通知常显），要么接受"仅前台可发现/接收"。

### 4.3 SAF 文件选择与 `content://` 落地
- 第三方选文件返回 `content://` URI；Rust `std::fs` 读不了。项目现在 `src/actions.ts` 用 `@tauri-apps/plugin-dialog` 的 `open()`，Windows 下拿到的是路径，Android 下必须转换成"拷贝到私有缓存后的真实路径"。
- dialog 插件历史上"返回 Content URI 而非文件路径"，v2.0.0-beta.4 修复（https://v2.tauri.app/release/dialog/v2.0.0-beta.4/）；即便修复，官方文档/源码注释仍提示 Android 上路径在 content URI 之后，推荐 fs 插件读取（社区 vendored 源码注释：https://github.com/opengeos/GeoLibre/pull/3218）；另有已知 bug"Android 选择器不返回真实文件名"（https://github.com/tauri-apps/plugins-workspace/issues/3029）。
- 更麻烦的是**授权生命周期**：dialog 的 `open()` 走 `ACTION_GET_CONTENT`，只给临时读权限，进程重启后失效；要跨重启保留必须改用 `ACTION_OPEN_DOCUMENT` + `takePersistableUriPermission`（SAF 官方文档：https://developer.android.com/training/data-storage/shared/documents-files ；社区实践与补丁：https://github.com/opengeos/GeoLibre/pull/3218）。
- 建议方案：**多文件选择后立即用 ContentResolver 流式拷贝到 `cacheDir/outgoing/<uuid>/`，再调用现有 `net::start_send`**；拷完校验大小/哈希，结束时清理。这样传输核心与前端改动最小，代价是双倍 IO 与等待时间——需要进度 UI。

### 4.4 接收目录与共享存储
- Android 10+ 分区存储：app 私有目录可自由写；写入公共 Downloads/媒体库需 MediaStore；用户自定义目录需 SAF tree。官方：https://developer.android.com/training/data-storage ；https://developer.android.com/training/data-storage/shared/documents-files
- **不要用 `MANAGE_EXTERNAL_STORAGE`**：Google Play 对该权限有严格审核政策（https://support.google.com/googleplay/android-developer/answer/10467955），传输工具不构成充分理由，**推测**会被拒。
- 建议默认：`app.path().app_data_dir()/received`，并提供"导出到下载"（MediaStore.Downloads）与"选择文件夹"（SAF tree）；`net.rs` 的 `start_receiver(bind, dest_dir)` 语义可保持不变。

### 4.5 生命周期与后台策略
- `background_throttling` 在 Android 不支持（WindowsConfig 文档明确 "Linux / Windows / Android: Unsupported"）：WebView 隐藏约 5 分钟后定时器会被节流、视图可能被卸载 → **前端轮询/动画进度会停**，传输状态必须以 Rust 为真源，前端回到前台时重新拉取。
- 需要桥接 `onPause/onResume/onStop/onDestroy` 与 `onNewIntent`（插件生命周期回调 `load(webView)`、`onNewIntent(intent)` 已由 Tauri 插件机制提供，见 develop-mobile 文档）。
- Doze / App Standby 会掐断长连接（官方：https://developer.android.com/training/monitoring-device-state/doze-standby）：局域网传输建议"前台可见时才保证在线"，或引导用户加白名单（`REQUEST_IGNORE_BATTERY_OPTIMIZATIONS`，商店政策敏感，**推测**需谨慎）。

### 4.6 网络与设备信息
- 设备名：`COMPUTERNAME` → `Settings.Global.DEVICE_NAME` / `Build.MODEL`（**推测**：部分 ROM 对 `DEVICE_NAME` 读取有差异，需兜底）。
- 本机 IP / 接口名：`if-addrs` 在 Android 的可用性需真机验证（**推测**可用，接口名常见 `wlan0`、热点 `ap0`/`swlan0`）；热点模式下"对方连我"的场景要区分 STA/AP。
- 子网扫描：`net.rs` 的 `/24` 遍历在移动网络/AP 隔离下会大量失败，需超时与并发收敛（现有实现已有超时，但要压并发省电）。

### 4.7 权限、通知与返回键
- 权限清单（**推测**，落地时以真机报错为准）：`INTERNET`、`ACCESS_NETWORK_STATE`、`ACCESS_WIFI_STATE`、`CHANGE_WIFI_MULTICAST_STATE`、`POST_NOTIFICATIONS`（13+）、`FOREGROUND_SERVICE` + `FOREGROUND_SERVICE_DATA_SYNC`（14+）、`NEARBY_WIFI_DEVICES`（13+，若调用 Wi-Fi 相关 API：https://developer.android.com/develop/connectivity/wifi/wifi-permissions）。
- **Android 16 本地网络权限**：官方已引入本地网络访问限制（可用 `adb shell am compat enable RESTRICT_LOCAL_NETWORK <package>` 复现/测试），未来 targetSdk 提升后局域网访问可能需要运行时授权：https://developer.android.com/privacy-and-security/local-network-permission → 这是本项目的**头号平台风险**，需尽早在一个 Android 16 设备上试。
- 返回键：Tauri 2 已提供 Android 返回键事件（PR #14133 merged），前端监听后自定义路由返回；未注册处理时默认 `webView.goBack`。若前端是单页无历史栈，需自行 `exit`（**推测**具体 API 名以 `@tauri-apps/api` 版本为准）。

### 4.8 分发与更新
- Android 不支持 `tauri-plugin-updater`（官方平台表 Android ✗，https://raw.githubusercontent.com/tauri-apps/plugins-workspace/v2/plugins/updater/README.md）；分发走 Google Play（AAB）或侧载 APK + 自建更新提示（`versionCode` 由 `bundle.android.versionCode` 或默认 `major*1e6+minor*1e3+patch` 控制，见 https://docs.rs/tauri-utils/latest/tauri_utils/config/struct.AndroidConfig.html）。
- 单实例/自启动插件同样不支持 Android：https://raw.githubusercontent.com/tauri-apps/plugins-workspace/v2/plugins/single-instance/README.md 、https://raw.githubusercontent.com/tauri-apps/plugins-workspace/v2/plugins/autostart/README.md
- dialog 插件全平台可用：https://raw.githubusercontent.com/tauri-apps/plugins-workspace/v2/plugins/dialog/README.md

---

## 5. 建议的分阶段落地路径

1. **阶段 0（1–2 天）**：装 NDK/cmdline-tools + 四个 Rust target；`tauri android init`；先做"能装能跑空壳"，验证模板 Gradle/AGP 与本机 JDK 17 兼容。
2. **阶段 1（平台隔离，2–3 天）**：`main.rs` 全量 `#[cfg(desktop)]` 化（托盘、transparent、decorations、Mica、DWM、autostart、reveal_path、CLI 模式、single-instance、updater）；capability 拆 mobile；`journal.rs`/`settings.rs` 增加 Android 目录分支；确认编译通过并起 UI。
3. **阶段 2（发现可用，3–5 天）**：写 `LanPipeMobile` Kotlin 插件：MulticastLock + 权限申请 + 设备名/IP；验证 `lanpipe-net` 在 Android 上收得到发现包（真机 + 热点两种网络形态）。
4. **阶段 3（传输可用，5–8 天）**：SAF 选择 → 拷入 cache → 复用 `start_send`；接收写私有目录 + MediaStore 导出；前台服务 + 通知 + 进度；处理返回键/前后台。
5. **阶段 4（打磨与风险验证）**：Android 12/13/14/15/16 真机矩阵、Doze/省电策略、Wi-Fi 多播在各 ROM 的表现、16 KB page、Play 政策与签名。

---

## 6. 风险矩阵

| 风险 | 影响 | 概率 | 验证方式 |
| --- | --- | --- | --- |
| **Android 16 本地网络限制/新权限** | 发现与直连可能被系统阻断，需运行时授权 | 中高 | 在 Android 16 设备上 `am compat enable RESTRICT_LOCAL_NETWORK` 对照测试（官方页） |
| **Wi-Fi 多播在部分 ROM/省电模式下被丢** | 找不到对方，功能不可用 | 高 | 小米/华为/三星/原生各 1 台，热点与路由器两种场景 |
| **后台被杀 / Doze / FGS 6h 限额** | 长传输中断、接收端离线 | 高 | 传输中锁屏 30 分钟、后台 6h 边界测试 |
| **`content://` 与 `std::fs` 不兼容** | 发送端选文件直接失败 | 确定（设计层面） | 按 4.3 方案先做拷贝层 |
| **`transparent(true)` 白屏 / 桌面视觉全部失效** | UI 白屏、进度不可见 | 确定 | 平台隔离首日即可验证（issue #9408） |
| **系统 WebView 版本碎片（旧 Chromium）** | 前端新 API/backdrop-filter 异常 | 中 | 低端机/旧 ROM 上跑 `chrome://inspect` 看版本 |
| **Google Play 政策（MANAGE_EXTERNAL_STORAGE）** | 上架被拒 | 中 | 不使用该权限，走 SAF/MediaStore |
| **移动端内存/性能（16MB 栈、大目录扫描、哈希）** | 卡顿/ANR/OOM | 中 | 主线程零 IO；协程 + 分块；真机 profiler |
| **Tauri 移动端自身 bug** | 例如 transparent 类问题、插件返回 URI 之类 | 中 | 锁定 Tauri CLI 2.12.x 版本，先在空壳上跑关键路径 |

---

## 7. 待验证清单（落地前必须做）

- [ ] `tauri android init` + `android build` 在本机 JDK 17 / NDK r28 下能出 APK（含 16 KB page 检查）。
- [ ] `tauri::webview_version()`、`if-addrs`、`walkdir`、`socket2` 在 Android target 上编译并运行。
- [ ] MulticastLock 前后：发现包接收率对比（同设备同网络）。
- [ ] dialog `open()` 在 Android 返回什么（路径 / `content://`），多选行为与文件名是否正确。
- [ ] `app.path().download_dir()` / `config_dir()` 在 Android 的实际值。
- [ ] WebView 隐藏 5 分钟后前端定时器是否停（`background_throttling` 不可用的实证）。
- [ ] Android 16 上本地网络访问是否被限制、需要什么权限。
- [ ] 前台服务 `dataSync` 的通知与 6h 限制表现。
- [ ] 返回键事件的具体 JS API（`@tauri-apps/api` 2.x）。

---

## 8. 参考资料

Tauri 官方
- 环境准备（Android targets / JDK / NDK / SDK）：https://v2.tauri.app/start/prerequisites/
- CLI（`android init/dev/build`）：https://v2.tauri.app/reference/cli/
- 移动端开发（dev server、真机调试、chrome://inspect）：https://v2.tauri.app/develop/
- WebView 版本策略（系统 WebView，不打包）：https://v2.tauri.app/reference/webview-versions/
- 配置参考：https://v2.tauri.app/reference/config/
- `WindowConfig` 平台注记：https://docs.rs/tauri-utils/latest/tauri_utils/config/struct.WindowConfig.html
- `AndroidConfig`（minSdk/versionCode）：https://docs.rs/tauri-utils/latest/tauri_utils/config/struct.AndroidConfig.html
- Google Play 分发（minSdk 24 起）：https://v2.tauri.app/distribute/google-play/
- 安全/能力系统（iframe 警告、`platforms` 字段）：https://v2.tauri.app/security/capabilities/
- 移动端插件开发（Kotlin 插件、JNI、权限、16 KB page）：https://v2.tauri.app/develop/plugins/develop-mobile/
- Android 工程模板（Gradle 9.9.2 / AGP 9.5.2 / compileSdk 37）：
  - https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri-cli/templates/mobile/android/app/build.gradle.kts
  - https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri-cli/templates/mobile/android/build.gradle.kts
  - https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri-cli/templates/mobile/android/gradle/wrapper/gradle-wrapper.properties

Tauri issue / PR / 插件
- `transparent` 在 Android 白屏：https://github.com/tauri-apps/tauri/issues/9408
- Android 返回键事件：https://github.com/tauri-apps/tauri/pull/14133
- 插件平台支持表：
  - single-instance（Android ✗）：https://raw.githubusercontent.com/tauri-apps/plugins-workspace/v2/plugins/single-instance/README.md
  - autostart（Android ✗）：https://raw.githubusercontent.com/tauri-apps/plugins-workspace/v2/plugins/autostart/README.md
  - updater（Android ✗）：https://raw.githubusercontent.com/tauri-apps/plugins-workspace/v2/plugins/updater/README.md
  - dialog（全平台 ✓）：https://raw.githubusercontent.com/tauri-apps/plugins-workspace/v2/plugins/dialog/README.md
- dialog 返回 Content URI 的修复：https://v2.tauri.app/release/dialog/v2.0.0-beta.4/
- fs 插件与 `content://`：https://v2.tauri.app/release/fs/v2.7.0/
- Android 文件选择器不返回真实文件名：https://github.com/tauri-apps/plugins-workspace/issues/3029
- SAF 持久授权的第三方补丁案例：https://github.com/opengeos/GeoLibre/pull/3218

Rust / 工具链
- Rust Android 平台支持（Tier 2、LTS NDK 策略）：https://doc.rust-lang.org/rustc/platform-support/android.html
- cargo-ndk：https://github.com/bbqsrc/cargo-ndk ；https://crates.io/crates/cargo-ndk/4.0.3
- AOSP Rust（平台侧，非 App 侧）：https://source.android.com/docs/setup/build/rust ；https://source.android.com/docs/setup/build/rust/building-rust-modules/library-modules

Android 官方
- `MulticastLock`：https://developer.android.com/reference/android/net/wifi/WifiManager.MulticastLock
- `CHANGE_WIFI_MULTICAST_STATE`：https://developer.android.com/reference/android/Manifest.permission#CHANGE_WIFI_MULTICAST_STATE
- 本地网络权限（Android 16）：https://developer.android.com/privacy-and-security/local-network-permission
- 前台服务 6 小时限制（dataSync）：https://developer.android.com/develop/background-work/services/fgs/timeout
- 前台服务类型：https://developer.android.com/develop/background-work/services/fgs/service-types
- Android 15 行为变更（FGS 后台启动限制、edge-to-edge）：https://developer.android.com/about/versions/15/behavior-changes-15
- SAF / 文档访问：https://developer.android.com/training/data-storage/shared/documents-files
- 数据存储总览（分区存储）：https://developer.android.com/training/data-storage
- Doze / App Standby：https://developer.android.com/training/monitoring-device-state/doze-standby
- Wi-Fi 权限（NEARBY_WIFI_DEVICES）：https://developer.android.com/develop/connectivity/wifi/wifi-permissions
- 16 KB page size：https://developer.android.com/guide/practices/page-sizes
- Play 的 MANAGE_EXTERNAL_STORAGE 政策：https://support.google.com/googleplay/android-developer/answer/10467955
