# LanPipe Android 构建环境与构建方案

> 实测机器：Windows Server 2022（10.0.20348），仓库 `F:\Projects\lanpipe`。
> 本文所有命令均在 PowerShell 7（`pwsh`）中实测；路径、版本号、错误文本均来自本机实际输出。
> 实测时间：2026-11-28 23:43 ~ 2026-11-29 00:2x（Asia/Shanghai）。

## 0. 结论速览

| 环节 | 状态 | 关键事实 |
| --- | --- | --- |
| JDK 17 | ✅ 可用 | `C:\Program Files\Java\jdk-17`（17.0.12）；Gradle 以它启动并能跑 AGP 9.x |
| Android SDK | ✅ 可用（选定 `F:\Android\android-sdk`） | 10.79 GB，含 build-tools 35/36/37、platforms 35/37.0、platform-tools、cmdline-tools 22.0、system-images x2 |
| cmdline-tools / sdkmanager | ✅ 可用 | `F:\Android\android-sdk\cmdline-tools\latest\bin\sdkmanager.bat`（Pkg.Revision 22.0，会提示 deprecated，仍可用） |
| NDK | ⚠️ 可被 Tauri 接受，版本非官方推荐 | 已装 `30.0.14904198-beta1 (r30-beta1)`；Tauri v2 文档推荐 `27.0.12077973`，未装（决定见 §4.3） |
| Gradle | ✅ 已打通 | 无需系统安装；`gen/android` 的 wrapper 9.6.1，从腾讯镜像下载后 `--version` 通过 |
| Rust Android targets | ✅ 已装（绕过 rustup 镜像 404） | aarch64 / armv7 / i686 / x86_64-linux-android，手工解包 rust-std 到 rustlib |
| `tauri android init` | ✅ 成功 | `--ci --skip-targets-install`，2.5 s，生成 `apps/desktop/src-tauri/gen/android` |
| `tauri android build --apk --debug` | ❌ 卡在**业务代码结构**（非环境问题） | 0.7 分钟即失败：`error: no library targets found in package 'lanpipe'` → 见 §6 |
| 模拟器 | ✅ 硬件加速可用，未启动实例 | WHPX `accel: 0 / WHPX(10.0.20348) is installed and usable`；5 个既有 AVD；`adb devices` 为空 |
| 真机 | ❌ 无 | 需要用户接入 USB / `adb connect` |

## 1. 环境矩阵（实测值）

| 项 | 值 |
| --- | --- |
| OS | Windows Server 2022, 10.0.20348 |
| 磁盘 | `F:` 剩余约 47–50 GB（NDK 27 约 1–2 GB，够用） |
| JDK | `C:\Program Files\Java\jdk-17`，`java -version` = 17.0.12；`JAVA_HOME` 已指向它；PATH 中的 `java` 是 Oracle javapath（也是 17） |
| Node / 包管理 | Node v22.15.1，npm 11.5.1，pnpm 10.11.0 |
| Rust | rustc/cargo 1.100.0（stable-x86_64-pc-windows-msvc），rustup 1.29.3 |
| Rust targets（原装） | x86_64-pc-windows-msvc、x86_64-unknown-linux-musl |
| Tauri | tauri-cli 2.12.5，`tauri` crate 2.12.7 |
| WebView2 | 158.0.4370.45（仅桌面端相关，Android 用系统 WebView） |
| MSVC | VS 2022 Build Tools 已装（桌面端链接用） |
| 模拟器 | emulator 37.1.11，adb 37.0.1 |
| 虚拟化 | Hyper-V 已启用，WHPX 可用（模拟器硬件加速 OK） |

## 2. 两份 Android SDK 的取舍

本机存在两份 SDK，实测对比后**只保留 `F:\Android\android-sdk` 作为唯一 SDK 根**：

| 路径 | 体积 | 内容 | 结论 |
| --- | --- | --- | --- |
| `%LOCALAPPDATA%\Android\Sdk` | 1.58 GB | build-tools 36.0.0、platforms android-35 / android-37.0、platform-tools、emulator；**缺** cmdline-tools、NDK、system-images；`licenses` 为空 | ❌ 不完整，勿用 |
| `F:\Android\android-sdk` | 10.79 GB | build-tools 35.0.0 / 36.0.0 / 37.0.0、platforms android-35 / android-37.0、platform-tools（adb 37.0.1）、**cmdline-tools 22.0**、**ndk 30.0.14904198-beta1**、system-images android-35（google_apis x86_64 + arm64-v8a）、emulator、已接受 licenses、Gradle 缓存 | ✅ 采用 |

两份同时存在且 `%LOCALAPPDATA%` 那份也在 PATH 上时，Gradle/tauri 可能挑错 SDK。**必须显式设置 `ANDROID_HOME`/`ANDROID_SDK_ROOT` 指向 F: 那份**（见 §3.1）。若不再需要 `%LOCALAPPDATA%\Android\Sdk`，建议改名（如 `Sdk.bak`）以免误用；本次未动它。

## 3. 可复制命令序列（从零复现）

### 3.1 环境变量（当前会话 + 持久化）

```powershell
$env:ANDROID_HOME     = 'F:\Android\android-sdk'
$env:ANDROID_SDK_ROOT = 'F:\Android\android-sdk'
$env:JAVA_HOME        = 'C:\Program Files\Java\jdk-17'
$env:NDK_HOME         = 'F:\Android\android-sdk\ndk\30.0.14904198'   # 见 §4.3，若换 27 则改这里
$env:PATH = "$env:JAVA_HOME\bin;$env:ANDROID_HOME\platform-tools;$env:ANDROID_HOME\cmdline-tools\latest\bin;$env:PATH"

# 持久化（机器级，需管理员；也可用 -Scope User）
[Environment]::SetEnvironmentVariable('ANDROID_HOME',     'F:\Android\android-sdk', 'Machine')
[Environment]::SetEnvironmentVariable('ANDROID_SDK_ROOT', 'F:\Android\android-sdk', 'Machine')
[Environment]::SetEnvironmentVariable('NDK_HOME',         'F:\Android\android-sdk\ndk\30.0.14904198', 'Machine')
```

### 3.2 用 sdkmanager 补装组件（已验证成功）

```powershell
$sdk = "$env:ANDROID_HOME\cmdline-tools\latest\bin\sdkmanager.bat"
& $sdk --sdk_root=$env:ANDROID_HOME --licenses            # 全 y
& $sdk --sdk_root=$env:ANDROID_HOME "platforms;android-37.0" "build-tools;37.0.0"
# 如需官方推荐 NDK（见 §4.3）：
# & $sdk --sdk_root=$env:ANDROID_HOME "ndk;27.0.12077973"
```

实测耗时：`platforms;android-37.0` + `build-tools;37.0.0` 从 `dl.google.com` 下载（约 4.7 MB/s）数十秒完成。
注意：sdkmanager 22.0 每次结束会打印 `Warning: sdkmanager is deprecated. Use Android CLI instead.` 并**返回退出码 1**，但安装其实成功——判据是目录/文件是否出现（`platforms\android-37.0\android.jar`、`build-tools\37.0.0\`），不要被退出码误导。

### 3.3 Rust Android targets（rustup 走不通时的手工方案）

正常网络下只需：

```powershell
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
```

本机 `rustup target add` **全部失败**（原因与精确错误见 §5.1），改用「手工下载 rust-std + 解包复制」：

```powershell
$mirror = 'https://rsproxy.cn/dist/2026-11-12'   # 与 rustc 1.100.0 对应的 dist 日期
$tmp    = 'F:\Projects\android-tmp\rust-std'
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
$rustlib = "$(rustc --print sysroot)\lib\rustlib"
foreach ($t in 'aarch64-linux-android','armv7-linux-androideabi','i686-linux-android','x86_64-linux-android') {
  curl.exe -4 -sS -L --max-time 240 -o "$tmp\rust-std-1.100.0-$t.tar.xz" "$mirror/rust-std-1.100.0-$t.tar.xz"
}
python F:\Projects\android-tmp\install_std.py   # 解包并复制到 $rustlib\<target>
```

`install_std.py` 做的事：`tarfile.open(..., "r:xz")` 解包 → 在解包树中定位 `lib/rustlib/<target>` → `copytree` 到 `$(rustc --print sysroot)\lib\rustlib\<target>`。
**不要用 Windows 自带 `tar.exe`**：它是 bsdtar 且本机没有 `xz.exe`，会报
`tar.exe: Error opening archive: Can't initialize filter; unable to run program "xz -d -qq"`。

目标体积（实测）：aarch64 30,242,188 B；x86_64 29,930,256 B；i686 29,597,768 B；armv7 约 27.9 MB。

### 3.4 `tauri android init`（已验证成功）

```powershell
cd F:\Projects\lanpipe
npx --no-install tauri android init --ci --skip-targets-install
```

实测：2.5 s，输出 `Project generated successfully!`，生成 `apps/desktop\src-tauri\gen\android\`（含 `app/`、`buildSrc/`、`gradle/`、`gradlew.bat`、`build.gradle.kts`、`settings.gradle`、`gradle.properties` 等）。日志同时打印
`Info Using installed NDK: F:\Android\android-sdk\ndk\30.0.14904198`。

`gen/` 已是生成物且被忽略：`apps/desktop/src-tauri/gen/` 在 `.gitignore` 中，init 后 `git status` 只显示 `?? docs/mobile/`，**不会污染提交**。

### 3.5 Gradle wrapper 镜像（本机必需）

`services.gradle.org` 的发行包会 307 跳到 `github.com`（本机受限），首次 `gradlew` 会卡住或失败。改 `gen/android/gradle/wrapper/gradle-wrapper.properties`：

```powershell
$p = 'F:\Projects\lanpipe\apps\desktop\src-tauri\gen\android\gradle\wrapper\gradle-wrapper.properties'
Copy-Item $p "$p.bak" -Force
(Get-Content $p) -replace 'distributionUrl=.*',
  'distributionUrl=https\://mirrors.cloud.tencent.com/gradle/gradle-9.6.1-bin.zip' | Set-Content $p
```

⚠️ 转义必须是 `https\://`（单个反斜杠）。写成 `https\\://` 会得到
`Could not load wrapper properties`（properties 文件里 `\\` 是字面反斜杠）。
实测：腾讯镜像 150 MB 约 10 s；`.\gradlew.bat --version --no-daemon` → Gradle 9.6.1（2026-06-26）、Kotlin 2.3.21、Launcher JVM 17.0.12。
可替代镜像（均实测 200）：`mirrors.huaweicloud.com/gradle/...`、`mirrors.aliyun.com/macports/distfiles/gradle/...`。
⚠️ 该修改在 `gen/` 内，**重新 `tauri android init` 会被覆盖**，需要重跑上面这段。

### 3.6 构建

```powershell
cd F:\Projects\lanpipe
npx --no-install tauri android build --apk --debug --target aarch64
```

参数已用 `tauri android build --help` 核对：`--apk`、`--debug`、`-t/--target [aarch64|armv7|i686|x86_64]`、`--ci`、`--ignore-version-mismatches` 均存在。
该命令会先执行 `build.beforeBuildCommand`（前端 `pnpm build` → `dist/`），再调 Gradle + cargo 交叉编译 Rust。

## 4. 生成的 Android 工程参数（供审阅）

来自 `apps/desktop/src-tauri/gen/android/`：

| 项 | 值 |
| --- | --- |
| Gradle | 9.6.1（wrapper） |
| AGP | `com.android.tools.build:gradle:9.3.1` |
| Kotlin | 2.2.10 |
| compileSdk / targetSdk / minSdk | 37 / 37 / 24 |
| applicationId / namespace | `com.lanpipe.desktop` |
| `build.gradle.kts`（root） | 插件版本如上；`app/build.gradle.kts` 末尾 `apply(from = file("tauri.build.gradle.kts"))` |
| `gradle.properties` | `org.gradle.jvmargs=-Xmx2048m`、`configuration-cache=true`、`android.builtInKotlin=false`、`android.newDsl=false`、`android.proguard.failOnMissingFiles=false` |
| AndroidX 依赖 | webkit 1.14.0、appcompat 1.7.1、activity-ktx 1.10.1、material 1.12.0、lifecycle-process 2.10.0 |
| 前端 | `frontendDist = ../../../dist`，`devUrl = http://localhost:1420/` |

### 4.3 NDK 版本决策

- Tauri v2 官方 prerequisites（https://v2.tauri.app/start/prerequisites/）写明 Android 需要 NDK，官方示例/CI 使用 **`27.0.12077973`**；社区 issue（tauri-apps/tauri discussion #14517）报告 **NDK 29 会导致构建失败**，建议回退。
- 本机现状：`ndk;30.0.14904198-beta1`（r30 beta），`tauri android init` 接受它并把路径写进日志/环境。
- 决策：**先用现有 NDK 30 跑一次 build 实测**（成本 0）；若出现 NDK/toolchain 相关错误，再装官方推荐的 27：
  `sdkmanager --sdk_root=... "ndk;27.0.12077973"`（约 1–2 GB，按 4.7 MB/s 估算 5–8 分钟），并把 `NDK_HOME` 指过去、`android/local.properties` 的 `ndk.dir`（若生成）同步。
- 未装 27 的原因：时间预算内优先验证工具链可行性，避免 1–2 GB 下载挤占。

## 5. 失败点与替代方案（精确错误）

### 5.1 rustup target add 失败（镜像 404）

```
error: could not download file from 'https://mirrors.tuna.tsinghua.edu.cn/rustup/dist/2026-11-12/rust-std-1.100.0-aarch64-linux-android.tar.xz' to '...'
http request returned an unsuccessful status code: 404
```

- 根因：本机 rustup 的 dist server 被配成 TUNA 镜像（`RUSTUP_DIST_SERVER` 在 PowerShell 与 cmd 级都设过，均**不生效**；`rustup.exe` 内无 `tuna` 字符串、`settings.toml` 无 mirror 配置），而 TUNA 上该日期目录缺 Android rust-std 组件（或目录结构不同）。
- 替代：手工下载 rust-std 并从 rsproxy/USTC/sjtug 解包（§3.3）。实测 `static.rust-lang.org` HEAD 200 但 `curl` 下载 body 卡在 0 字节；`rsproxy.cn/dist`、`mirrors.ustc.edu.cn/rust-static`、`mirrors.sjtug.sjtu.edu.cn/rust-static` 均 200 可用；`mirrors.aliyun.com/rustup` 该路径 404。

### 5.2 Windows tar 不支持 .tar.xz

```
tar.exe: Error opening archive: Can't initialize filter; unable to run program "xz -d -qq"
```

替代：用仓库外置 Python（`C:\Users\Administrator\.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\python\python.exe`）的 `tarfile` 模块解包（`install_std.py`）。也可装 7-Zip 后 `7z x`。

### 5.3 Gradle 发行包被 github 跳转阻塞

`services.gradle.org/distributions/gradle-9.6.1-bin.zip` → 307 → `github.com/...`（本机受限，零字节/超时）。
替代：腾讯/华为/阿里镜像（§3.5）。同理，`maven.google.com` 页面本身超时，但**不影响构建**：Gradle 的 `google()` 仓库实际解析到 `https://dl.google.com/dl/android/maven2`（实测 AGP pom 200，下载快）。

### 5.4 其它网络事实

| 域名 | 实测 | 用途 |
| --- | --- | --- |
| `dl.google.com` | ✅ ~4.7 MB/s | SDK 组件、Google Maven artifacts |
| `maven.google.com` | ⚠️ 页面超时 | 仅浏览用，构建不依赖 |
| `repo.maven.apache.org` | ✅ | Maven Central |
| `plugins.gradle.org` | ✅ | Gradle 插件门户 |
| `services.gradle.org` | ✅ 索引 / ❌ 发行包跳 github | Gradle wrapper |
| `static.rust-lang.org` | ⚠️ HEAD 200，body 卡死 | rustup dist |
| `mirrors.tuna.tsinghua.edu.cn/rustup` | ❌ 404（Android 组件） | rustup dist 镜像 |
| `rsproxy.cn` / `mirrors.ustc.edu.cn/rust-static` | ✅ | rust-std 替代源 |
| `github.com` | ❌ 受限 | — |
| `ghproxy.net` | ✅（任务前置信息） | github 代理 |

## 6. `tauri android build` 实测结果

### 6.1 环境链路验证通过的部分

| 步骤 | 结果 |
| --- | --- |
| `cargo check -p lanpipe-net --target aarch64-linux-android` | ✅ **成功，25 s**（`lanpipe-proto` + `lanpipe-net` 全部 `Checking`，`Finished dev profile`，exit 0）。Rust 核心已能交叉编译到 Android aarch64，无需额外 NDK 环境变量（`check` 不链接） |
| tauri CLI 读取 NDK | ✅ `Info Using installed NDK: F:\Android\android-sdk\ndk\30.0.14904198` |
| 下载 Android 专属 crate | ✅ `jni 0.22.4`、`rustls-platform-verifier-android 0.2.0`、`simdutf8`、`rustix` 等均下载成功（dl.google/crates 无阻塞） |
| Rust Android std | ✅ 4 个 target 的 `rustc --print target-libdir --target <t>` 均指向已安装的 `...\lib\rustlib\<t>\lib` |
| NDK 工具链完整性 | ✅ `ndk\toolchains\llvm\prebuilt\windows-x86_64\bin` 下有 `aarch64-linux-android{21..35}-clang.cmd` 等全套 wrapper（minSdk 24 → 用 `aarch64-linux-android24-clang.cmd`）；`source.properties` = `Pkg.Revision = 30.0.14904198-beta1`, `Pkg.ReleaseName = r30-beta1` |

### 6.2 失败点（精确错误，40 s 内返回）

```text
npx --no-install tauri android build --apk --debug --target aarch64
...
error: no library targets found in package `lanpipe`
failed to build Android app: `Failed to run `cargo build`: command ["cargo", "build", "--package", "lanpipe",
  "--manifest-path", "F:\\Projects\\lanpipe\\apps\\desktop\\src-tauri\\Cargo.toml",
  "--target", "aarch64-linux-android", "--features", "tauri/custom-protocol tauri/custom-protocol", "--lib"]
  exited with code 101`
BUILD-EXIT=1 DURATION=40s
```

**根因**：`apps/desktop/src-tauri/Cargo.toml` 只定义了 `[[bin]]` 语义（无 `[lib]` 段），且 `src/` 下只有 `main.rs`、`firewall.rs`、`net.rs`、`settings.rs`，**没有 `lib.rs`**。Tauri 移动端要求 app crate 提供 `staticlib`/`cdylib` 库目标（Android 侧由 JNI 加载 `.so`），所以 `cargo build --lib` 直接报 `no library targets found`。
这不是环境问题——Gradle、NDK、Rust std 都还没轮到执行。

### 6.3 需要业务代码改动（本次按约束未改，供 Lead 排期）

为了让 `tauri android build` 进入 Gradle/NDK 阶段，`apps/desktop/src-tauri` 需要做标准 Tauri v2 移动化改造：

1. `Cargo.toml` 增加库目标：
   ```toml
   [lib]
   name = "lanpipe_lib"
   crate-type = ["staticlib", "cdylib", "rlib"]
   ```
2. 新建 `src/lib.rs`：把 `main.rs` 里的 `tauri::Builder` 启动逻辑（约第 250–340 行的 `run()`）搬进去，并加移动端入口：
   ```rust
   #[cfg_attr(mobile, tauri::mobile_entry_point)]
   pub fn run() { /* 原 main 主体 */ }
   ```
3. `src/main.rs` 瘦身为 `fn main() { lanpipe_lib::run() }`（Windows 专属 `#[cfg(windows)]` 代码可留在 lib 内被 cfg 屏蔽）。
4. 移动端不兼容的插件/能力需要 cfg 门控（预期下一轮编译会撞到）：
   - `tauri-plugin-single-instance`（桌面专用，`main.rs:258`）
   - `tauri-plugin-updater`（桌面专用，`main.rs:261`、`main.rs:806`）
   - 托盘：`build_tray`（`main.rs:331`、`416`）与 `tauri` 的 `tray-icon` feature 在 Android 上不可用
   - 大量 `#[cfg(windows)]` 模块（`main.rs:3,463,486,591,603,656,666,675,693,707` 等）保持原样即可，但需确认没有未门控的 Windows-only API 调用留在公共路径上
5. 预期还需：`tauri.conf.json` 的 `bundle.targets` 目前是 `["msi"]`（Android 构建走 `--apk`，通常不冲突，但建议加 `"android"` 或在 CI 用 CLI 覆盖）；`icons/` 只有 Windows `.ico`，Android 图标由 CLI 从 `icons/128x128.png` 生成。

## 7. 模拟器与真机

- 硬件加速：`emulator -accel-check` → `accel: 0` + `WHPX(10.0.20348) is installed and usable.`（Hyper-V 已启用）。
- system-images：`system-images;android-35;google_apis;x86_64`、`...;arm64-v8a` 已装（F: SDK）。
- 既有 AVD（无需再创建）：`lianyu-api35`、`lianyu-api35-arm64`、`lianyu-api35-noaccel`、`lianyu-api35-repro`、`lianyu-api35-tiny`。
- 当前无运行实例：`adb devices` → 空。
- 启动建议（x86_64 镜像 + WHPX）：

```powershell
& "$env:ANDROID_HOME\emulator\emulator.exe" -avd lianyu-api35 -no-snapshot-load -gpu swiftshader_indirect
& "$env:ANDROID_HOME\platform-tools\adb.exe" wait-for-device
& "$env:ANDROID_HOME\platform-tools\adb.exe" install -r <apk>
& "$env:ANDROID_HOME\platform-tools\adb.exe" shell am start -n com.lanpipe.desktop/.MainActivity
```

- 若模拟器起不来，优先接入**真机**（USB 调试或 `adb connect <ip>:5555`），比修模拟器省时。

## 8. 下一步工时估算（以本机现状为起点）

| 工作 | 估算 | 说明 |
| --- | --- | --- |
| 业务代码移动化改造（§6.3：lib target + `mobile_entry_point` + 插件/托盘 cfg 门控） | 1–2 h | 纯结构改造，风险中等（main.rs 约 800 行需拆分/门控） |
| 重跑 `--apk --debug`：首次 Gradle 依赖 + Rust 交叉编译（tauri 依赖树大） | 0.5–1 h | 依赖下载已部分预热，预计更快 |
| 若 NDK 30 在**链接阶段**不兼容 → 装 NDK 27 并重试 | +0.5–1 h | 下载 1–2 GB（5–8 min）+ 重编译；目前尚未走到链接，兼容性未知 |
| 模拟器/真机安装启动验证 | 0.5–1 h | 真机最快；模拟器 WHPX 可用但需调参 |
| release 签名 + AAB | 0.5 h | 需确定 keystore 策略（debug keystore 自动生成） |
| **合计（到首次装到设备/模拟器上跑起来）** | **3–5.5 h** | 其中环境部分已 0 成本（本任务已完成） |

## 9. 本次改动与环境副作用清单

- 新增 `docs/mobile/ANDROID-BUILD.md`（本文）。
- 新增生成物 `apps/desktop/src-tauri/gen/android/**`（已被 `.gitignore` 忽略，允许）。
- 修改 `gen/android/gradle/wrapper/gradle-wrapper.properties` 的 `distributionUrl`（备份 `.bak`，位于同目录，亦被忽略）。
- F: SDK 新增 `platforms/android-37.0`、`build-tools/37.0.0`。
- rust 工具链新增 4 个 `lib/rustlib/*-linux-android*` 目录（手工安装，`rustup target list --installed` **不会**显示它们，因为未写入 rustup 的 target 记录；`cargo --target` 可正常使用）。
- 临时目录 `F:\Projects\android-tmp`（rust-std 包与 `install_std.py`、`android-attempt.log` 构建日志），可随时删。
- 用户环境变量（`HKCU\Environment`，非系统级）新增：`ANDROID_HOME=F:\Android\android-sdk`、`ANDROID_SDK_ROOT=F:\Android\android-sdk`、`NDK_HOME=F:\Android\android-sdk\ndk\30.0.14904198`；新开的终端会自动生效。
- 未修改 `src/`、`crates/`、`apps/` 业务代码；未 git commit/push。§6.3 的改造方案仅写入文档，待排期实施。
