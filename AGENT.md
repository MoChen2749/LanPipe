# AGENT.md — LanPipe 项目方向（定稿 v0.2）

> **本文件是本项目唯一的方向性文档（source of truth）。** 任何 agent / 协作者动工前先读它；
> 方向、选型、协议、验收标准的变更**必须先改本文件，再改代码**。
> 本文件只回答"做什么、怎么定位、做到什么程度"，不讨论备选路线。
>
> - 定稿日期：2026-11-18
> - 状态：方向已定，M0 demo 待实现
> - 前期调研依据：`F:\Projects\lan-transfer-research\`（调研报告 + 零磁盘补充报告 + 三轮本机实测）

---

## 1. 一句话

**LanPipe = 局域网"拖进去 → 点设备 → 传完"的游戏/大目录传输工具**：接收端零额外磁盘、海量小文件尽量跑满带宽、单 exe、无控制台窗口、跨平台。

## 2. 背景与硬约束（来自前期实测，不再重复验证）

| 事实 | 数据/证据 | 对产品的约束 |
|---|---|---|
| 归档再解包 = 2x 磁盘 | 370.8 MiB 目录暂存峰值 732–747 MiB；sendme 源码级 `ExportMode::Copy` 2x | **禁止**任何 zip/tar staging；接收端只允许每文件 `.part` |
| 单流建文件上限 ≈600–700 文件/s | 本机 tar 管道 3202 文件 5.08s | **必须多流**（4–8 条）才能跑满 |
| 多流可到 2,000–2,700 文件/s | 4 流 1.95k、8 流 2.2–2.7k 文件/s | 默认 4 条，可配 1–8 |
| 1GbE ≈ 90–120 MiB/s；HDD ≈ 125 MiB/s | 本机 RAW 写 125.6 MiB/s | 性能验收以 1GbE 跑满为准，2.5G 视接收盘 |
| 压缩对游戏目录无益 | gzip-1 使 5.0s → 16.8–18.8s | 压缩默认关闭，仅按文件类型可选 |

## 3. 目标 / 非目标

**v1 目标**
- 拖入文件或整个目录 → 局域网内其他开着 LanPipe 的设备自动出现 → 点击即发送
- 接收端有「接受/拒绝」确认，进度可视，可取消
- 接收端**峰值额外磁盘 ≈ 0**（每文件 `.part` → fsync → rename）
- 1GbE 下尽量跑满；数万小文件不退化
- 文件级续传：中断重传跳过已完成文件
- Windows 单 exe 可运行（前端资源内嵌），配置独立；**不出现 cmd/控制台窗口**
- **设置能力从 M0 起就必须可用**：接收保存目录、设备名 / 设备 ID、IP 直连发送（见 §7.1）

**非目标（v1 不做）**
- 公网穿透 / 中继服务器 / 账号体系 / 云同步
- 与 Syncthing 式的持续双向同步
- 以系统共享或外部命令行进程作为传输实现（传输只走本项目协议）
- 桌面三端优先：Windows / macOS / Linux（Tauri v2 同一套前端，按平台做适配）
- 移动端（Android/iOS）放到 M3 之后评估，不作为近期目标
- **不做 LocalSend 协议兼容**（2026-11-20 明确决定，至少当前阶段不做；握手/端口/清单都不向 LocalSend 对齐）

## 4. 技术选型（已定，改动需先改本节）

| 层 | 选择 | 理由 |
|---|---|---|
| 语言 | **Rust 2021** | 零 GC、网络/IO 性能、单二进制、跨平台 |
| UI 壳 | **Tauri v2** | 前端资源内嵌进 exe；无 Node 运行时；`windows_subsystem` 无控制台；跨 Win/macOS/Linux |
| 前端 | **TypeScript + Vite**（M0 不用框架） | 起步快；UI 上限高，便于后续实现 WinUI3 视觉规范；状态复杂后再引入 SolidJS |
| 网络 | **tokio** + 手写 LP/1 帧协议（TCP）+ UDP 广播发现 | 可控、可调多流/背压/续传 |
| 校验 | `crc32fast`（帧）+ `xxhash-rust`（文件） | 低 CPU，足够 LAN 场景 |
| 压缩 | `zstd`（可选，默认关） | 只对明确的文本类小文件开启 |
| 打包 | `tauri build`，Windows 产出**单 exe**（assets 内嵌） | 满足"单可执行文件"；配置文件放 `%APPDATA%\LanPipe\config.json` |
| 无控制台 | `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` | 双击不弹 cmd |

**框架备选与否决理由**

| 方案 | 否决/保留理由 |
|---|---|
| **egui / eframe** | UI 上限与 WinUI3 视觉还原弱（自绘控件成本高）。**保留为"零 WebView 依赖"备胎** |
| **Slint** | 外部文件拖拽/复杂列表生态待验证，M0 风险高 |
| **Makepad** | 生态/文档不成熟，表单与调试成本高，不适合 1–2 天出 demo |
| **Flutter + flutter_rust_bridge** | 体积大、桥接复杂、非单 exe |
| **Electron / Go+Wails** | 资源占用高 / 带 Go 后端，均不符"资源少 + 纯单 exe" |
| **C# + WinUI3** | 仅 Windows，违反源码级跨端 |
| MyGO（用户提到） | 未检索到成熟 Rust 原生 UI 框架；按 Makepad/egui 一类评估后未选 |

> 结论：**Tauri v2 是"UI 上限 × 单 exe × 跨平台 × 资源"当前最优折中**。代价是 Windows 依赖系统 WebView2（Win11 自带；Win10 绝大多数已装，缺了给引导安装）。

## 4.1 运行时依赖策略

**原则：传输本身不依赖任何外部程序**，所有能力要么静态编入 exe，要么是操作系统自带组件。

| 依赖类型 | 例子 | 策略 |
|---|---|---|
| 系统运行时 | **WebView2** | 启动检测；缺失时引导安装。Tauri `WebviewInstallMode`：`downloadBootstrapper`（默认，小包联网装）、`embedBootstrapper`（内置引导器）、`offlineInstaller`（内置完整包，+100~200MB）、`skip`。**不把运行时塞进 exe** |
| 可静态编译的库 | rustls、zstd、xxhash、crc32 | **编进二进制**，无 sidecar、无 PATH |
| 外部程序 | — | **不作为依赖**；若未来确有需要，只允许"exe 同目录 sidecar"或"用户手选绝对路径 + 本地配置" |

**永不写系统 PATH**（需要管理员、影响全局、卸载易残留）。

## 4.2 权限、防火墙与"一键可用"

目标：**普通用户双击即用**，不提权也能完成完整传输；需要提权的配置集中成一个按钮。

| 事项 | 设计 |
|---|---|
| 监听端口 | 53310 / 53311（均 >1024）→ **无需管理员**；WebView2 也不需要 |
| Windows 防火墙 | 首次运行检测入站规则；提供**"一键放行"**（UAC 提权执行 `netsh advfirewall firewall add rule`，只对 **Private** 配置文件放行 TCP 53311 + UDP 53310）；用户拒绝提权则回退图文手动步骤 + **端口连通性自检** |
| 网络位置 | 检测到 Public 网络时提示切换 Private（提示，不静默改系统设置） |
| 写目标目录 | 默认 `%USERPROFILE%\Downloads\LanPipe`，可改；权限失败按文件上报，不请求管理员 |
| 端口冲突 | 自动换端口并写入 announce 报文，不写死 |
| 卸载 | 配置保留/删除可选；防火墙规则要么自动删、要么提示用户 |

## 5. 架构

```
┌─────────────────────────────┐
│  UI (TypeScript + HTML/CSS) │  Tauri WebView
└──────────────┬──────────────┘
               │ Tauri IPC（commands / events）
┌──────────────▼──────────────┐
│  Rust core (apps/desktop)   │
│  ├─ settings    config.json │  目录/ID/流数/网卡
│  ├─ discovery   UDP 53310   │  announce / 直连
│  ├─ control     TCP 53311   │  JSON 信令
│  ├─ transfer    TCP 53311   │  k 条数据流，LP/1 帧
│  └─ writer      .part→rename│
└─────────────────────────────┘
```

**crate 划分（Cargo workspace）**
- `crates/lanpipe-proto`：帧编解码、manifest、错误类型（无 IO，可单测）
- `crates/lanpipe-net`：发现、会话、发送/接收引擎（tokio）
- `apps/desktop/src-tauri`：Tauri 命令、事件、配置读写、UI 胶水
- `apps/desktop/src`：前端（含设置页）

## 6. 协议 LP/1（v1 草案，实现以 `docs/protocol.md` 为准）

**发现**：UDP 多播 + 受限广播，每 2s 一次
`{"t":"announce","v":1,"id":"<uuid>","name":"DESKTOP-X","os":"windows","port":53311,"cap":["lp/1"]}`；退出时 `{"t":"bye","id":...}`。
UI 以 5s 无心跳判离线。**手动 IP 直连**作为兜底（不经发现，直接 `HELLO`）。

**控制面**：TCP 53311，`u32 LE 长度 + JSON`。
`HELLO → ACCEPT{token} → MANIFEST → RESUMED{skip[]} → DATA… → FILE_DONE → DONE`

**数据帧**（LP/1）：
```
magic "LPK1"(4) | type u8 | flags u8 | stream u16 | len u32 | payload
DATA payload: pathId u32 | offset u64 | crc32c u32 | bytes
```
- DATA 块 1–4 MiB；小文件可合帧（≤4 MiB 或 ≤64 个文件）减少 syscall
- 接收端按 `pathId` 找 manifest 条目，**直接写目标路径 + `.lp-part`**

**manifest**：`{root, total, files:[{id, path, size, mtime, mode, xxh64?}]}`；`treeHash` 用于识别目录变化
**落盘**：`target/<path>.lp-part` → 校验 → `fsync` → `rename`；目录按需建；保留 mtime
**续传**：文件级——`size + mtime + xxh64` 一致则 skip；会话断后重连重新 MANIFEST
**校验**：帧 CRC32C；文件 xxh64（默认 <64 MiB 开启，大文件可配）
**压缩**：默认关；per-file flag，仅 <1 MiB 且熵低的文件用 zstd-1
**安全**：M0 明文 + 接收端显式确认；M1 换 rustls 自签证书 + 指纹 TOFU（局域网工具的常见做法）
**并行**：默认 4 条流（可 1–8）；大文件独占流，小文件 round-robin

### 6.1 发现可靠性

LocalSend 用 53317 多播发现，已知失败原因：AP 隔离（客户端隔离）、防火墙拦入站、多网卡/VPN 绑定错误、多播被禁、子网不同、休眠、端口占用等（参考 [LocalSend Network & Discovery Problems](https://deepwiki.com/localsend/localsend/8.1-network-and-discovery-problems)）。LanPipe 采取**四层兜底**：

1. **双通道广播**：每块活动网卡上同时发 **UDP 多播**（`239.255.42.99:53310`）与**受限广播**（`255.255.255.255:53310`），2s 一次；**接收端也主动 announce**（双向），解决单侧被过滤。
2. **主动单播扫描**：对本机路由表每个 /24 段发探测（限速、可关），覆盖"广播/多播被禁但单播可达"的网络。
3. **手动兜底**：手动 IP、二维码、6 位配对码；最近在线设备缓存 10 分钟。
4. **诊断页**：网卡/子网、监听端口、防火墙规则状态、最近 20 条发现报文、对端可达性测试，一键复制。

其他：默认绑定全部接口（UI 可指定网卡）；IPv6 link-local 作为补充；announce 带实例 ID + 启动时间，避免旧缓存；5s 无心跳判离线。

## 7. UI / 交互

> M2 设计草案见 [`docs/UI-DESIGN.md`](docs/UI-DESIGN.md)（WinUI 3 / Fluent 2，模块化 + i18n XML）；线框草图见 `docs/ui/wireframe-*.png`。

- 主窗 900×600，可调；深/浅色跟随系统
- 交互流：**拖入** → 文件/目录摘要（数量/总大小）→ 设备列表（名称/平台/在线）→ 点击设备 → *接收端弹确认* → 双端进度 → 完成/失败
- 常驻入口：**设置**（齿轮）、**IP 直连**、**接收目录**（可点击直接换）、**诊断**
- 错误态：同名冲突、空间不足、对端拒绝、连接断开（可重试）
- 视觉按 Fluent 2 token（accent `#005FB8`/`#4CC2FF`、8px 圆角、Segoe UI Variable、48px 标题栏）；禁用 emoji 图标，统一内联 SVG

### 7.1 设置项（M0 起必须可用）

"第一阶段只做功能"≠"没有设置"。以下设置 M0 就要能改、能存、能生效：

| 设置项 | 默认值 | 行为要求 | M0 |
|---|---|---|---|
| **接收保存目录** | `%USERPROFILE%\Downloads\LanPipe` | 系统目录选择对话框；选择后立即生效；写入前做可写性检查；空间不足时提示 | ✅ |
| **设备显示名** | 主机名 | 可编辑并保存；保存后下一次 announce 立即用新名字（无需重启） | ✅ |
| **设备 ID** | 首次启动生成 UUIDv4 | **用于快捷识别对端**：列表与设置页展示「设备名 + 短 ID（UUID 前 8 位）」；可复制完整 ID；提供"重置 ID"（二次确认，提示会让旧对端缓存失效） | ✅ |
| **IP 直连发送** | 无 | 输入 `IP` 或 `IP:端口` → 直接连接发送，**完全不依赖发现**；验证格式/端口范围；最近 5 条历史可点选 | ✅ |
| 监听端口 | UDP 53310 / TCP 53311 | 可改；冲突自动递增并持久化；改后重启监听 | ✅（用户确认提前到 M0） |
| 并发流数 | 4 | 1–8 滑杆；大文件自动独占流 | ✅（用户确认提前到 M0） |
| 压缩 | 关 | 开/关；开启时只压小文本类文件 | M1 |
| 网卡选择 | 全部接口 | 下拉列出活动网卡（名称/IP/速率）；切换后重建发现套接字 | M1 |
| 发现开关 / 单播扫描 | 开 / 开 | 可关（关闭后仍可用 IP 直连） | M1 |
| 防火墙一键放行 | — | 检测规则 → 提权添加 → 结果显示 | M1 |
| 诊断页 | — | 网卡/子网/端口/规则/报文/可达性测试 | M1 |
| 主题 | 跟随系统 | 浅/深/跟随 | M1 |
| 语言 | 中文 | 中/英 | M2 |
| 托盘 / 开机自启 | 关 | 开关 | M2 |

**配置存储**：`%APPDATA%\LanPipe\config.json`，原子写（`config.json.tmp` → rename）；解析失败回退默认并备份坏文件；**不含任何路径猜测/外部程序路径**。字段草案：

```json
{
  "deviceId": "uuid-v4",
  "displayName": "DESKTOP-X",
  "downloadDir": "C:\\Users\\<user>\\Downloads\\LanPipe",
  "listen": { "udp": 53310, "tcp": 53311 },
  "transfer": { "streams": 4, "compress": "off" },
  "network": { "interface": null, "discovery": true, "subnetScan": true },
  "recentPeers": [{ "addr": "192.168.1.5:53311", "name": "LAPTOP-Y" }],
  "theme": "system",
  "language": "zh-CN"
}
```

## 8. 目录结构（约定）

```
lanpipe/
├─ AGENT.md                  # 本文件（方向定稿）
├─ README.md
├─ docs/
│  ├─ protocol.md            # LP/1 协议细节（随代码更新）
│  └─ decisions/             # 重大决策记录（每条一个 md）
├─ crates/
│  ├─ lanpipe-proto/
│  └─ lanpipe-net/
└─ apps/desktop/
   ├─ src/                   # 前端（含 settings 页）
   └─ src-tauri/             # Rust/Tauri
```

## 9. 里程碑

| 阶段 | 内容 | 预估 |
|---|---|---|
| **M0（当前）** | 单机双实例 demo：拖拽 → 发现（多播/广播 + 手动 IP 兜底）→ 发送 → 进度 → 落盘校验；**设置页（接收目录 / 设备名 / 设备 ID / IP 直连 / 监听端口 / 并发流数）**；不做加密/续传/美化 | 1–2 天 |
| M1 ✅ | 跨机双端（Windows ↔ WSL Ubuntu）、1x 磁盘、25 GB 实测、纠错重传、**断点续传 L1（journal + 跳过已完成）** | 已完成 |
| M1（剩余） | 真实物理机 1GbE 复测、kill -9/篡改续传测试、journal TTL、小文件合批、防火墙一键放行 + 诊断页 | 进行中 |
| M2（后续） | TLS/TOFU、历史记录持久化、托盘、开机自启、macOS/Linux 构建 | 待定 |
| M2（进行中） | GUI 重写：WinUI 3 / Fluent 2、组件化（单文件 ≤300 行）、i18n XML、深浅色、桌面三端 + 窄屏/移动端预留 | 进行中 |
| M3 | 打包/签名/自动更新；移动端（Android/iOS）评估 | 待定 |

## 10. 验收标准（M1 结束须全部满足）

- **功能**：拖拽文件/目录、自动发现、接受/拒绝、进度、取消、失败可重试、落盘文件数与字节完全一致
- **设置**：目录/设备名/ID/IP 直连可改可存；重启后保持；**关闭发现后 IP 直连仍能发送**；改目录立即生效
- **零额外磁盘**：接收端峰值额外磁盘 ≤ `最大单文件 + 8 MiB`（实测采样）
- **性能**：1GbE 混合目录 ≥ 100 MiB/s（大文件主导场景）；1 万小文件 ≥ 1,800 文件/s
- **资源**：空闲 RAM < 150 MB；单 exe 双击无控制台窗口
- **平台**：Win10 1903+/Win11（WebView2）、macOS 12+、Ubuntu 22.04+（WebKitGTK）
- **代码**：`cargo clippy -- -D warnings` 通过；proto/net 有单元测试

## 11. 风险与对策

| 风险 | 对策 |
|---|---|
| 目标机缺 WebView2 | 启动检测 → 引导安装 Evergreen Runtime（`downloadBootstrapper`/`embedBootstrapper`）；或切 egui 备胎 |
| Windows 防火墙拦截 | 首次运行检测规则 → "一键放行"（UAC 提权，仅 Private）；失败回退手动图文 + 端口自检 |
| AP 隔离 / 多播被禁 / VPN 多网卡 | 多播+广播+单播扫描+手动 IP 四层兜底；诊断页显示网卡/子网/规则/报文 |
| 一键放行需要管理员权限 | 只在"一键放行"按钮处提权；拒绝提权不影响本机回环测试，只影响跨机 |
| 长路径 >260 | Rust 侧统一 `\\?\` 前缀（`dunce`/自封装） |
| 大小写冲突（Windows 不敏感） | manifest 预检重名，发送前报错 |
| 配置损坏/迁移 | 原子写 + 坏文件备份 + 缺字段用默认值；版本字段 `configVersion` 预留 |
| Defender 扫描拖慢写入 | 文档建议把目标目录加排除；UI 给提示 |
| 符号链接/权限 | M0 跳过符号链接并在 UI 提示；权限错误按文件上报 |
| 磁盘写满 | 接收前按 manifest 预检空间并留余量 |

## 12. 开发约定

- 方向/选型/协议/验收变更：先改 AGENT.md，再改代码
- 协议实现与 `docs/protocol.md` 同步；不兼容变更升 `LP/2`
- 无遥测；日志仅本地；不上传任何设备信息
- Rust：`rustfmt` + `clippy -D warnings`；前端 M1 起加 prettier/eslint
- 标识符英文、注释中文可
- 单实例运行（`tauri-plugin-single-instance`）
- 设置项读写集中在 `settings` 模块，**UI 不直接改 config 文件**
- 生产构建必须 `cargo build --release --features custom-protocol`（缺少该 feature 会进入 dev 模式、加载 devUrl 导致白屏）；开发模式不带
- 本开发机沙箱限制：工作区内 `F:\Projects` 的 exe 只能写工作区路径，须从 `%LOCALAPPDATA%\Programs\LanPipe\LanPipe.exe` 运行（`build.bat` 自动部署）

## 13. 待确认（不阻塞 M0）

1. 产品名 **LanPipe** 是否保留（可随时改，仅影响 exe 名/端口文档）
2. GUI 重写的范围：只换视觉，还是同时做组件库/多窗口/国际化？（当前决定：先做视觉+组件化+桌面三端适配）
3. 是否需要**托盘常驻 / 开机自启**（影响生命周期与 M2 排期）
4. 设备 ID 重置后，历史 `recentPeers` 是否需要一并清空

## 14. M0 实现记录（2026-11-19）

**已实现**：UDP 发现（多播 239.255.42.99 + 受限广播，2s 公告/5s 超时）、LP/1 控制会话（Offer/Accept/DATA/FileDone/Done）、N 流并行传输（1–8，默认 4）、零暂存落盘（`.lp-part` → rename）、GUI 全流程、设置持久化、CLI 自测模式（`--send <ip:port> <paths> [--streams N]`）、同机多实例（`--instance b` → `%APPDATA%\LanPipe-b` + 端口 +10）。

**关键决定（与初版设计不同，需记住）**：

1. **接收端并发模型**：不用全局 `Mutex<Session>` 包住写入（会把 IO 串行化）。改为 `Arc<Session>` + `files_done/bytes_done: AtomicU64` + `open: StdMutex<HashMap<u32, Arc<Mutex<PartState>>>>`，每个文件独立锁。实测 903 文件 3.81s → 1.42s（2.7×）。
2. **不逐文件 `sync_all`**：只 flush + 原子 rename，与 tar/robocopy 基线口径一致。牺牲"单文件立即持久化"，换吞吐；掉电一致性由 M1 的 checkpoint/续传机制承担。
3. **逐文件 `FileReceived` 事件不转发到前端**：万级文件时每次 IPC+JSON 序列化会显著拖慢；只发节流后的 `ReceiveProgress/SendProgress`（8 MiB 或 200ms）与最终 `Received/Sent`。
4. **CLI 与 GUI 日志分离**：`%TEMP%\LanPipe\cli-send.log` vs `lanpipe.log`，避免 CLI 截断 GUI 日志。
5. **发送端 30s 等 Accept、Done 后接收端最多等 5s 追平**，不完整则 `Failed`。

**实测（loopback，已预热）**：903 文件/63.9 MiB → 4 流 1.30s（51.4 MiB/s、693 文件/s）；3×20 MiB → 4 流 0.213s（295 MiB/s）。tar 管道基线小文件上限 2,598 文件/s，差距在逐文件建文件/rename 与帧开销，M1 做小文件合批。
6. **同机双实例发现不互通（M0 已知限制）**：UDP 发现端口 = 监听端口，多实例端口偏移后公告发往各自端口，互相收不到。跨机默认端口正常；同机自测走 IP 直连。M1 方案：固定发现端口 + 单播扫描兜底。

### §14.1 2026-11-19 修复与纠错（M0 内）

- 0 字节文件：接收端 `finish_empty_file`（无 PartState 时按 manifest size==0 建空文件 + 计数 + FileReceived）。
- 校验纠错：`Verify{hashes}` / `Resync{ids}` / `Complete`；发送端每文件 xxHash64，最多 3 轮按文件重传；接收端 `resync()` 删除已落地文件并回退计数。
- 发现：`build_iface_senders` 每网卡独立 socket（`set_multicast_if_v4`）+ 子网定向广播；`Announce.addrs` + `pick_peer_addr`（优先默认路由 /24）；net.rs 每 2s 重发 `peers:update`。
- 同机双实例：`--instance` 只偏移 TCP 且仅首次生成配置时偏移。
- 集成测试：`e2e_directory_transfer_zero_staging`、`corrupted_hash_triggers_resync_and_recovers` 均通过。

### 14.2 跨机验证（2026-11-19）

- 对端：WSL2 Ubuntu 24.04 + `lanpipe-cli`（`apps/cli`）。双向发现、双向传输、坏哈希纠错、0 字节文件、488 文件/510 MiB 均已实测通过。
- Windows 防火墙入站规则必须限定 `-RemoteAddress LocalSubnet`（TCP/UDP 53310-53311、53320-53321）；不要对 `Any` 全开。
- 发现层的地址选择：`Announce` 携带本机全部 IPv4，接收端用 `pick_peer_addr` 优先选与默认路由网卡同 /24 的地址，避免多网卡/VPN 下取到不可达地址。
- `RECEIVED` 事件已用 `AtomicBool` 去重（finish_file 与 Done 两处都会触发完成判定）。
- 待办（M1）：真实两台物理机 1GbE 吞吐复测；WSL/虚拟机测试只能验证功能，不能代表 LAN 上限。

### 14.3 稳健性修复（2026-11-19 晚）

- `MAX_CONTROL_FRAME = 64 MiB`（proto），DATA 帧仍校验 ≤ `MAX_DATA_FRAME`；接收端 `handle_conn` 与发送端 Verify/Complete 读取均用控制帧上限。
- keepalive 30s/10s；DATA 写超时 180s。
- 大栈：CLI main 64 MiB 线程 + tokio 16 MiB worker；GUI 同样把发送放到 64 MiB 栈线程，并在 `main()` 先把 tokio runtime 的 worker 栈设为 16 MiB 再交给 Tauri。
- `tune_stream` 教训：曾经因批量替换写出 `fn tune_stream(stream) { tune_stream(&stream); }` 无限递归，导致任何连接一建立就爆栈；现在函数体固定为 `set_nodelay(true)` + keepalive，不要再被脚本替换。
- 接收端目录延迟创建；单连接错误仅结束该连接（`eprintln!` 记录，不再发全局 Failed），靠 Verify/Resync 修复。

## 15. GUI 重写与多端适配（M2，2026-11-20 起）

### 15.1 决策

- **不做 LocalSend 协议兼容**：握手、端口（53310/53311）、清单格式全部保持 LP/1 自有协议；未来若做互操作，另立独立模块，不污染主协议。
- **视觉基线**：Fluent Design / WinUI 3（颜色、圆角、层级、字体、动效），在 Windows 上贴近系统原生观感；macOS/Linux 沿用同一套 token，仅做字体/间距/快捷键的平台微调。
- **技术**：Tauri v2 + TypeScript/Vite，前端不引重框架（保持单 exe 小体积、启动快）；以「设计 token + 组件函数 + 页面模块」组织，后续可平滑替换为任意框架。

### 15.2 设计 token（`src/styles.css` 的 `:root`）

| 类别 | token | 值（浅色 / 深色） |
| --- | --- | --- |
| 主色 | `--accent` | `#005FB8` / `#4CC2FF` |
| 页面底 | `--bg` | `#F3F3F3` / `#1F1F1F` |
| 卡片 | `--surface` | `#FFFFFF` / `#2B2B2B` |
| 描边 | `--stroke` | `rgba(0,0,0,.08)` / `rgba(255,255,255,.10)` |
| 文本 | `--text` / `--text-2` | `#1B1B1B` / `#F5F5F5`；`#5F5F5F` / `#B8B8B8` |
| 圆角 | `--radius` | 8px（卡片）/ 6px（控件） |
| 间距 | 4/8/12/16/24 | 统一 4 的倍数 |
| 字体 | `--font` | `"Segoe UI Variable", "Segoe UI", system-ui, -apple-system, "Noto Sans CJK SC", sans-serif` |
| 动效 | `--ease` | `cubic-bezier(.2,.8,.2,1)`，尊重 `prefers-reduced-motion` |

### 15.3 布局与组件

- 顶部：应用图标 + 名称 + 本机「设备名 / 短 ID」胶囊。
- 左列（主操作）：「发送」拖放区（支持点击选择）→ 已选内容摘要 → 「附近设备」列表（刷新按钮 + 设备卡，含首字母头像/名称/地址/发送按钮）→ 「IP 直连」（完整 IP 或尾号扫描）→ 「当前传输」（进度、速度、文件数、取消）。
- 右列（设置页，分节）：常规（显示名、保存目录）、网络（UDP/TCP 端口、并发流数、压缩策略）、关于（设备 ID / 复制 / 重置、版本、诊断折叠区）。
- 状态栏：最近一次操作结果 + 版本号。
- 组件清单：`Card`、`Field`、`Button`（primary/ghost/danger）、`DeviceItem`、`DropZone`、`Progress`、`Toast`/`StatusBar`、`Segmented`（主题：跟随系统/浅色/深色）。
- 空态/错误态必须有文字说明；所有操作有禁用态与加载态。

### 15.4 多端适配

| 平台 | 适配点 |
| --- | --- |
| Windows | Segoe UI Variable；标题栏原生；键盘 `Ctrl+,` 打开设置；拖放路径用 `onDragDropEvent` |
| macOS | 系统字体 `-apple-system`、更宽松的左右内边距、`Cmd+,`；窗口红绿灯区域留白 |
| Linux | 字体回退 Noto Sans/DejaVu；不用系统标题栏装饰假设；HiDPI 用 CSS 像素 |
| 窄窗口 | `< 900px` 单列：设置收进可折叠区；拖放区与设备列表保持可用 |
| 通用 | 深浅色跟随系统 + 手动覆盖；键盘可达（Tab/Enter/Esc）；高对比度 `prefers-contrast`；`prefers-reduced-motion` 关闭动画 |

### 15.5 验收

- 三端窗口 1040×760 默认、最小 820×560 不出现横向滚动/裁切。
- 深浅色切换即时生效且持久化（`config.theme`）。
- 拖入文件夹后设备列表可点击发送；接收进度/完成在「当前传输」卡片展示；错误有可读文案。
- 设置项保存后立即生效；重启保持。

### 14.4 断点续传 L1（2026-11-20）

- `crates/lanpipe-net/src/journal.rs`：`manifest_digest`（元数据稳定摘要）、journal 读写（JSONL，容忍坏行）、`JournalData{root_name, records}`。
- 协议：`Offer.manifest_digest`、`Accept.resume = ResumeState{completed, partial}`（`partial` 预留 L2）。
- 接收端：按摘要查 journal；只信任「目标文件存在且大小一致」的记录；复用 journal 头部记录的接收根名；未完成文件的 `.lp-part` 清理重传。
- 发送端：跳过 completed，进度以已完成字节/文件数初始化；Verify 时对已完成文件用接收端 journal 的 xxh64 比对。
- 测试：`journal` 单测 + `tests/resume.rs`（中途掐断接收端→重启→续传，断言单根、文件树一致、进度从已完成数起步、journal 存在）。
- 待办（L2）：`FileOffset` 单文件偏移续传、块级 bitmap、状态目录 TTL 清理。

## 16. M2 UI 设计（2026-11-20）

### 16.1 设计依据

- 采用 `ui-ux-pro-max` v2.0（194 条推理规则 / Fluent 2 官方风格 / WinUI 3 栈指南 / 预交付检查清单：SVG 图标、对比度 ≥4.5:1、焦点可见、尊重 reduced-motion、文本可重排、响应式 375/768/1024/1440）。
- 技术栈不变：Tauri v2 + TypeScript + Vite，原生 DOM + CSS 变量，不引前端框架。
- 规格文档 `docs/UI-DESIGN.md`；线框 `docs/ui/wireframe-send.png`、`wireframe-receive.png`、`wireframe-settings.png`、`wireframe-mobile.png`；生成脚本 `tools/make_wireframes.py`。

### 16.2 布局

- 桌面 ≥1024：标题栏 48px → 左 NavigationView 76px（发送/接收/历史/设置）→ 内容区（发送页两列：主区 + 360px 设备面板）→ 状态栏 28px。
- 640–1024：单列，左栏收为图标；<640（移动端预留）：底部 Tab，触控目标 ≥44px。
- 发送页：拖放区（唯一主视觉）+ 待发送列表 + 附近设备（刷新/发送）+ IP 直连（完整 IP 或尾号）。
- 接收页：未完成传输横幅（journal 存在时）+ 当前传输卡（进度/速度/剩余）+ 保存位置 + 最近接收。
- 设置页：设备 / 传输 / 网络 / 外观与语言 / 关于 五组。

### 16.3 断点续传的界面表达

续传自动触发（重发同目录 → `manifest_digest` 命中 → `Accept.resume` 跳过已完成）。UI 只提供可观测入口，不提供「继续」按钮：接收页「未完成传输」横幅（进度/来源/清除记录）、历史页「已中断」条目（`已完成/总数` + 「等待发送方继续」）、设置页「断点续传」开关 + 保留天数（默认开 / 7 天）。

### 16.4 模块化与 i18n 约定

- 目录：`api/`（唯一 invoke 边界）、`state/`、`layout/`、`views/`、`components/`、`i18n/`、`styles/`；**单文件 ≤300 行**。
- 视图只做「取状态 → 组合组件 → 绑事件」，不写样式细节、不直接 invoke。
- 语言包 `public/i18n/zh-CN.xml`（`<resources lang><string key>`），`src/i18n/loader.ts` 提供 `loadLocale/t/applyI18n`；key 命名 `area.component.role`；缺失回退中文并告警一次。
- 文案：名词/短句、无「请/您」、无解释、无 emoji；失败信息一行 + 诊断入口。
- Rust 侧用户可见错误统一 `{code, message}`，前端按 `error.<code>` 映射。

### 16.5 待确认

见 `docs/UI-DESIGN.md` §13（布局形态、设备面板位置、续传入口数量、设置分组、XML 语言包）。
