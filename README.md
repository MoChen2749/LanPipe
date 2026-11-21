# LanPipe

> 局域网文件传输：**零暂存、逐文件直写、多流并行**。拖入文件夹 → 选择设备 → 发送，不做整包归档。
> 目标平台：Windows / macOS / Linux（Tauri 跨端）；M0 先 Windows。

## 现在能做什么（M0）

- ✅ 桌面应用（Rust + Tauri v2 + 原生 WebView2，**无控制台窗口**，前端资源内嵌单文件）
- ✅ **局域网发现**：UDP 53310 多播 `239.255.42.99` + 受限广播双通道，2s 公告、5s 超时
- ✅ **LP/1 传输引擎**：TCP 53311 控制连接 → Offer/Accept → N 条流并行传 DATA 帧 → 逐文件 `.lp-part` → fsync 语义关闭 → 原子 rename
- ✅ **零额外磁盘**：接收端边收边写目标文件，不落地任何 zip/tar 归档；实测峰值额外占用 ≈ 0
- ✅ **GUI 全流程**：拖放汇总 → 设备列表点击即发 → 进度条/百分比/速度 → 接收完成提示
- ✅ **设置持久化**：设备名、接收目录、设备 ID、监听端口、并发流数（1–8）、压缩开关；`%APPDATA%\LanPipe\config.json`
- ✅ **多实例自测**：`--instance b` 使用独立配置目录与偏移端口（53320/53321）
- ✅ **无界面发送模式**（便于脚本/自测）：`LanPipe.exe --send 192.168.1.5:53311 D:\Games --streams 4`
- ⏳ M1：断点续传、文件级 xxHash 校验、防火墙一键放行、托盘、打包安装器

## 实测（本机 loopback，已预热）

| 数据集 | 流数 | 耗时 | 吞吐 |
| --- | --- | --- | --- |
| 903 文件 / 63.9 MiB（900 小文件 1–8 KiB + 3×20 MiB） | 1 | 3.26 s | 19.6 MiB/s · 277 文件/s |
| 同上 | 2 | 1.76 s | 36.4 MiB/s · 514 文件/s |
| 同上 | **4** | **1.30 s** | **51.4 MiB/s · 693 文件/s** |
| 同上 | 8 | 1.27 s | 52.9 MiB/s · 715 文件/s |
| 3×20 MiB 大文件 | 1 | 0.257 s | 233 MiB/s |
| 3×20 MiB 大文件 | 4 | 0.213 s | **295 MiB/s** |

接收结果校验：903 文件 / 66,956,746 B 与源完全一致，无 `.lp-part`/`.zip`/`.tar` 残留。
对比参考：本机 tar 管道（无网络）小文件上限约 2,598 文件/s，LanPipe 目前约 693（4 流），瓶颈在逐文件建文件/改名与帧开销，M1 计划做小文件合批优化。

## 环境要求

| 依赖 | 版本 | 说明 |
| --- | --- | --- |
| Windows | 10/11 或 Server 2019+ | 本机为 Windows Server 2022 |
| Rust | stable（MSVC 工具链） | `rustup` 安装；构建需要 VS Build Tools 的 C++ 组件 |
| Node.js | ≥ 18 | 构建前端（Vite） |
| WebView2 | Evergreen | Win10/11 自带；Server 需单独安装运行时 |

## 构建（双击 `build.bat`）

`build.bat` 是双击即用的交互式菜单（也支持参数：`build.bat build|clean|run|dev|check|cross`）：

```
1) 清理构建产物       2) 构建 Release 单文件 exe
3) 运行（自动部署）   4) 开发模式（Vite+cargo run）
5) 交叉编译           6) 环境自检
0) 退出
```

产物：`dist-bin\LanPipe.exe`，并自动部署到 `%LOCALAPPDATA%\Programs\LanPipe\LanPipe.exe`（菜单 3 直接启动）。
配置文件：`%APPDATA%\LanPipe\config.json`（可写性失败时回退 exe 同目录便携模式）。

命令行等价构建：

```bat
call npm install
call npm run build
cargo build --release --features custom-protocol -p lanpipe
copy target\release\lanpipe.exe "%LOCALAPPDATA%\Programs\LanPipe\LanPipe.exe"
```

> `--features custom-protocol` 必须带：不带时 Tauri 会把应用当作 `tauri dev` 模式，去加载 `http://localhost:1420`，产生白屏。
> 开发模式（`build.bat` 选项 4 / `cargo run`）则**不要**带该 feature，它会连接 Vite dev server 并支持热更新。

> 本机注意：当前开发机对「位于工作区 `F:\Projects` 内的可执行文件」有沙箱限制（只能写工作区路径），因此工作区内的 `dist-bin\LanPipe.exe` 直接双击会因无法创建 WebView2 数据目录而白屏/被拦截。
> `build.bat` 会自动部署到 `%LOCALAPPDATA%\Programs\LanPipe\LanPipe.exe`，请从那里运行。

## 自测

单机双进程（接收端 GUI + 发送端 CLI）：

```bat
:: 1) 启动 GUI（接收端），保持运行
%LOCALAPPDATA%\Programs\LanPipe\LanPipe.exe

:: 2) 另开终端，用 CLI 发送（PowerShell 对 GUI 子系统 exe 不会等待，需 -Wait）
Start-Process "$env:LOCALAPPDATA\Programs\LanPipe\LanPipe.exe" -Wait -NoNewWindow `
  -ArgumentList @('--send','127.0.0.1:53311','D:\Games','--streams','4')

:: 3) 查看结果（CLI 与 GUI 日志已分离）
type "%TEMP%\LanPipe\cli-send.log"
```

断言：`%USERPROFILE%\Downloads\LanPipe\LanPipe-<ts>\` 下文件树与源一致，无 `.lp-part`。

端到端测试（协议层，不需 GUI）：

```bat
cargo test -p lanpipe-net
```

## 目录结构

```
lanpipe/
├─ AGENT.md                 # 项目方向定稿（技术选型/协议/里程碑/验收）
├─ build.bat                # 双击即用构建脚本
├─ index.html, src/         # 前端（TypeScript + Vite，M0 无框架）
├─ crates/
│  ├─ lanpipe-proto/        # LP/1 帧格式与消息（Offer/Accept/DATA/FileDone/Done）
│  └─ lanpipe-net/          # UDP 发现 + TCP 传输引擎（多流、零暂存）
└─ apps/desktop/src-tauri/  # Tauri v2 应用（命令、设置、事件桥接）
```

## 协议要点（LP/1）

- UDP 53310：JSON 公告 `{v,id,name,addrs,port}`；每块网卡分别发多播 + 子网广播，收端按“主用网段”挑选对端地址。
- TCP 53311：控制连接 → `Offer`（manifest：path/size/mtime/is_dir）→ `Accept` → N 条流（每流 `Hello`）→ `DATA`（`id|offset|bytes`）→ `FileDone` → 最后 `Done`。
- 安全：接收端拒绝绝对路径与 `..`，只允许写入接收目录之下。
- 落盘：`.lp-part` → flush → rename；不逐文件 `fsync`（与 tar/robocopy 基线一致，换取吞吐；掉电一致性留待 M1 checkpoint）。
- 校验纠错：每文件边传边算 xxHash64，`Done` 后接收端回 `Verify`，发送端比对后对缺失/不符文件发 `Resync{ids}` 重传（最多 3 轮），通过回 `Complete`。0 字节文件无 DATA 帧，由 `finish_empty_file` 直接建空文件并计数。
- 逐文件 `FileReceived` 事件不转发前端（万级文件时 IPC 会吃吞吐），只上报节流后的进度。

## 路线图（2026-11-20 调整：不做 LocalSend 兼容，转向 GUI 重写与多端适配）

| 阶段 | 内容 |
| --- | --- |
| M0 ✅ | 发现 + 发送/接收 + 设置页 + CLI 自测 + 单 exe |
| M1 | 断点续传（中断后继续）、小文件合批、防火墙一键放行、托盘 |
| M2 | TLS、打包安装器、自动更新评估、诊断页 |
| M2 | GUI 重写（WinUI3/Fluent 视觉 + 组件化 + 深浅色）+ 桌面三端适配 |
| M3 | 打包/签名/自动更新；移动端评估（**不做 LocalSend 协议兼容**） |

详见 [AGENT.md](AGENT.md)。

## 本轮修复（2026-11-19，针对 Minecraft 目录实测反馈）

- **文件数不完整** → 根因是 0 字节文件不产生 DATA 帧、接收端 `finish_file` 提前返回：既不建文件也不计数，`files_done` 永远追不平，最终不触发"接收完成"。已修（`finish_empty_file`），`开服侠（生存）` 128 文件（含 0 字节 `server/permissions.yml`）端到端一致。
- **纠错机制** → `Verify / Resync / Complete` 三轮握手：接收端报告每文件 xxHash64，发送端只重传差异文件（比按流重传更精确）。故障注入测试（`LANPIPE_FAULT_DROP_HASH=1`）通过，重传后文件树与源完全一致。
- **设备发现** →
  - 每块网卡单独发公告（多播 + 子网定向广播），修复多网卡/VPN 机器公告只走 WSL 网卡的问题；
  - 公告携带本机全部 IPv4，接收端优先选与"默认路由网卡"同 /24 的地址，UI 不再显示 10.255.x 这类虚拟网卡地址；
  - 主进程每 2s 重发一次 `peers:update`，UI 增加「刷新」按钮 + 每 4s 自动轮询，解决"上线后看不到设备"。
- **IP 直连** → 支持只输最后一段（如 `5`），自动扫描本机各网段 `x.y.z.5:53311`；也可输入完整 `IP:端口`。
- **并发语义** → UI 明确为"发送并发流数（发送端设置，接收端自动适配）"。
- **同机双实例** → `--instance b` 只偏移 TCP（UDP 发现端口保持 53310），且只在首次生成配置时偏移，端口不再每次启动漂移。
- **性能回归**（本机 loopback）：`开服侠（生存）` 128 文件/96 MB 0.35s（缓存命中）；`1.12.2` 488 文件/510 MB **5.03s ≈ 106 MB/s**（受 F: 双 HDD 镜像卷 ~125MB/s 上限约束）。

---

## 跨机实测（2026-11-19，Windows ↔ WSL Ubuntu 24.04）

> 环境：宿主 Windows Server 2022（Xeon E5-2680 v4 / 28 线程 / F: 双 HDD 镜像卷）；对端为 WSL2 中的 Ubuntu 24.04 + 自制 `lanpipe-cli`（同源码交叉构建）。WSL2 与宿主之间是虚拟 NAT，**不是真实 1GbE 链路**，因此带宽数字只用于验证功能链路，不代表 LAN 吞吐上限。

| 用例 | 结果 |
| --- | --- |
| 双向自动发现 | ✅ 宿主看到 `WSL-Ubuntu 172.18.249.169:53311`；WSL 看到 `WIN-2H1782KNJDF 172.18.240.1:53311` |
| host → WSL（游戏目录 128 文件 / 91.6 MiB，含 0 字节文件） | ✅ 0.5s；接收端 128 文件完全一致，`permissions.yml` 0 字节保留，无 `.lp-part` |
| WSL → host（488 文件 / 510 MiB） | ✅ 5.5s（约 92.6 MiB/s；WSL 读 `/mnt/f` 的 9p 开销为主），文件树完全一致 |
| 纠错重传（接收端 `LANPIPE_FAULT_DROP_HASH=1` 注入坏哈希） | ✅ `Verify → Resync → 重传 → Complete` 后 128/128 一致，只上报 1 次 `RECEIVED` |
| 0 字节文件 | ✅ 保留（空文件也走 `.lp-part → rename`） |
| 25 GB 全量（43,594 文件 / 25.0 GB，F: 游戏目录 → WSL） | ✅ 22.3 min（≈18.7 MB/s，源 HDD IOPS 限制）；逐文件清单一致，`.lp-part` 0 |
| Windows 防火墙 | 入站规则须限定 `-RemoteAddress LocalSubnet`（TCP/UDP 53310-53311、53320-53321），否则 WSL/其它主机连不进来 |

构建 Linux 端（WSL）：

```bash
sudo apt-get install -y build-essential
curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
cd /mnt/f/Projects/lanpipe && cargo build --release -p lanpipe-cli
./target-linux/release/lanpipe-cli recv --dest /tmp/recv --name WSL-Ubuntu
```

## 已知限制

- 同机双实例共用 UDP 发现端口时依赖 SO_REUSEADDR，行为取决于系统；跨机是正常路径。
- 尚无断点续传/会话恢复；传输中断需整目录重传（M1）。
- 未做逐目录 fsync（与 tar/robocopy 一致），需要掉电强一致时由调用方加 checkpoint（M1）。
- 大文件单流上限受磁盘；小文件吞吐仍低于 `tar|bsdtar` 基线（M1 计划批量合并小文件帧）。

## 传输稳健性（2026-11-19 晚）

- 控制帧上限 `MAX_CONTROL_FRAME = 64 MiB`（DATA 仍限 4 MiB）：几万文件的目录清单（Offer）可达数 MB，之前 4 MiB 上限会让 25GB 级目录直接断连。
- TCP keepalive：`TcpKeepalive::with_time(30s).with_interval(10s)`，避免长传输被设备/NAT 判定空闲断开。
- 大清单引发的爆栈：CLI 与 GUI 的发送路径都在独立 64 MiB 栈线程里执行，tokio worker 栈 16 MiB。
- 接收端不再在 `Accept` 前逐个 `create_dir_all`（几万个目录在慢盘上要几十秒，会耗尽发送端等待）；目录在 Accept 后后台补齐，文件写入时也会创建父目录。
- 单条连接异常（半帧/对端断开）只结束该连接，不再把整个会话标记失败；缺失文件由 Done 后的 `Verify → Resync` 补齐。
- DATA 写超时 60s → 180s。

### 断点续传（L1，2026-11-20）

- 接收端按「manifest 摘要」在 `<状态目录>/journals/` 维护 JSONL journal，记录已完成文件（id/size/xxh64）与首次接收根目录名。
- 再次发送同一源目录时，接收端在 `Accept.resume` 返回已完成文件集合，发送端跳过它们；接收根目录复用上次的 `LanPipe-*`，不会重复建目录。
- 完成文件仍参与最终 `Verify`（跳过的文件用 journal 里的 xxh64 比对）；中断留下的 `.lp-part` 首版直接清理重传，单文件内部偏移续传留给 L2。
- 状态目录：Windows `%APPDATA%\LanPipe\state`，Linux `$XDG_STATE_HOME/lanpipe`，可用 `LANPIPE_STATE_DIR` 覆盖；设计依据见 [docs/RESUME-RESEARCH.md](docs/RESUME-RESEARCH.md)。

## UI 设计（M2）

- 设计文档：[`docs/UI-DESIGN.md`](docs/UI-DESIGN.md)（WinUI 3 / Fluent 2，模块化 + i18n XML）
- 线框草图：`docs/ui/wireframe-send.png`、`wireframe-receive.png`、`wireframe-settings.png`、`wireframe-mobile.png`
- 断点续传为自动机制：中断后重新发送同一目录即可续传（详见 UI-DESIGN §6、`docs/RESUME-RESEARCH.md`）
- 短 ID：设备主 IPv4 末段三位数（如 `#005`），用于同子网识别与 IP 直连；完整 UUID 保留为完整 ID（设置页可复制）
- IP 直连：只输 1–3 位尾号 → 扫描本机所有子网的 `*.NNN:53311`；也可输入完整 `IP[:端口]`
- 移动端（<640px）：底部 Tab，仅「选择文件 / 选择文件夹」，无拖入

## 前端结构（M2，2026-11-21）

- `src/styles/`：`tokens.css`（Fluent 2 设计变量）、`base.css`（骨架与响应式）、`components.css`、`views.css`。
- `src/ui/`：DOM 构造与 SVG 图标、格式化函数。
- `src/i18n/`：XML 语言包加载器（`public/i18n/zh-CN.xml`，`t(key, vars)`）。
- `src/api/tauri.ts`：后端命令/事件封装；`src/state/store.ts`：单一状态与事件折叠。
- `src/layout/shell.ts`：标题栏 + 左导航 + 状态栏（<640px 底部 Tab）。
- `src/views/`：`send` / `receive` / `history` / `diagnostics` / `settings` 五页；`src/actions.ts` 业务动作。

构建：`npm run build`（前端 → `dist/`）后再 `cargo build --release --features custom-protocol -p lanpipe`。

### UI 验收截图

```
$env:LANPIPE_CDP = "9333"; Start-Process "$env:LOCALAPPDATA\Programs\LanPipe\LanPipe.exe"
node tools/ui-shot.mjs 9333 docs/shot.png 3 --tall
```

截图由 WebView2 DevTools 协议直接取页面渲染，不依赖屏幕/前台窗口。

## 诊断与单实例（M1）

- 左导航「诊断」：概览（版本/Build/材质/短 ID/完整 ID/配置与状态目录）、网络（逐块网卡/掩码/短 ID、监听端口、发现、并发流、防火墙、接收目录）、附近设备。
- 「刷新」重新采集；「复制诊断信息」把整份 JSON 放进剪贴板。
- 单实例：第二次启动只聚焦已有窗口，不会新开进程；关闭窗口收进托盘。
