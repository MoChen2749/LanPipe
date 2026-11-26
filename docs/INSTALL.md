# LanPipe 安装与部署指南

适用版本：`0.1.2`。最后更新：2026-11-27。

本指南覆盖三类目标：

| 目标 | 状态 |
| --- | --- |
| Windows 桌面端（MSI / 独立 exe / 源码构建） | 已验证 |
| Linux CLI（WSL2 Ubuntu 24.04） | 已在测试中验证 |
| Linux 桌面端（WSL2 Ubuntu 24.04 + WSLg） | 已构建并启动，中文显示正常 |
| 原生 Linux 桌面、macOS | 未验证，仅提供构建路径 |

通用前置说明：

- LanPipe 面向可信局域网，传输为明文且无身份认证，接收端自动接受。不要部署在公网或不可信网段。
- 设备发现使用 IPv4 多播与广播，要求两台设备处于同一子网；跨网段请使用 IP 直连。
- 程序默认监听 UDP 53310、TCP 53311（同机第二实例为 TCP 53321），端口可在设置中修改；均大于 1024，绑定不需要管理员权限。

## 1. Windows

### 1.1 系统要求

- Windows 10 / 11 或 Windows Server，x64（已验证环境为 Windows Server 2022）。更旧系统与 ARM64 未验证。
- Microsoft Edge WebView2 Runtime。Windows 11 与多数已更新的 Windows 10 已内置；Windows Server 与精简系统可能缺失，需安装 Evergreen Runtime（<https://developer.microsoft.com/microsoft-edge/webview2/>）。
- 安装 MSI 需要管理员权限（按机器安装，写入 `C:\Program Files`）。

### 1.2 MSI 安装

构建产物的 MSI 位于仓库根目录 `target/release/bundle/msi/`，提供中文与英文两个安装包：

```text
LanPipe_0.1.2_x64_zh-CN.msi
LanPipe_0.1.2_x64_en-US.msi
```

双击按向导安装。静默安装（管理员 PowerShell 或命令提示符）：

```powershell
msiexec /i "LanPipe_0.1.2_x64_zh-CN.msi" /qn
```

安装结果：

- 程序目录：`C:\Program Files\LanPipe`；
- 开始菜单文件夹：`LanPipe`；
- 控制面板/设置中注册卸载项。

卸载：打开「设置 → 应用 → 已安装的应用」，找到 LanPipe 卸载。静默卸载需要 ProductCode：

```powershell
$product = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*' |
  Where-Object { $_.DisplayName -like 'LanPipe*' } |
  Select-Object -First 1
msiexec /x $product.PSChildName /qn
```

卸载会移除安装目录与快捷方式。用户配置（`%APPDATA%\LanPipe`）、状态目录与接收到的文件属于用户数据，不会被自动删除，需要时手动清理。

### 1.3 独立 exe

从 Release 下载 `LanPipe_v0.1.2_x64-windows.exe`，无需安装、无需管理员权限，放到任意目录运行即可。

- 运行该 exe；需要系统已装 WebView2 Runtime；
- 配置优先写入 `%APPDATA%\LanPipe\config.json`；该位置不可写时（受限权限、便携场景）回退到 exe 同目录的 `config.json`；
- 移除方式：从托盘菜单退出程序、关闭开机自启，然后删除该 exe；按需删除 `%APPDATA%\LanPipe`。

### 1.4 防火墙配置

推荐在应用内操作：在「设置」页点击防火墙放行按钮，确认 UAC 提示即可。程序会写入三条入站规则：

| 规则名 | 类型 |
| --- | --- |
| `LanPipe` | 程序规则 |
| `LanPipe-UDP` | UDP 端口 |
| `LanPipe-TCP` | TCP 端口 |

应用内规则使用 `profile=any`，跨网络位置可用，但放行范围较宽。需要收紧时，删除应用创建的规则后手动添加仅限本子网的规则（管理员权限）：

```powershell
netsh advfirewall firewall delete rule name="LanPipe"
netsh advfirewall firewall delete rule name="LanPipe-UDP"
netsh advfirewall firewall delete rule name="LanPipe-TCP"

netsh advfirewall firewall add rule name="LanPipe-UDP" dir=in action=allow protocol=UDP localport=53310 remoteip=localsubnet
netsh advfirewall firewall add rule name="LanPipe-TCP" dir=in action=allow protocol=TCP localport=53311,53321 remoteip=localsubnet
```

说明：

- 单实例常规使用只需要 UDP 53310 与 TCP 53311；`53321` 仅用于同机第二实例（`--instance b`）自测。
- 测试环境曾同时放行 `53320–53321` 的 TCP/UDP，用于同机双实例验证；其中第二实例实际使用的是 TCP 53321，UDP 仍为 53310。
- 仅放行发送端出站不够：接收端必须允许入站，发现报文是双向的，建议两端都放行。

查看现有规则：

```powershell
Get-NetFirewallRule -DisplayName 'LanPipe*' | Format-Table Name, Enabled, Direction, Action, Profile
```

### 1.5 已验证与待查项

- 已验证：MSI 安装、静默安装（`msiexec /i ... /qn`）、静默卸载、开始菜单与卸载项、独立 exe 启动。
- 待查：NSIS 安装包（`LanPipe_0.1.0_x64-setup.exe`）静默安装 `/S` 返回 0 但未写入文件，当前不作为交付形态。请使用 MSI。

### 1.6 运行数据位置

| 内容 | 路径 |
| --- | --- |
| 配置 | `%APPDATA%\LanPipe\config.json`（第二实例为 `%APPDATA%\LanPipe-b\config.json`） |
| 断点续传 journal | `%APPDATA%\LanPipe\state\journals\<dest>-<digest>.jsonl` |
| 默认接收目录 | `%USERPROFILE%\Downloads\LanPipe\LanPipe-<Unix 秒>\` |
| 界面日志 | `%TEMP%\LanPipe\lanpipe.log` |
| CLI 发送日志 | `%TEMP%\LanPipe\cli-send.log` |
| 防火墙临时脚本 | `%TEMP%\lanpipe-firewall.ps1`（执行后可删除） |

状态目录可用环境变量 `LANPIPE_STATE_DIR` 覆盖；配置损坏时会备份为 `config.json.bad`。

## 2. Linux

Linux 版提供两个**单文件二进制**（不提供 deb/AppImage 安装包）：桌面端 `LanPipe_vX.Y.Z_x86_64-linux` 与静态 CLI `lanpipe-cli_vX.Y.Z_x86_64-linux`，均从 Release 下载。

构建脚本：`tools/build-linux.sh`（原生 Linux、WSL、容器、CI 通用）。

```bash
bash tools/build-linux.sh deps      # 检查工具链与系统库
bash tools/build-linux.sh cli       # 静态 CLI -> dist-linux/lanpipe-cli
bash tools/build-linux.sh desktop   # 桌面二进制 -> dist-linux/lanpipe
```

在 Windows 上可用 `tools/build-linux.ps1`：`cli` 直接交叉编译静态 CLI（rust 自带 lld，不需要 WSL）；`desktop`/`deps` 通过 WSL 执行。

```powershell
powershell -File tools/build-linux.ps1 -Command cli
powershell -File tools/build-linux.ps1 -Command desktop -Distro lanpipe-build
```

### 2.1 命令行版

静态二进制，可直接拷到任意 x86-64 Linux（musl，无 glibc 版本要求）：

```bash
chmod +x lanpipe-cli_v0.1.2_x86_64-linux
./lanpipe-cli_v0.1.2_x86_64-linux recv --dest /data/inbox --name build-node
./lanpipe-cli_v0.1.2_x86_64-linux send 192.168.1.5:53311 /data/out --streams 4
```

从源码构建：

```bash
sudo apt install -y build-essential curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add x86_64-unknown-linux-musl
CLI_TARGET=x86_64-unknown-linux-musl bash tools/build-linux.sh cli
```

- 接收端会在标准输出打印 `READY`、`PEER-UP`、`RECEIVED` 等事件行，便于脚本解析。
- 状态目录优先级：`LANPIPE_STATE_DIR` → `$XDG_STATE_HOME/lanpipe` → `~/.local/state/lanpipe`。
- 防火墙：若启用了 ufw/firewalld，需要放行 UDP 53310 与 TCP 53311 入站。

### 2.2 桌面版

发布形态为单文件二进制，运行时依赖系统的 GTK3 / WebKitGTK 4.1 等库（与 Tauri 应用一致）：

```bash
sudo apt install libwebkit2gtk-4.1-0 libgtk-3-0 libayatana-appindicator3-1 librsvg2-2
chmod +x LanPipe_v0.1.2_x86_64-linux
./LanPipe_v0.1.2_x86_64-linux
```

若界面中文显示为方块，安装中文字体：`sudo apt install fonts-noto-cjk`。

从源码构建：

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libjavascriptcoregtk-4.1-dev \
  libsoup-3.0-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev
npm ci
bash tools/build-linux.sh desktop
```

注意：

- 已验证：WSL2 Ubuntu 24.04 下构建成功并在 WSLg 中启动，中文渲染正常（`docs/screenshots/linux-wslg.png`）；原生 Linux 桌面机未验证；
- 材质：Mica/Acrylic 为 Windows 实现，Linux 走普通 GTK 窗口；
- 二进制不会自动注册桌面项、托盘图标依赖桌面环境的 AppIndicator 支持。

## 3. macOS（未验证）

前置条件：

- Xcode Command Line Tools：`xcode-select --install`；
- Rust stable（rustup 安装）；
- Node.js 18 及以上。

构建桌面版：

```bash
npm install
npm run build
cd apps/desktop/src-tauri
npx tauri build --bundles app,dmg
```

注意：

- 同样需要覆盖 `bundle.targets`（配置中仅声明 `msi`）；
- 分发 `.app`/`.dmg` 需要 Apple 开发者签名与公证，仓库当前未配置 macOS 签名；
- 未签名产物在本机运行可能被 Gatekeeper 拦截，仅供开发自测；
- Windows 专属能力（Mica/Acrylic 材质、Windows 防火墙放行、注册表开机自启）在 macOS 上不可用；防火墙需在「系统设置 → 网络」中手动允许；
- CLI 理论上可用 `cargo build --release -p lanpipe-cli` 构建，未验证。

## 4. 从源码构建（通用）

### 4.1 依赖

| 依赖 | 版本/说明 |
| --- | --- |
| Rust | stable；由 `rust-toolchain.toml` 固定，含 rustfmt 与 clippy |
| Node.js | 18 及以上（Vite 6 要求） |
| npm | 随 Node.js 提供 |
| Windows | MSVC Build Tools、WebView2 Runtime |
| Linux 桌面 | 见 §2.2 系统依赖 |
| macOS | Xcode Command Line Tools |

### 4.2 构建步骤

```powershell
npm install
npm run build
cargo build --release --features custom-protocol -p lanpipe
```

- 必须先构建前端：`frontendDist` 指向 `dist/`；
- 生产构建必须启用 `custom-protocol` feature，否则程序会加载开发服务器地址并显示空白窗口；
- 开发调试改用 `npm run dev` 与 `cargo run -p lanpipe`（不带该 feature）；
- Windows 上也可运行 `build.bat`，按菜单构建并部署到 `%LOCALAPPDATA%\Programs\LanPipe`。

### 4.3 测试与检查

```powershell
cargo fmt --check
cargo clippy -- -D warnings
cargo test
cargo test -p lanpipe-net     # 包含端到端传输测试
node tools/check-i18n.mjs
```

### 4.4 生成发行物

Windows MSI：

```powershell
npm run build
cd apps/desktop/src-tauri
npx tauri build --bundles msi
```

产物在仓库根目录的 `target/release/bundle/msi/`。首次构建会自动下载 WiX 工具链。

Windows 独立 exe：

```powershell
cargo build --release --features custom-protocol -p lanpipe
Copy-Item target\release\lanpipe.exe "target\release\artifacts\LanPipe_vX.Y.Z_x64-windows.exe"
```

Linux 二进制（原生 Linux，或在 Windows 上经 `tools/build-linux.ps1` 走 WSL）：

```bash
bash tools/build-linux.sh desktop        # 桌面端 -> dist-linux/lanpipe
CLI_TARGET=x86_64-unknown-linux-musl bash tools/build-linux.sh cli   # 静态 CLI
```

上述产物由 `.github/workflows/release.yml` 在推送 `v*` tag 时自动构建并上传 Release。自动更新签名与发布：`tools/publish-release.ps1` 生成 `.sig` 与 `latest.json`，签名私钥通过 `TAURI_SIGNING_PRIVATE_KEY` 与 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 提供；更新端点为仓库 Release 的 `latest.json`。

## 5. 常见问题（FAQ）

### 5.1 设备列表里看不到对方

按顺序排查：

1. 两台设备是否在同一 IPv4 子网。多播/广播不跨网段，跨网段只能 IP 直连。
2. Windows 网络位置是否为「专用网络」。公用网络下入站规则更严格。
3. 防火墙是否放行（见 §1.4）；接收端必须允许入站 UDP 53310 与 TCP 53311。
4. 路由器或无线 AP 是否开启客户端隔离（AP Isolation），开启后设备之间无法互访。
5. 是否连接了 VPN、虚拟网卡或多网卡；多播报文可能从非预期网卡发出，可临时禁用后重试。
6. 仍无法发现时，在发送页直接输入对方 IP 或 IPv4 末段短号（如 `5`）。

### 5.2 能发现设备，但发送一直停在连接中

- 检查接收端 TCP 53311 是否可达：

  ```powershell
  Test-NetConnection 192.168.1.5 -Port 53311
  ```

- 检查端口是否被占用：

  ```powershell
  netstat -ano | findstr :53311
  ```

  冲突时结束占用进程，或在设置中更换监听端口（两端需一致）。
- 确认两端使用同一版本的 LanPipe；协议仍在演进，跨版本可能不兼容。

### 5.3 接收端没有确认弹窗，直接开始接收文件

这是当前版本的设计（接收端自动接受，无身份认证）。风险与适用范围见 README 的「安全边界」。不在可信网络时应退出程序，而不是仅最小化到托盘。

### 5.4 窗口整窗发灰、透明或材质失效

- Mica 需要 Windows 11 build 22000 及以上，且系统「透明效果」开启；Acrylic 需要 Windows 10 build 17763 及以上；更旧系统退化为纯色。
- WebView2 145.x / 154.x 存在窗口透明回归（上游 issue #5481、#5752）。程序检测到受影响版本会自动回退纯色，诊断页会显示 WebView2 版本与「窗口透明 运行时缺陷，已回退纯色」。
- 诊断页可直接查看 WebView2 版本；也可在「设置 → 应用 → 已安装的应用」中查看「Microsoft Edge WebView2 Runtime」。
- 远程桌面、部分虚拟机与 Windows Server 环境不提供材质效果。
- 排障可用环境变量强制指定：`LANPIPE_BACKDROP=none` 强制纯色，`LANPIPE_TRANSPARENCY=force` 强制透明。

### 5.5 接收目录出现 `.lp-part` 文件

`.lp-part` 表示该文件尚未接收完成。传输中断后重新向同一接收端发送同一目录即可续传，LanPipe 会清理残留并重传未完成文件。确认不再续传后可手动删除。

### 5.6 命令行 `--send` 没有反应

单实例限制：已有 LanPipe 进程运行时，再次启动只会在已有实例中处理参数，`--send` 会被忽略。先从托盘菜单退出 LanPipe，再运行命令行；或改用图形界面发送。CLI 发送日志在 `%TEMP%\LanPipe\cli-send.log`。

### 5.7 检查更新失败

预期行为。`plugins.updater.endpoints` 指向 `https://github.com/MoChen2749/LanPipe/releases/latest/download/latest.json`，需等仓库发布过 Release（至少一个含 `latest.json` 的版本）后才会返回 200；签名密钥与发布流程见 AGENT.md §16.12。

### 5.8 安装被 SmartScreen 拦截 / 卸载后有残留

- 安装包当前未做代码签名，SmartScreen 提示属于预期；确认来源后选择「更多信息 → 仍要运行」。
- 卸载后残留按需手动清理：
  - 防火墙规则：`netsh advfirewall firewall delete rule name="LanPipe-UDP"`（同法处理 `LanPipe`、`LanPipe-TCP`）；
  - 开机自启：注册表 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 下的 `LanPipe` 值；
  - 用户数据：`%APPDATA%\LanPipe`（配置与 journal）与接收目录。

### 5.9 传输速度不理想

- 并发流默认 4，可在设置中调整 1–8；大量小文件时提高并发更有效。
- 源盘或目标盘为机械硬盘时受 IOPS 限制；25 GB 实测约 18.7 MB/s 即为此类瓶颈。
- Wi-Fi、USB 网卡、WSL NAT 与虚拟化网络都会显著低于千兆有线。
- 压缩默认关闭；对已压缩内容开启压缩没有收益。

本指南引用的实测数字来源：README「性能实测」章节与 [TEST-REPORT-2026-11-19.md](TEST-REPORT-2026-11-19.md)。

### 5.10 端口与协议一览

| 用途 | 协议 | 端口 |
| --- | --- | --- |
| 设备发现 | UDP | 53310 |
| 控制与数据 | TCP | 53311 |
| 第二实例（`--instance b`） | TCP | 53321 |

`53320` 不用于正常通信；测试环境曾为同机双实例自测一并放行 `53320–53321`，其中第二个 TCP 端口实际只用到 53321。

### 5.11 收集诊断信息

反馈问题前建议附上：

```powershell
# 程序版本、配置路径、WebView2 版本、端口与连通性：见应用「诊断」页
Get-Content "$env:TEMP\LanPipe\lanpipe.log" -Tail 200
```

同时说明操作系统版本、网络拓扑（有线/无线、是否跨网段、是否启用 VPN）、两端 LanPipe 版本，以及问题发生的时间点。

## 6. 验证状态汇总

| 项目 | 状态 |
| --- | --- |
| Windows MSI 安装/静默安装/卸载 | 已验证 |
| Windows 独立 exe | 已验证 |
| Windows 防火墙一键放行 | 已验证（`profile=any`） |
| 25 GB / 43,594 文件传输（Windows ↔ WSL2） | 已验证 |
| Linux CLI 构建与传输 | 已在 WSL2 Ubuntu 24.04 验证 |
| Linux 桌面端构建与运行（WSLg） | 已构建、启动、截图验证中文 |
| macOS 构建与运行 | 未验证 |
| Windows ARM64 构建产物运行 | 未验证 |
| NSIS 静默安装 | 未通过，待查 |
| 自动更新 | 已发布 v0.1.0/v0.1.1，检测与升级安装实测通过；自动下载安装未实现 |
| 真实物理 1 GbE / 2.5 GbE 吞吐 | 未验证 |
