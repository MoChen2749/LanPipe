# 更新日志

本项目遵循语义化版本（`MAJOR.MINOR.PATCH`）。格式参考 Keep a Changelog。

## [未发布]

### 新增

- Android 客户端：复用 Rust 传输核心与前端，Fluent 2 观感 + 底部 5 项导航；系统文件选择器（SAF）多选文件/目录后拷贝到缓存再发送；前台服务、通知权限、多播锁、edge-to-edge 与系统返回键适配。
- 打包产物统一目录：`tools/package.ps1` 支持 `-Bundles msi,exe,linux,android,portable,all`，所有最终文件复制到仓库根目录 `artifacts/` 并按 `<应用>_<版本>_<平台>.<扩展名>` 固定命名；新增 `-Clean`；`release.yml` 同步使用 `artifacts/`。

### 修复

- Android 上 `net::start` 卡死（UDP socket 交给 tokio 注册时挂起）：改用 tokio 建 socket 后用 `SockRef` 设置广播/复用选项，并跳过逐网卡重复多播加入。

## [0.1.2] - 2026-11-27

### 变更

- 发行形态调整为单文件二进制优先：Release 只发布 Windows MSI、Windows 独立 exe、Linux 桌面二进制、Linux 静态 CLI 四类资产；不再构建或发布便携 zip、deb、AppImage。
- Linux 构建脚本新增 `CLI_TARGET`，可在 Linux / CI 内直接产出 musl 静态 CLI；`release.yml` 的 Linux 作业改为构建桌面二进制与静态 CLI 并追加到同一 Release。
- 文档同步新的发行形态与构建命令（README、`docs/INSTALL.md`、`AGENT.md`）。

## [0.1.1] - 2026-11-26

### 新增

- 项目正式化：`master` / `dev` 分支模型、贡献指南、分工与交接文档。
- CI 与 Release 工作流：提交检查 clippy/test/i18n；打 tag 自动校验版本与签名 Secret，构建 MSI、便携版，签名并生成 `latest.json`，创建 GitHub Release。
- 自动更新端点改为仓库 Release 的 `latest.json` 永久地址，自动更新链路打通，v0.1.0 发布后「检查更新」实测通过。
- 打包集成：`build.bat package` / `tools/package.ps1` 一条命令产出安装包。
- 协议文档 `docs/PROTOCOL-LP1.md`、多端部署文档 `docs/INSTALL.md`。
- Linux 支持：`tools/build-linux.sh`（deps/cli/desktop/bundle）、`tools/build-linux.ps1`（Windows 端交叉编译静态 CLI，或用 WSL 构建桌面端与 deb）；桌面端代码完成平台隔离，可在 Linux 编译运行；deb 依赖补 `fonts-noto-cjk`。
- CI / Release 增加 Linux 作业：校验依赖、构建静态 CLI 与桌面端、测试，发布时构建 deb 并追加到同一 Release。

### 修复

- 修复接收端 `Offer.root_name` 未校验导致的路径穿越风险（仅接受单层目录名，且拒绝反斜杠与冒号）。
- 修复续传时残留 `.lp-part` 不会被清理的问题（删除目标文件后重发可正确重建）。
- 修复大量小文件传输时的校验竞态：接收端在数据连接尚未处理完就发 `Verify`，导致发送端误重传、与在途帧并发写同一文件（表现为 `rename 拒绝访问`、残留 `.lp-part`、连接被重置）。现在接收端等所有文件落地后再出校验清单，并新增 6,000 文件回归测试。

### 变更

- journal 增加 30 天保留期，接收会话开始时后台清理过期记录。

## [0.1.0] - 2026-11-22

### 新增

- 局域网发现（UDP 多播 + 子网广播）与 IP 直连（短 ID 尾号扫描全部子网）。
- 多流 TCP 传输（1–8 条，默认 4），零暂存直写：`.lp-part` → flush → rename。
- xxh64 逐文件校验、Verify/Resync 纠错重传。
- 断点续传 L1：journal 记录已完成文件，重发同一清单自动跳过。
- Tauri v2 + WinUI 3 / Fluent 2 前端（发送/接收/历史/诊断/设置五页，中英留位）。
- Mica/Acrylic 材质、级联入场动效、无边框自绘标题栏、深浅色与窄窗布局。
- 系统集成：防火墙一键放行、托盘常驻、开机自启、单实例。
- 诊断页：网卡/端口/防火墙/材质/续传状态与一键复制。
- MSI 中文安装包、便携版 zip、自动更新骨架（`tauri-plugin-updater`）。

### 修复

- 25 GB / 43,594 文件全量传输验证通过（22.3 分钟，逐文件清单一致）。
- 控制帧上限提升到 64 MiB，修复大清单序列化爆栈。
- 接收端目录延迟创建，修复 6,699 目录阻塞 Accept；DATA 写超时 900 s。
- 单连接错误隔离，避免单流异常拖垮整个会话。
