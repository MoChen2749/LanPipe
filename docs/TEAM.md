# 项目分工与交接

本文档说明 LanPipe 的角色分工、协作方式与交接清单，保证任何人（或自动化代理）接手时知道去哪里看、改什么、怎么验收。

## 角色

| 角色 | 负责范围 | 主要文件 |
|---|---|---|
| 项目负责人（人类） | 决策、GitHub 账号与权限、发布审批、密钥保管 | GitHub 仓库设置、Actions Secrets |
| 技术负责人（Lead） | 架构、协议、跨模块改动、发布把关、`master` 合并 | `crates/`、`apps/desktop/src-tauri/` |
| 前端 / UI | 五页界面、i18n、材质与动效、组件化 | `src/`、`public/i18n/`、`index.html` |
| 构建与发布 | 构建脚本、安装包、自动更新、CI/Release | `build.bat`、`tools/`、`.github/workflows/`、`tauri.conf.json` |
| 文档 | README、协议、安装部署、设计说明 | `README.md`、`docs/`、`AGENT.md`、`CONTRIBUTING.md` |
| 测试 | 功能矩阵、跨机与大数据量回归、缺陷复现 | `docs/TEST-REPORT-*.md`、`docs/screenshots/` |

最小团队可以一人多角色；合并到 `master` 前至少经过一次非作者审查（人或代理）。

## 协作约定

- 分支：`master`（稳定）/ `dev`（集成）/ `feat/*`、`fix/*`（短分支），见 `CONTRIBUTING.md`。
- 任务拆到「一个文件或一个目录」粒度，避免多人同时改同一文件；共享任务清单记录预期写入范围。
- 文档与代码同一次提交更新：改协议改 `docs/PROTOCOL-LP1.md`，改界面改 `docs/UI-DESIGN.md`，改构建改 `README.md` 构建章节。
- 版本号只在一处改：`apps/desktop/src-tauri/tauri.conf.json` 的 `version` 与根 `Cargo.toml` 的 `version.workspace`，两者必须一致。

## 交接清单

接手某个角色时，按顺序确认：

1. 环境：Node 20+、Rust stable、`cargo` 在 PATH、WebView2 运行时。
2. 能跑起来：`npm ci` → `npm run build` → `cargo build --release --features custom-protocol -p lanpipe`。
3. 能测：`cargo test --workspace --all-targets`、`cargo clippy --workspace --all-targets -- -D warnings`、`node tools/check-i18n.mjs`（missing 0）。
4. 能打包：`build.bat package`（或 `tools/package.ps1`）产出 MSI 与便携 zip。
5. 能发版：`.github/workflows/release.yml` 所需 Secrets 已配置（见下）。
6. 文档入口：`README.md` → `docs/INSTALL.md` → `docs/PROTOCOL-LP1.md` → `AGENT.md`。

## 密钥与 Secrets

| 名称 | 用途 | 说明 |
|---|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | 自动更新签名 | 私钥内容；**仅存 CI Secret**，不入库 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 私钥口令 | 同上 |
| SSH 部署密钥 | 推送代码 | 本地 `~/.ssh/lanpipe_github`，公钥加到 GitHub 账号或仓库 Deploy Key |

开发期密钥（`.keys/lanpipe-updater.key`，口令 `lanpipe-dev-key`）只用于本机验证，**正式发布必须更换**。

## 发布检查表

- [ ] `dev` 全绿（CI + 本地测试 + clippy）
- [ ] 版本号已更新且两处一致
- [ ] `README.md` / `AGENT.md` / 协议文档已同步
- [ ] `build.bat package` 产物可安装、可卸载、可启动
- [ ] 打 tag `vX.Y.Z` 并确认 Release 产物包含 MSI、`.sig`、便携 zip、`latest.json`
- [ ] 用上一版本安装包验证一次「检查更新」升级路径

## 当前状态

- 最近一次发布：`v0.1.0`（MSI 已验证安装/卸载/运行）。
- 待办见 `README.md`「已知限制」与 `AGENT.md` 里程碑表。
