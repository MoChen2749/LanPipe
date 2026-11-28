# Android 原生桥接验收记录（task-13）

构建/验收时间：2026-11-29。模拟器：`lianyu-api35`（API 35，x86_64，`emulator-5554`，WHPX）。

## 构建

```powershell
$env:CARGO_HOME='F:\Projects\.lanpipe-build\cargo-home'
$env:GRADLE_USER_HOME='F:\Projects\.lanpipe-build\gradle-home'
$env:TEMP='F:\Projects\.lanpipe-build\tmp'; $env:TMP=$env:TEMP
$env:ANDROID_USER_HOME='F:\Projects\.lanpipe-build\android-home'
$env:ANDROID_HOME='F:\Android\android-sdk'; $env:ANDROID_SDK_ROOT=$env:ANDROID_HOME
$env:NDK_HOME='F:\Android\android-sdk\ndk\30.0.14904198'
cd apps/desktop/src-tauri
npx --no-install tauri android build --apk --debug --target aarch64
```

产物：`apps/desktop/src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk`
（205,066,626 B，package `com.lanpipe.desktop`，minSdk 24 / targetSdk 37，native-code arm64-v8a，含 `liblanpipe_lib.so`）。
完整构建日志：`docs/mobile/build-android.log`。

## 组件清单

| 类别 | 内容 |
| --- | --- |
| 权限（AndroidManifest） | `ACCESS_NETWORK_STATE`、`ACCESS_WIFI_STATE`、`CHANGE_WIFI_MULTICAST_STATE`、`POST_NOTIFICATIONS`、`FOREGROUND_SERVICE`、`FOREGROUND_SERVICE_DATA_SYNC`（原有 `INTERNET` 保留） |
| Activity | `.MainActivity`（edge-to-edge、WindowInsets→`android:insets`、返回键→`android:back`、`WifiManager.MulticastLock`、Android 13+ 通知权限请求） |
| Service | `.LanPipeSyncService`（`foregroundServiceType="dataSync"`，通知渠道 `lanpipe_sync`，通知 id 1001） |
| Kotlin 插件 | `com.lanpipe.desktop.LanPipeNativePlugin`（`@TauriPlugin`，Rust 侧 plugin id `lanpipe-native`） |
| Rust | `apps/desktop/src-tauri/src/mobile.rs`（插件注册、命令实现、insets 轮询）；`src/lib.rs` 的 `generate_handler!` 追加 7 个 `mobile::android_*` |

## Tauri 命令

| 命令 | 说明 |
| --- | --- |
| `android_insets` | 返回 `{top, bottom, ime}`（dp），并每 300ms 轮询变化后 emit 事件 `android:insets` |
| `android_pick_files` | `ACTION_OPEN_DOCUMENT` 多选；把 content:// 拷贝到 `cacheDir/imports/<ts>/`，返回绝对路径数组 |
| `android_pick_folder` | `ACTION_OPEN_DOCUMENT_TREE`；递归拷贝目录树到 `cacheDir/imports/<ts>/<名>/`，返回目录绝对路径 |
| `android_open_path` | FileProvider(`<pkg>.fileprovider`) + `ACTION_VIEW`，打开接收到的文件/目录 |
| `android_foreground_start` / `android_foreground_stop` | 启动/停止 `LanPipeSyncService` 前台服务 |
| `android_exit` | 结束当前 Activity（`AppHandle::exit` 在 Android 上等价 finish activity） |

前端接入：`src/api/tauri.ts` 新增 `androidPickFiles` / `androidPickFolder`；`src/actions.ts` 的 `pickFiles()/pickFolder()` 在 Android 分支优先走原生命令（失败回退 dialog 插件）。返回值为 `string[]`。

## 实测证据

### 启动 / 崩溃
```
I lanpipe-native: multicast lock acquired
I lanpipe-native: mobile setup: settings ok
I lanpipe-native: mobile setup: net start invoked
I lanpipe-native: mobile setup: net task running
V Tauri/Plugin: Tauri plugin: pluginId: lanpipe-native, command: getInsets   (每 ~300ms)
```
无 `AndroidRuntime` FATAL / 崩溃。

### 权限（`adb shell dumpsys package com.lanpipe.desktop`）
requested：`POST_NOTIFICATIONS, FOREGROUND_SERVICE, CHANGE_WIFI_MULTICAST_STATE, INTERNET,
FOREGROUND_SERVICE_DATA_SYNC, ACCESS_NETWORK_STATE, ACCESS_WIFI_STATE`，全部 `granted=true`。

### SAF（content:// → cache 绝对路径）
```
I lanpipe-native: pickFiles: launching ACTION_OPEN_DOCUMENT
I lanpipe-native: pickFiles: copied 2/2 file(s) into /data/user/0/com.lanpipe.desktop/cache/imports/1795885629533
```
`run-as com.lanpipe.desktop find cache/imports` → `alpha.txt`、`beta.txt` 均在。界面上「待发送 · 2 项 · 52 B」，条目路径为 cache 绝对路径。

### 前台服务
```
I lanpipe-native: startForeground: dataSync service requested
I lanpipe-native: dataSync foreground service started
```
`dumpsys activity services` → `isForeground=true foregroundId=1001 types=0x00000001`，通知渠道 `lanpipe_sync`，含「停止」action。

### 其他
- `android_insets` → `{"top":49,"bottom":24,"ime":0}`。
- 返回键：从「设置」页按 BACK 回到「发送」页（原生 `onBackPressedDispatcher` → `CustomEvent('android:back')` → 前端 `history.back()`）。
- `android_open_path`：对 cache 内 beta.txt 触发系统 ResolverActivity（模拟器无默认文本查看器）。
- `android_exit`：Activity 结束回到桌面；重新 `am start` 正常。

### 截图
`docs/mobile/android-emulator.png`（1080x2400，发送页，含「待发送 · 2 项 · 52 B」）。

## 已知限制

1. **`net::start` 在模拟器上挂起**：日志停在 `mobile setup: net task running`，之后无 `net started`；`/proc/net/udp`、`/proc/net/tcp` 无该 uid 监听 socket。异步任务已正常调度（非 `block_on` 问题），卡点在 `crate::net::start` 内部（疑为 `lanpipe_net::start_discovery` 的 socket2/bind/multicast join 或 `start_receiver` 的 `TcpListener::bind`）。模拟器为 x86_64 通过 `libndk_translation` 翻译执行 arm64-v8a `.so`，判定为翻译层/模拟器限制，需真机验证；未因此崩溃或阻塞 UI。
2. **SAF 拷贝而非引用**：选中的文件会被复制到应用私有 cache（`cache/imports/<ts>/`），原文件不动；大文件有耗时与空间开销，cache 由系统回收。目录使用 `ACTION_OPEN_DOCUMENT_TREE` 递归拷贝（最大深度 24）。
3. **多选后偶发 picker 不自动关闭**：DocumentsUI 在部分路径下拷贝完成后仍停留（拷贝已完成、结果已回传），可返回键/再次进入恢复。
4. **`android_pick_files`/`android_pick_folder` 命令返回值是 `string[]`**（与 dialog 插件返回 content:// 不同），前端已按此适配；旧的 JS 层 `__TAURI_INTERNALS__.invoke` 劫持方案不可行（属性 `writable:false, configurable:false`），已放弃。
5. 前台服务目前只是骨架（`startForeground` + 通知 + 「停止」action），未真正承载传输任务；`POST_NOTIFICATIONS` 在 Android 13+ 首次启动会请求（模拟器上已 `pm grant`）。
6. 多播锁只在 Activity resume/pause 生命周期内持有；模拟器无真实 Wi-Fi 多播链路，实际局域网发现行为需真机验证。
