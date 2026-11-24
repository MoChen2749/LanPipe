# 贡献指南

LanPipe 使用 `master` / `dev` 双分支模型。所有改动先进入 `dev`，验证通过后再合并到 `master`。

## 分支模型

| 分支 | 用途 | 规则 |
|---|---|---|
| `master` | 稳定发布分支 | 只接受来自 `dev` 的合并；每次合并打 tag（`v0.1.1`）并触发 Release |
| `dev` | 日常集成分支 | 功能与修复先合到这里；CI 必须全绿 |
| `feat/<name>` `fix/<name>` | 短生命周期分支 | 从 `dev` 切出，完成后 PR 回 `dev` |

流程：

```text
feat/xxx ──PR──> dev ──回归通过──> main ──tag vX.Y.Z──> GitHub Release
```

## 提交规范

使用 Conventional Commits（中文描述可以）：

```text
feat(net): 支持 8 条并发流的自适应调度
fix(ui): 修复下拉浮层层级被卡片遮挡
docs(protocol): 补充 Resync 帧字段
chore: 清理测试截图
```

常用类型：`feat`、`fix`、`docs`、`refactor`、`perf`、`test`、`build`、`ci`、`chore`。

## 合并前检查

本地必须全部通过（CI 会重复执行）：

```powershell
npm run build
node tools/check-i18n.mjs
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

注意：

- 前端改动必须先 `npm run build` 再 `cargo build`，因为 `dist/` 会被嵌入 exe。
- i18n 只允许 `t("key")`，`tools/check-i18n.mjs` 的 `missing` 必须为 0。
- 不要提交 `dist/`、`target/`、`.keys/`、安装包产物与测试截图。

## 代码约定

- 单一文件不超过 300 行（前端视图/组件）；超出就拆分模块。
- Rust：错误用 `Result<_, String>` 返回给前端；命令注册在 `generate_handler!` 且与 `src/api/tauri.ts` 同步。
- 协议字段变更必须同步 `docs/PROTOCOL-LP1.md`，并说明兼容策略（`#[serde(default)]` / 版本号）。
- 视觉与交互遵循 `docs/UI-DESIGN.md`；材质分层见 `AGENT.md` §16.7。

## 发布

```powershell
# 1. 更新版本号（tauri.conf.json 与 Cargo.toml workspace）
# 2. 全量验证
npm run build; cargo test --workspace; cargo clippy --workspace --all-targets -- -D warnings
# 3. 打包（CI 也会做）
powershell -NoProfile -ExecutionPolicy Bypass -File tools/package.ps1
# 4. 合并到 master 并打 tag
git checkout master; git merge --no-ff dev; git tag -a v0.1.1 -m "LanPipe 0.1.1"; git push origin master --tags
```

Release 工作流会构建 MSI、便携版、签名并生成 `latest.json`，上传到 GitHub Release。

## 报告问题

请附：Windows/Linux 版本、`诊断` 页导出的信息（`复制诊断信息`）、复现步骤、期望与实际结果。
