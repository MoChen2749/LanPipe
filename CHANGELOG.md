# 更新日志

本项目遵循语义化版本（`MAJOR.MINOR.PATCH`）。格式参考 Keep a Changelog。

## [未发布]

### 新增

- 项目正式化：`main` / `dev` 分支模型、贡献指南、分工与交接文档。
- CI 与 Release 工作流：提交检查 clippy/test/i18n，打 tag 自动构建 MSI、便携版并生成 `latest.json`。
- 打包集成：`build.bat package` / `tools/package.ps1` 一条命令产出安装包。
- 协议文档 `docs/PROTOCOL-LP1.md`、多端部署文档 `docs/INSTALL.md`。

### 修复

- 修复接收端 `Offer.root_name` 未校验导致的路径穿越风险（仅接受单层目录名）。
- 修复续传时残留 `.lp-part` 不会被清理的问题（删除目标文件后重发可正确重建）。

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
