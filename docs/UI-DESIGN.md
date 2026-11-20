# LanPipe UI 设计（M2 · WinUI 3 / Fluent 2）

> 状态：**设计草案，待确认**（2026-11-20）。确认后进入实现。
>
> 设计方法：按 `ui-ux-pro-max` v2.0（194 条推理规则 / Fluent 2 官方体系 / WinUI 3 栈指南 / 预交付检查清单）产出；技术栈仍为 Tauri v2 + TypeScript + Vite（M0/M1 已定），不引入前端框架，组件用原生 DOM + CSS 变量实现。
>
> 线框草图：`docs/ui/wireframe-send.png`、`wireframe-receive.png`、`wireframe-settings.png`、`wireframe-mobile.png`（带分区与编号标注，图例在每张图下方）。

---

## 1. 设计原则

1. **一个主操作**：发送页只有「拖入 → 选设备」一条主线；接收页只呈现状态，不需要用户操作。
2. **状态即界面**：传输的 6 个状态（空闲/等待接受/发送中/接收中/已完成/已中断）各有唯一表达，不弹模态框。
3. **不解释**：界面不写原理、不写「为什么」、不写欢迎语；标题 + 数值 + 动作即可。
4. **可恢复**：中断不隐藏，用「未完成传输」常驻横幅告诉用户「会自动续传」。
5. **模块化**：单文件 ≤ 300 行；视图只组合组件，组件不直接调 `invoke`；数据集中在 store。
6. **移动端可扩展**：桌面用 NavigationView（左栏），窄屏自动切底部 Tab；布局用 CSS Grid + 断点，不写第二套页面。

---

## 2. 设计 token（Fluent 2 语义）

| 语义 | 浅色 | 深色 | 用途 |
| --- | --- | --- | --- |
| `--accent` | `#005FB8` | `#4CC2FF` | 主按钮、选中态、进度条 |
| `--accent-soft` | `#E5F1FB` | `#1B3A52` | 选中背景、信息横幅 |
| `--bg` | `#F3F3F3` | `#1F1F1F` | 窗口背景 |
| `--surface` | `#FFFFFF` | `#2B2B2B` | 卡片 |
| `--border` | `#E1E1E1` | `#3A3A3A` | 分隔线、卡片描边 |
| `--text` | `#1A1A1A` | `#F5F5F5` | 主文本 |
| `--muted` | `#606060` | `#B0B0B0` | 次要文本 |
| `--success` | `#107C10` | `#6CCB5F` | 完成状态 |
| `--danger` | `#C42B1C` | `#FF99A4` | 失败/中断 |

- 圆角：卡片 8px，控件 6px，胶囊 999px。
- 间距：4 / 8 / 12 / 16 / 24 / 32（8px 网格）。
- 字体：`Segoe UI Variable, "Microsoft YaHei UI", "Segoe UI", system-ui`；正文 13px / 行高 20，标题 20px / 28，卡片标题 13px 加粗，辅助 11px。
- 控件高度：按钮 32px（紧凑）= 触控场景 44px；输入框 32px；导航项 48px。
- 动效：120–180ms ease-out；进度条宽度 transition 150ms；遵循 `prefers-reduced-motion`（关闭 transition）。
- 对比度：正文 ≥ 4.5:1，大字 ≥ 3:1；焦点环 `2px var(--accent)` + 2px offset。

---

## 3. 桌面布局（≥1024px）

```
┌──────────────────────────────────────────────────────────────┐
│ ① 标题栏 48px：图标+LanPipe | 本机身份胶囊 | 设置             │
├──────┬───────────────────────────────────────────────────────┤
│ ②    │                                                       │
│ Nav  │  内容区（padding 24，最大宽度 1120）                  │
│ 76px │                                                       │
│      │                                                       │
├──────┴───────────────────────────────────────────────────────┤
│ ⑦ 状态栏 28px：发现状态 · 已发现设备数 | 版本                  │
└──────────────────────────────────────────────────────────────┘
```

- 标题栏：应用图标 + 名称 + 副标题「局域网传输」；右侧「本机 · <设备名>」胶囊（只读，点击进设置）+ 设置图标按钮。
- 左栏（NavigationView）：发送 / 接收 / 历史 / 设置；选中项 3px 强调条 + 浅色底；窄屏折叠为底部 Tab。
- 内容区：`max-width: 1120px`，卡片间距 16；桌面两列（主内容 1fr / 设备面板 360px），640–1024 变单列。

---

## 4. 页面规格

### 4.1 发送页（`wireframe-send.png`）

| 区域 | 内容 | 交互 |
| --- | --- | --- |
| ③ 拖放区 | 图标 + 「拖入文件或文件夹」+「支持多选 · 目录原样保留」+ 选择文件/选择文件夹 | 拖入高亮 2px accent；空态高度 180；有内容时收缩为 120 |
| ④ 待发送列表 | 行：类型图标 / 名称 / 「文件夹 · 2,954 项 · 3.9 GB」/ 移除；标题「待发送 · 3 项 · 4.2 GB」；底部主按钮「选择设备发送」 | 点击行选中（可多选合并）；再次点击设备卡直接发送 |
| ⑤ 附近设备 | 刷新 + 设备卡：名称 / IP:端口 / 状态（在线·空闲）/「发送」 | 状态文字 + 颜色双编码；点击卡片空白处选中 |
| ⑥ IP 直连 | 输入框（完整 IP 或尾号）+「连接」；尾号提示一行 | 输入尾号 → 扫描；失败就地红字，不弹窗 |
| ⑦ 状态栏 | 「发现中 · 2 台设备」 | 点击进设置·诊断 |

**状态**：未选内容 → 设备卡「发送」禁用；无设备 → 列表空态「未发现设备」+ 提示「刷新或 IP 直连」；发送中 → 设备卡变「取消」，底部按钮变进度。

### 4.2 接收页（`wireframe-receive.png`）

| 区域 | 内容 | 说明 |
| --- | --- | --- |
| ① 未完成传输横幅 | 「来自 <设备> · <目录> · 863/2,954 文件（29%）」+「清除记录」「查看位置」 | 仅当 journal 有记录时出现；随「再次发送」自动消失 |
| ② 当前传输卡 | 来源设备、进度条、`1,842 / 2,954 文件 · 2.4 / 4.2 GB`、`86 MB/s · 剩余 12 秒`、当前文件路径（截断） | 暂停/取消按钮先占位（M3） |
| ③ 保存位置 | 只读路径 + 更改 + 打开 | 更改即时生效 |
| ④ 最近接收 | 行：目录名 / 状态（完成·已中断·失败）/ 时间 / 大小；点「已中断」行可清除或等待续传 | 最多 10 条，来自 journal + 内存 |

### 4.3 历史页

与「最近接收」同构，但含发送与接收两类，支持按状态筛选（全部/完成/中断/失败）、「重新发送」「打开位置」「清除记录」。

### 4.4 设置页（`wireframe-settings.png`）

分组卡片：设备（名称、ID 复制/重置）→ 传输（接收目录、并发流数 1–8、压缩 关/自动、断点续传开/关 + 保留 7 天、传输后校验 快速/严格）→ 网络（发现开关、UDP/TCP 端口、防火墙一键放行、诊断页）→ 外观与语言（主题 跟随系统/浅/深；语言 中文（简体））→ 关于（版本、许可、检查更新）。

### 4.5 移动端扩展（`wireframe-mobile.png`）

- 断点 <640px：底部 Tab（发送/接收/历史/设置），标题栏压缩为 44px，隐藏副标题。
- 拖放区改为「选择文件/选择文件夹」两个大按钮（触控优先），设备卡整行可点。
- 传输卡纵向堆叠；进度条高度 8；按钮最小 44×44；列表行高 56。

---

## 5. 传输状态 → 界面映射

| 状态 | 界面表现 | 文案（中文） |
| --- | --- | --- |
| 空闲 | 发送页空态 | 「拖入文件或文件夹」 |
| 等待接受 | 设备卡显示「等待对方接受…」 | 「等待对方接受」 |
| 发送中 | 底部进度条 + `已发送 x / y · z MB/s` | 「发送中」 |
| 接收中 | 接收卡进度 + 当前文件 | 「接收中」 |
| 已完成 | 卡片绿色状态点 + 摘要，3 秒后收起到最近列表 | 「接收完成」/「发送完成」 |
| 已中断 | 接收页横幅 + 历史页「已中断」条目 | 「未完成传输」 |
| 校验/重传 | 进度条保持 100%，副文本「校验中」/「重传 3 个文件」 | 「校验中」 |
| 失败 | 红色状态点 + 一行原因（可展开详情） | 「传输失败」 |

---

## 6. 断点续传的界面表达（回应「如何触发」）

**触发是自动的，没有「继续」按钮**——续传状态由接收端 journal 决定，发送方重发同一目录时自动匹配：

1. 传输中断（关闭接收端/断网/任一进程崩溃）→ 接收端保留 `journal` 与已落盘文件。
2. 发送方**再次选择并发送同一目录**（同路径、同大小、同 mtime → 同一 `manifest_digest`）。
3. 接收端在 `Accept.resume` 里返回已完成文件列表；发送端跳过这些文件，界面上：
   - 接着上次进度显示（例如 `863 / 2,954` 起）；
   - 传输卡副标题显示「续传 · 已跳过 863 个文件」；
   - 接收页横幅消失，转为普通「接收中」。
4. 完成后照常校验（`Verify`）；跳过的文件信任 journal 的 xxh64，设置里「传输后校验：严格」可强制重算。

**用户可见入口**（本次设计新增）：
- 接收页「未完成传输」横幅：显示进度、来源、保存位置，可「清除记录」；
- 历史页「已中断」条目：显示 `863/2,954`，提示「等待发送方继续」；
- 设置页「断点续传」开关（默认开）与保留天数（7 天）。

**手动触发（供测试）**：命令行 `lanpipe-cli recv --dest <目录> --name <名字>` 运行中 `pkill` 或关闭窗口；重新运行同一 `--dest`，再用同一源目录发送即可。

---

## 7. 组件与模块划分（单文件 ≤ 300 行）

```
index.html                     # 只放 <div id="app">
public/i18n/zh-CN.xml          # 语言包（仅中文）
src/
  main.ts                      # 启动：加载 i18n → 挂载 Shell（≤80 行）
  i18n/
    loader.ts                  # XML 解析、缓存、t(key, params)
    types.ts
  api/
    lanpipe.ts                 # 所有 invoke 封装（单一后端边界）
    events.ts                  # peers/transfer 事件订阅
  state/
    store.ts                   # 极简订阅式 store（set/get/subscribe）
    peers.ts                   # 设备列表状态
    transfers.ts               # 传输状态机 + 最近记录
    settings.ts                # 设置读写
  layout/
    Shell.ts                   # 标题栏 + Nav + 内容区插槽 + 状态栏
    NavRail.ts
    TitleBar.ts
    StatusBar.ts
  views/
    SendView.ts
    ReceiveView.ts
    HistoryView.ts
    SettingsView.ts
    AboutView.ts
  components/
    Button.ts / Field.ts / Toggle.ts / Select.ts / Slider.ts
    Card.ts / EmptyState.ts / DeviceCard.ts / StagedRow.ts
    ProgressBar.ts / TransferCard.ts / ResumeBanner.ts / ListRow.ts
  styles/
    tokens.css   # 设计 token（§2）
    base.css     # reset + 字体 + 焦点环
    layout.css
    views/*.css
```

约束：
- 视图文件只做「取状态 → 拼组件 → 绑事件」，不写样式细节、不直接 `invoke`。
- 组件接收 props、发出回调；不订阅全局事件。
- `api/*` 是唯一知道 Tauri 命令名的地方；Rust 侧错误统一映射成 `{ code, message }`，前端按 `code` 取 i18n 文案。

---

## 8. 多语言（i18n）设计

### 8.1 文件格式（XML）

`public/i18n/zh-CN.xml`：

```xml
<?xml version="1.0" encoding="UTF-8"?>
<resources lang="zh-CN" name="中文（简体）" version="1">
  <string key="app.name">LanPipe</string>
  <string key="app.subtitle">局域网传输</string>
  <string key="nav.send">发送</string>
  <string key="nav.receive">接收</string>
  <string key="nav.history">历史</string>
  <string key="nav.settings">设置</string>
  <string key="send.drop.title">拖入文件或文件夹</string>
  <string key="send.drop.hint">支持多选 · 目录原样保留</string>
  <string key="send.pick.files">选择文件</string>
  <string key="send.pick.folder">选择文件夹</string>
  <string key="send.staged.title">待发送 · {count} 项 · {size}</string>
  <string key="send.devices.title">附近设备</string>
  <string key="send.devices.empty">未发现设备</string>
  <string key="send.direct.title">IP 直连</string>
  <string key="send.direct.connect">连接</string>
  <string key="transfer.sending">发送中</string>
  <string key="transfer.receiving">接收中</string>
  <string key="transfer.verifying">校验中</string>
  <string key="transfer.done">接收完成</string>
  <string key="transfer.failed">传输失败</string>
  <string key="transfer.interrupted">未完成传输</string>
  <string key="transfer.resume.skipped">续传 · 已跳过 {count} 个文件</string>
  <string key="transfer.meta.files"> {done} / {total} 文件</string>
  <string key="transfer.meta.speed">{speed} · 剩余 {eta}</string>
  <string key="settings.transfer.dir">接收目录</string>
  <string key="settings.transfer.streams">并发流数</string>
  <string key="settings.transfer.resume">断点续传</string>
  <string key="settings.transfer.verify">传输后校验</string>
  <string key="settings.network.firewall">一键放行</string>
  <string key="settings.appearance.language">语言</string>
  <string key="common.save">保存</string>
  <string key="common.cancel">取消</string>
  <string key="common.open">打开</string>
</resources>
```

规则：
- key 命名 `area.component.role`；新增 key 必须同时进所有语言包（当前只有 zh-CN）。
- 占位符用 `{name}`；单复数暂不处理（中文无复数），数值格式化在 loader 内做。
- 不使用 `innerHTML` 拼文案；统一 `t()` 返回纯文本，避免 XML 注入。
- 语言包按需 `fetch`，解析用 `DOMParser`；缺失 key 回退 `zh-CN` 并 `console.warn` 一次。
- 预留 `localStorage.lanpipe.locale`；设置页语言下拉从 `available` 列表读（M2 只有 zh-CN，M3 加 en-US / ja-JP）。
- 后端 Rust 的用户可见错误：统一 `error_code` → 前端映射 `error.<code>`；M2 先只给通用文案。

### 8.2 加载器接口

```ts
export type Locale = "zh-CN";
export async function loadLocale(locale: Locale): Promise<void>;
export function t(key: string, params?: Record<string, string | number>): string;
export function getLocale(): Locale;
export function availableLocales(): { code: Locale; name: string }[];
```

`applyI18n(root, keys)`：给元素 `data-i18n="key"` 的节点填文本、`data-i18n-attr="placeholder:key"` 填属性；切换语言时重跑一次。

---

## 9. 文案规范

- 短句、名词优先：按钮 2–4 字（发送/取消/打开/更改），标题 ≤ 8 字。
- 不写「请」「您」「点击这里」「注意」；不用感叹号、不用 emoji。
- 不解释原理（不写「基于断点续传技术」）；只用数字和状态。
- 失败信息一行：`无法连接到 192.168.1.5:53311`，详情放「诊断」。
- 中英混排空格：`192.168.1.5:53311`、`4.2 GB`、`86 MB/s`。

| 不要写 | 写成 |
| --- | --- |
| 「欢迎使用 LanPipe，这是一个……的工具」 | 「LanPipe」 |
| 「请点击下方按钮选择文件」 | 「选择文件」 |
| 「该文件正在传输中，请耐心等待」 | 「接收中」 |
| 「断点续传功能已开启，中断后可继续」 | 「断点续传 · 开启」 |
| 「传输失败，可能是网络问题或对方离线」 | 「传输失败」+「诊断」 |

---

## 10. 可访问性与响应式

- 全部可交互元素可 Tab 到达，焦点环可见；按钮有 `aria-label`，图标按钮必须有。
- 进度条 `role="progressbar"` + `aria-valuenow`。
- 设备状态不只靠颜色：文字「在线/空闲/离线」+ 圆点。
- 断点：≥1024 双列；640–1024 单列、左栏收为 56px 图标；<640 底部 Tab。
- 文本可重排：长路径/文件名用 `text-overflow: ellipsis` + `title`，不硬截断字符。
- 尊重 `prefers-reduced-motion` 和 `forced-colors`。

---

## 11. 与后端接口（沿用现有命令）

| 前端调用 | 现有命令 | 备注 |
| --- | --- | --- |
| 读设置 | `get_settings` / `config_location` | 保持 |
| 存设置 | `save_settings` | 失败返回 `{code, message}` |
| 设备列表 | `list_peers` + `peers:update` 事件 | 已有 2s 推送 |
| 发送 | `start_send(target, paths, streams)` | 前端只传目标与路径 |
| 传输事件 | `transfer:event` | 状态机在 `state/transfers.ts` |
| IP 直连 | `start_send` / `scan_last_octet` | 设备选择逻辑在 `state/peers.ts` |
| 接收完成/中断 | `transfer:event`（`received`/`failed`） | 续传横幅数据后续由 `list_resumable` 提供（新增命令，P2） |

---

## 12. 实施阶段

| 阶段 | 内容 | 验收 |
| --- | --- | --- |
| P1 骨架 | `Shell/TitleBar/NavRail/StatusBar` + tokens + i18n loader + SendView 静态布局 | 窗口渲染、切换页面、语言 key 全来自 XML |
| P2 数据 | store + api + SendView（拖放/暂存/设备/IP 直连）+ 状态栏 | 拖入文件可发送到 WSL/本机 |
| P3 接收/历史 | ReceiveView（进度/续传横幅/最近）+ HistoryView | 续传时显示跳过数量；中断显示「未完成」 |
| P4 设置/多端 | SettingsView + 深浅色 + 窄屏 Tab + 焦点/对比度检查 | 375/640/1024/1440 无溢出；键盘可走完主流程 |
| P5 打磨 | 空态/错误态/长路径/无设备/防火墙提示 | ui-ux-pro-max 预交付清单逐项通过 |

每个阶段结束跑：`npm run build`、`cargo test`、`clippy -D warnings`，并截图存档 `docs/ui/screens/`。

---

## 13. 待确认（请回复）

1. 桌面布局是否采用「左侧导航 + 两列内容」？（备选：顶部 Tab，更轻但导航层级弱）
2. 发送页设备面板放右侧常驻，还是设备列表放拖放区下方全宽？（移动端必然是下方全宽）
3. 接收页是否保留「未完成传输」横幅 + 历史页「已中断」两个入口？还是只保留历史页？
4. 设置页是否按「设备/传输/网络/外观/关于」五组？有无需要新增的项（如「发送后校验」默认值）？
5. 语言包用 XML（本方案）是否符合预期？（备选：JSON/`.resx`；XML 便于后续用对象存储/翻译平台导出）
