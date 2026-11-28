# LanPipe Android 端 Fluent 2 视觉与交互映射方案

> 文档状态：调研稿（2026-11-28），供评审。
> 范围：仅回答「LanPipe 的 Windows / Fluent 2 界面如何落到 Android 上」，并给出**可直接执行**的改动清单（文件 + 选择器 + 取值）。不改代码、不提交。
> 关联文档：`docs/UI-DESIGN.md`（交互与页面规格）、`AGENT.md` §16.7（材质分层、动效、移动端约定）、`src/styles/fluent-tokens.css`（token 事实源）。
> 视觉总原则（用户要求）：**Android 主题样式参考 Windows 应用（Fluent 观感），不照搬 Material**。Material 3 只借「系统行为」——edge-to-edge、窗口尺寸分级、返回手势、触摸目标、导航栏语义——不借「Material 皮肤」：无 ripple、无 FAB、无 tonal elevation、默认关闭 Material You 动态取色。

---

## 0. 结论摘要（TL;DR）

| 决策点 | 结论 | 理由 |
| --- | --- | --- |
| 导航形态 | **底部 Tab 栏，5 项**（发送/接收/历史/诊断/设置），高 56dp + 安全区 | M3 Navigation bar 允许 3–5 个目的地；390dp/5=78dp ≥ 48dp 触摸下限；与 `AGENT.md` §16.9「桌面与移动都是 5 项」一致 |
| 材质替代 | **纯色分层为主 + 玻璃层为辅**：页底/卡片/浮层用不透明 token；仅标题栏与 Tab 栏在 `backdrop-filter` 可用时用半透明 + blur | Android 无 Mica/Acrylic 真材质；RenderEffect 真模糊需 Kotlin 插件（API 31+），列为阶段 B 可选，不进 v1 |
| 色值来源 | 100% 复用 `src/styles/fluent-tokens.css` 的 `--bg/--surface/--accent/...`，Android 只**新增覆盖块**，不新建调色板 | 一套 token 双端一致，避免"安卓版看起来像另一个产品" |
| 圆角 | 控件 4dp / 卡片 8dp / 浮层 8dp / 底部 Sheet 顶部 16dp / 胶囊 999dp | Fluent 2 形状规范（4/2/8/12）；Sheet 16dp 是 Fluent 12dp 与 M3 28dp 的折中，28dp 太"Material" |
| 字号 | 正文 14sp/20sp、页标题 22sp/28sp（平板 26sp/32sp）、卡片标题 15sp/20sp、导航标签 11sp | 中文正文桌面 13px 在手机偏小；Fluent 2 Body 1 = 14/20，正好可用 |
| 边到边 | `targetSdk 35` 起系统强制 edge-to-edge，必须做 insets：Kotlin 插件读 `WindowInsets` → CSS 变量 `--android-inset-top/bottom` → `.app-root` 三行栅格 | Android 15/16 不再允许退出 edge-to-edge；`env(safe-area-inset-*)` 在 WebView 里值不可靠，需原生量测 |
| 文件选择 | SAF：`ACTION_OPEN_DOCUMENT`（多选文件）+ `ACTION_OPEN_DOCUMENT_TREE`（文件夹），**不用** `MANAGE_EXTERNAL_STORAGE` | 现有 `actions.ts` 的 `open()` 已是 Tauri dialog 插件，Android 走 SAF；无存储权限弹窗 |
| 桌面元素 | 隐藏窗口按钮/拖动区/双击最大化/状态栏/材质开关/开机自启/防火墙/更新检查；`info-pop` 的 IP 信息合并进诊断页 | 桌面专有能力或 Android 无对应概念 |

---

## 1. 技术前提：Tauri v2 Android 上「能改什么」

### 1.1 渲染路径

LanPipe 前端是原生 DOM + CSS 变量（无框架），Tauri v2 Android 中运行在**系统 WebView（Chrome/Blink）**里。因此：

- **视觉 95% 复用现有 CSS**：`tokens.css` / `fluent-tokens.css` / `materials.css` / `components.css` / `views.css` / `motion.css` 全部可沿用，只需要一个 Android 覆盖层文件。
- **单位换算**：`<meta name="viewport" content="width=device-width, initial-scale=1">` 下，**1 CSS px = 1 dp**（Android WebView 的 CSS px 就是 dp，这点和 iOS 的 pt 一致）。所以桌面 token 的 4/8/16px 在 Android 直接写成 4/8/16 即可，不乘 2、不除 dpr。
- **字体缩放**：系统「字体大小」默认不改变 WebView 的 CSS px，需要在 Kotlin 侧读 `Configuration.fontScale` 并设 `webView.settings.textZoom = fontScale * 100`，同时在 CSS 里写 `-webkit-text-size-adjust: 100%` 防止二次放大。

### 1.2 能力对照（桌面有、Android 没有）

| 桌面能力 | 现状实现 | Android 现实 | 处理 |
| --- | --- | --- | --- |
| Mica/Acrylic 真材质 | Rust `Win::set_effects` + `transparent(true)` | 不存在 | 半透明叠层 / 纯色分层（§3.2） |
| 无边框窗口 + 自绘窗口按钮 | `.win-controls`、`data-tauri-drag-region` | 系统窗口，无边框概念 | 隐藏（§6.7） |
| 双击标题栏最大化 | `shell.ts` dblclick | — | 不绑定 |
| 拖入文件 | `onDragDropEvent` + `.drag-over` | 无拖放 | SAF 选择器（§5.4） |
| 托盘 / 关闭进托盘 | Rust tray | 无托盘；Android 用前台服务通知 | 阶段 D（§5.6） |
| 开机自启 | HKCU Run | 无等价物（且不应做） | 隐藏设置项 |
| 防火墙放行 | `firewall_*` 命令 | 无 | 隐藏设置项 |
| 标题栏材质下拉 | 设置页 `backdrop_apply` | 无材质可切 | 隐藏设置项 |
| `backdrop-filter` | 全部落 `materials.css` | WebView 支持，但低端机 GPU 代价高 | 特性检测 + 仅 Tab 栏/标题栏 |
| 真模糊（RenderEffect） | — | API 31+ 可做，需 Kotlin 插件 | 阶段 B 可选，v1 不做 |

### 1.3 改动面分三层

1. **CSS 层**（新增 `src/styles/android.css` + 少量选择器覆盖）：占比 70%，纯前端，最低风险。
2. **TS 组件层**（`shell.ts` 分支、`src/platform.ts`、`src/ui/sheet.ts`、`main.ts` 返回栈）：占比 25%。
3. **Kotlin/Rust 原生层**（insets 插件、前台服务、分享 intent、FileProvider）：占比 5%，但依赖最重，独立排期。

---

## 2. 系统栏与 edge-to-edge

### 2.1 目标

- Android 15（API 35）起 **edge-to-edge 强制**：应用内容默认延伸到状态栏与导航栏后面，`setDecorFitsSystemWindows(true)` 的退出方式在 Android 16 被移除。参考：[Edge-to-edge 官方指南](https://developer.android.com/develop/ui/views/layout/edge-to-edge)、[Android 15 行为变更](https://developer.android.com/about/versions/15/behavior-changes-15)。
- 自绘标题栏要**压在状态栏下方**（不与系统状态栏内容重叠），Tab 栏要**抬到手势导航条上方**。
- 深色/浅色主题下状态栏图标明暗必须跟随（浅色主题 → 深色图标）。

### 2.2 insets 数据链路

```
Kotlin: MainActivity
  WindowCompat.setDecorFitsSystemWindows(window, false)   // 或 enableEdgeToEdge()
  ViewCompat.setOnApplyWindowInsetsListener(root) { v, insets ->
      val i = insets.getInsets(WindowInsetsCompat.Type.systemBars() or ime())
      plugin.emit("android:insets", {top=i.top, bottom=i.bottom, ime=ime.bottom})
  }
        │  Tauri event
        ▼
TS: src/ui/android-insets.ts
  invoke("android_insets")          // 首帧同步取值，避免闪一下
  listen("android:insets", ...)     // 旋转/键盘/手势条切换时更新
        │  px = dp（1:1）
        ▼
CSS: documentElement.style.setProperty("--android-inset-top", `${top}px`)
     documentElement.style.setProperty("--android-inset-bottom", `${bottom}px`)
```

**为什么要自己量测而不是只用 `env()`**：Android WebView 的 `env(safe-area-inset-*)` 依赖 `viewport-fit=cover` 与 WebView 版本，实测在部分 ROM/横屏下返回 0；同时键盘（ime）高度、三键导航（48dp）与手势导航（24dp）差异只能靠 `WindowInsets` 拿到。`env()` 只作为**降级兜底**。

### 2.3 CSS 落地

在 `src/styles/android.css`（新文件，见 §6.2）：

```css
:root[data-platform="android"] {
  --android-inset-top: 0px;     /* JS 写入 */
  --android-inset-bottom: 0px;  /* JS 写入 */
  --android-titlebar-h: 44px;   /* Fluent 移动端页头 */
  --android-tabbar-h: 56px;
}

:root[data-platform="android"] .app-root {
  /* 三行栅格：标题栏 / 内容 / Tab 栏，行高各自含安全区 */
  grid-template-rows:
    calc(var(--android-titlebar-h) + var(--android-inset-top))
    1fr
    calc(var(--android-tabbar-h) + var(--android-inset-bottom));
  grid-template-columns: 1fr;
  grid-template-areas: "titlebar" "content" "nav";
  height: 100dvh;      /* 不用 100vh，避免地址栏/手势条换算抖动 */
  border-radius: 0;    /* 系统窗口没有圆角，app-root 的 8px 裁切要去掉 */
}

:root[data-platform="android"] .titlebar {
  height: calc(var(--android-titlebar-h) + var(--android-inset-top));
  padding-top: var(--android-inset-top);
  padding-left: 16px;
  padding-right: 16px;
}

:root[data-platform="android"] .nav {
  height: calc(var(--android-tabbar-h) + var(--android-inset-bottom));
  padding-bottom: var(--android-inset-bottom);
}
```

> 降级：若 insets 插件未就绪，`--android-inset-*` 的初始值可取
> `max(env(safe-area-inset-top, 0px), 24px)` / `max(env(safe-area-inset-bottom, 0px), 24px)`（手势导航条常见 24dp）。

### 2.4 状态栏/导航栏图标明暗

Kotlin 侧跟随 WebView 当前主题（JS 通知一次即可）：

```kotlin
WindowInsetsControllerCompat(window, decorView).apply {
  isAppearanceLightStatusBars = light          // 浅色主题 → 深色图标
  isAppearanceLightNavigationBars = light
}
window.statusBarColor = Color.TRANSPARENT
window.navigationBarColor = Color.TRANSPARENT
```

JS 侧在 `watchTheme()` 的写回钩子里 `invoke("android_set_bars", { light })`。
**不要**在 `index.html` 里用 `<meta name="theme-color">` 控制状态栏底色——edge-to-edge 下状态栏是透明的，只有图标明暗有意义。

### 2.5 返回手势与返回键

现状风险：`main.ts` 的 `navigate()` 是纯函数式 replaceChildren，**没有历史栈**，Android 返回键会直接把 Activity 退出（丢当前页/丢正在传的可见性）。

设计（三层，按需实现）：

```
系统返回 / 左缘右滑（predictive back，Android 14+）
        │
        ├─ 有打开的浮层/Sheet/下拉 → 关闭浮层（不退出页面）
        ├─ 路由栈 > 1（如 设置→历史 的跨页跳转）→ 回上一页
        ├─ 当前不是「发送」→ 回发送页（一级页兜底）
        └─ 在发送页 → 2 秒内再按一次才退出（toast：再按一次退出）
```

实现要点：

1. `src/main.ts` 增加 `const routeStack: Route[] = ["send"]`，`navigate(route, push = true)` 时入栈；`navigate(back)` 时出栈（`push=false`）。
2. 在 `index.html`/`main.ts` 加 `window.addEventListener("popstate", ...)`，`navigate` 时 `history.pushState({route}, "")`——这样 WebView 内的 `history.back()` 能吃掉一次返回，**Kotlin 侧不必拦截也能生效**（WebView 有历史时返回键默认走 `canGoBack()`）。
3. Android 14+ predictive back：`AndroidManifest.xml` 的 `<application android:enableOnBackInvokedCallback="true">`，Kotlin 侧 `onBackInvokedDispatcher` 派发到 JS（`window.dispatchEvent(new CustomEvent("android:back"))`），JS 决定 `history.back()` 还是 `invoke("android_exit")`。参考：[Predictive back gesture](https://developer.android.com/guide/navigation/custom-back/predictive-back-gesture)。
4. 键盘可见时返回键应先收键盘：Kotlin 先关 IME，再走上面的链路。

Tab 切换**不**进历史栈（与 M3 Navigation bar 语义一致：Tab 是平级目的地，返回只在「跨页 drill-down」时生效）。

---

## 3. 视觉映射：没有 Mica/Acrylic 时如何保住 Fluent 观感

### 3.1 原则

Windows 的 Fluent 观感由四件事支撑：**分层深度（材质）→ 克制圆角 → 细边框 + 轻阴影 → 精确 13/14px 字号与 100ms 动效**。
Android 拿不到材质，就靠「**不透明分层 + 1px 细描边 + Fluent 阴影 + 更紧的信息密度**」来复刻，而不是靠 M3 的 tonal elevation（阴影 + 表面染色，会让界面发灰发"Material"）。

一句话：**Windows 上"背景是玻璃"，Android 上"背景是纸"——但纸的层数、边框、圆角、阴影必须和 Windows 完全一致。**

### 3.2 四种材质替代方案与选型

| 方案 | 做法 | 成本 | 效果 | 结论 |
| --- | --- | --- | --- | --- |
| T0 纯色分层 | 页底 `--bg`，卡片/浮层 `--surface` + 1px `--border` + Fluent 阴影 | 0 | 最稳，零性能风险，观感"Windows 10 关透明效果" | **v1 默认** |
| T1 半透明叠层 + `backdrop-filter` | `rgba(surface,.86)` + `backdrop-filter: blur(40px) saturate(1.6)`，仅标题栏/Tab 栏 | 低（纯 CSS + 特性检测） | 接近 Acrylic，滚动时能感知壁纸/内容 | **v1 开启，限 2 处** |
| T2 RenderEffect 真模糊 | Kotlin 插件对 View 做 `RenderNode.setRenderEffect(RenderEffect.createBlurEffect(...))`（API 31+） | 中高（原生插件 + 与 WebView 合成层交互复杂） | 最接近系统级 Mica | **阶段 B 可选，不进 v1** |
| T3 Material You 动态取色 | 读取壁纸主色，仅用于 `--bg` 轻微着色（饱和度 ≤8%），accent 仍用 Fluent `#005FB8` | 中（Kotlin `dynamicColors`） | 更"安卓"，但会稀释 Windows 观感 | **默认关闭**，设置页给「跟随壁纸」开关（可选） |

约束（沿用 §16.7.2 的禁令，Android 同样适用）：
- 相邻两个 `backdrop-filter` 层不得相接（blur 边界会露出接缝）；卡片**永远不加** `backdrop-filter`（还会破坏 `§16.7.7` 的层叠上下文约束）。
- 卡片/列表**不得加 `transform`**（同上，`box-shadow` 与 hover 卡片动画依赖层叠上下文）。

### 3.3 分层色值表（Windows → Android）

| 层 | Windows 实现（§16.7.2） | Android 浅色 | Android 深色 | 无 blur 兜底 |
| --- | --- | --- | --- | --- |
| 窗口底 / 页面 | Mica 壁纸采样 | `#F5F5F5`（`--bg`） | `#141414`（`--bg`） | 同左 |
| 标题栏 | `surface` 74% + blur30 | `rgba(245,245,245,.92)` + blur(24px) | `rgba(20,20,20,.92)` + blur(24px) | `#F5F5F5` / `#141414` + 底部 1px `--border` |
| Tab 栏（底部导航） | 左导航 90% + blur30 | `rgba(255,255,255,.86)` + blur(40px) | `rgba(41,41,41,.86)` + blur(40px) | `#FFFFFF` / `#292929` + 顶部 1px `--border`（深色用 `rgba(255,255,255,.08)`） |
| 内容区 | `--bg` 80% | 直接 `--bg`（透明层会让滚动内容透出，Android 不需要） | 同 | — |
| 卡片 | `--surface` 92% | `#FFFFFF`（`--surface`） | `#292929`（`--surface`） | 已是兜底 |
| 列表行 / 设备卡 | `--surface` 80% | `#FFFFFF`，hover 态改按压态（§5.1） | `#292929` | — |
| 浮层（Toast/下拉/Info） | 100% `--surface` + shadow16 | `#FFFFFF` + `--shadow-flyout` | `#292929` + `--shadow-flyout` | — |
| 底部 Sheet | —（桌面无） | `#FFFFFF`，顶部 16dp 圆角，`0 -8px 24px rgba(0,0,0,.16)` | `#292929`，同上 | — |
| 遮罩 Scrim | — | `rgba(0,0,0,.32)` | `rgba(0,0,0,.56)` | — |
| 分割线 | `--border` | `#E0E0E0` | `#525252` | — |
| 强调色 | `--accent` | `#005FB8`（按压 `#004F99`） | `#4CC2FF`（按压 `#3AA7E0`） | — |

> 说明：Android 的「卡片在页面上」与 Windows 桌面一致——浅色页面 `#F5F5F5` + 白色卡片 + `#E0E0E0` 细边，这是本方案保住 Fluent 观感最关键的一对色；**不要**改成 M3 的 `surfaceContainerLow` 一类带色调的灰。

### 3.4 Android token 覆盖块（可直接粘贴）

```css
/* src/styles/android.css —— 只被 <html data-platform="android"> 命中 */
:root[data-platform="android"] {
  /* 尺寸 */
  --android-titlebar-h: 44px;
  --android-tabbar-h: 56px;
  --android-control-h: 44px;      /* 视觉高度，命中区 48px（§5.1） */
  --android-list-row-h: 56px;
  --android-page-pad: 16px;       /* UI-DESIGN §4.5 */
  --android-gap: 12px;

  /* 玻璃（T1） */
  --android-glass-titlebar: rgba(245, 245, 245, .92);
  --android-glass-tabbar: rgba(255, 255, 255, .86);
  --android-glass-blur-titlebar: blur(24px) saturate(1.4);
  --android-glass-blur-tabbar: blur(40px) saturate(1.6);

  /* 交互态（无 hover，改按压） */
  --android-state-pressed: rgba(0, 0, 0, .05);
  --android-state-pressed-strong: rgba(0, 0, 0, .09);

  /* 浮层 */
  --android-sheet-radius: 16px;
  --android-scrim: rgba(0, 0, 0, .32);
  --android-sheet-shadow: 0 -8px 24px rgba(0, 0, 0, .16);
  --android-tab-indicator: var(--accent-soft);
  --android-tab-indicator-w: 56px;
  --android-tab-indicator-h: 30px;
}

:root[data-platform="android"][data-theme="dark"] {
  --android-glass-titlebar: rgba(20, 20, 20, .92);
  --android-glass-tabbar: rgba(41, 41, 41, .86);
  --android-state-pressed: rgba(255, 255, 255, .06);
  --android-state-pressed-strong: rgba(255, 255, 255, .10);
  --android-scrim: rgba(0, 0, 0, .56);
  --android-sheet-shadow: 0 -8px 24px rgba(0, 0, 0, .32);
}

/* 系统为深色但用户未显式选主题时的兜底（沿用 fluent-tokens.css 的写法） */
@media (prefers-color-scheme: dark) {
  :root[data-platform="android"]:not([data-theme="light"]) {
    --android-glass-titlebar: rgba(20, 20, 20, .92);
    --android-glass-tabbar: rgba(41, 41, 41, .86);
    --android-state-pressed: rgba(255, 255, 255, .06);
    --android-state-pressed-strong: rgba(255, 255, 255, .10);
    --android-scrim: rgba(0, 0, 0, .56);
    --android-sheet-shadow: 0 -8px 24px rgba(0, 0, 0, .32);
  }
}

/* 只有检测到 blur 能力时才启用玻璃，否则保持 T0 纯色 */
:root[data-platform="android"][data-blur="0"] .titlebar {
  background: var(--bg) !important;
  backdrop-filter: none !important;
}
:root[data-platform="android"][data-blur="0"] .nav {
  background: var(--surface) !important;
  backdrop-filter: none !important;
}
```

### 3.5 圆角（dp）

| 元素 | 桌面 token | Android | 依据 |
| --- | --- | --- | --- |
| 控件（按钮/输入框/下拉/步进器） | `--radius-control` 4px | **4dp**（<32dp 的小元素可 2dp） | Fluent 2 shapes：默认 4，<32px 用 2，大/超大 8/12 |
| 卡片 | `--radius-card` 8px | **8dp** | 同上 |
| 浮层/下拉/Toast/Info | `--radius-overlay` 8px | **8dp** | 同上 |
| 底部 Sheet | — | **顶部 16dp**（底部 0） | Fluent 大圆角 12dp 与 M3 28dp 折中；满宽 Sheet 用 8dp 会显生硬 |
| 图标按钮态容器（导航指示条） | — | **15dp**（30dp 高胶囊） | M3 active indicator 的胶囊形，但用 Fluent 的 `accent-soft` 实色，不用 M3 的 `secondaryContainer` |
| 胶囊 | `--radius-pill` 999 | **999** | 不变 |
| Sheet 把手 | — | 32×4dp，`--border-strong` | 告知可下滑关闭 |

> 严禁把 M3 的 28dp 大圆角带到卡片/按钮上——那是最容易被一眼认出「换了系统」的地方。

### 3.6 字号与字体

**字体栈**（`--font` 已包含所需项，Android 上会顺序命中）：

```css
/* 实际生效：无 Segoe UI → Roboto（拉丁/数字）→ Noto Sans CJK SC（中文） */
--font: 'Segoe UI', Roboto, 'Noto Sans CJK SC', 'Noto Sans SC', 'Source Han Sans SC', sans-serif;
```

Android 系统就是 **Roboto + Noto Sans CJK**，与 Windows 的 Segoe UI + 微软雅黑观感接近（同属几何/人文无衬线、字面高度接近）；**不要**打包 Segoe UI Variable（体积、授权、且 Android 上反而不协调）。

**字号表（sp，1 CSS px = 1 sp 的前提是 textZoom=100）**：

| 用途 | 桌面现值 | Android 手机 | Android ≥600dp | 说明 |
| --- | --- | --- | --- | --- |
| 正文/列表主文本 | 13/20 | **14sp / 20sp** | 14sp / 20sp | Fluent 2 Body 1 = 14/20；中文 13 在手机上偏小 |
| 页标题 | 26/32 | **22sp / 28sp** | 26sp / 32sp | 390dp 下 6 个汉字标题不换行 |
| 卡片标题 | 13/20 | **15sp / 20sp** | 15sp / 20sp | 与正文区分度靠字重 500–600 |
| 次要/说明文字 | 12 | **13sp / 18sp** | 13sp / 18sp | 触摸屏上 12sp 中文可读性差 |
| 按钮文字 | 13 | **14sp / 20sp** | 14sp / 20sp | 字重 500 |
| 导航标签 | 11 | **11sp / 16sp** | 12sp / 16sp | M3 Navigation bar label medium 12sp；11sp 保 390dp 五等分不折行 |
| 诊断数值 | 12 tabular | **13sp** tabular | 13sp | 保留 `font-variant-numeric: tabular-nums` |
| Toast | 12 | **13sp / 18sp** | 13sp | — |

**CJK 字重陷阱**：Noto Sans CJK 只有 400/500/700，现有 CSS 里的 `font-weight: 600` 在安卓中文环境会被合成或命中 700，标题视觉过粗。建议：

```css
:root[data-platform="android"] .page-title,
:root[data-platform="android"] .card-title,
:root[data-platform="android"] .name,
:root[data-platform="android"] .field-label { font-weight: 500; }
:root[data-platform="android"] .page-title { font-weight: 600; } /* 仅标题保留 600，可接受 */
```

**字距**：Fluent 标题用 `letter-spacing: -0.01em` 是给拉丁字母的；中文会变挤，Android 上归零：`:root[data-platform="android"] .page-title { letter-spacing: 0; }`。

### 3.7 阴影 / "elevation"

Fluent 阴影是**中性色细阴影**，M3 elevation 是**大范围软阴影 + 表面染色**。Android 上直接复用 token，不引入 M3：

| 层级 | 桌面 token | Android 用法 |
| --- | --- | --- |
| 卡片静止 | `--shadow-card` | `.card`（保持） |
| 卡片按压/聚焦 | `--shadow-card-hover` | 仅带鼠标的平板（`@media (hover: hover)`） |
| 浮层 | `--shadow-flyout` | `.dd-pop` / `.toast` / `.info-pop` |
| 底部 Sheet | — | `var(--android-sheet-shadow)` |
| 页面顶部（Tab 栏） | — | `0 -1px 0 var(--border)`（不投影；投影会让底部栏显"浮起"） |

### 3.8 深浅色与「跟随系统」

- 现有 `ui/theme.ts` 的 `system | light | dark` 三态在 Android 上语义不变；默认值应改为 **system**（桌面默认也是 system，无需改逻辑，只在设置页文案上强调「跟随系统」）。
- WebView 必须关闭强制深色，否则 Blink 的 auto-dark 会二次反转我们的 CSS：
  ```kotlin
  if (Build.VERSION.SDK_INT >= 29) {
      webView.settings.isForceDarkAllowed = false
  }
  ```
  并用 `AppCompatDelegate.setDefaultNightMode(MODE_NIGHT_FOLLOW_SYSTEM)` 让 `prefers-color-scheme` 正确反映系统。
- Material You（动态取色）默认关闭：M3 的 `dynamicLightColorScheme` 会把 `--accent` 换成壁纸色，直接破坏 Windows 观感。若做 T3，仅允许它影响 `--bg`（饱和度上限 8%、明度对齐现有 token），accent 恒为 `#005FB8` / `#4CC2FF`。参考：[Material You 动态取色](https://m3.material.io/styles/color/dynamic-color/overview)、[Android 动态颜色实现](https://developer.android.com/develop/ui/views/theming/dynamic-colors)。

### 3.9 明确「不照搬」的 Material 元素

| Material 3 元素 | LanPipe Android 决策 | 替代 |
| --- | --- | --- |
| Ripple 波纹 | 不用 | 按压色变化（`--android-state-pressed`）+ 100ms |
| FAB | 不用 | 主操作留在页面内（`.btn.primary`），发送页是「选择文件」大按钮 |
| M3 Navigation bar（80dp 高、tonal 容器） | 不用 | 56dp 高 Fluent Tab 栏 + 玻璃底 + `accent-soft` 指示胶囊 |
| 28dp 大圆角 Sheet | 不用 | 16dp 顶部圆角 |
| Tonal elevation（表面染色） | 不用 | 1px 边框 + Fluent 阴影 |
| M3 图标（Material Symbols） | 不用 | 沿用现有 SVG 图标（20px 描边图标），Tab 图标用 24dp 版本 |
| M3 色彩角色（primaryContainer…） | 不用 | 沿用 `--accent/-soft/--surface-alt` 等 Fluent token |
| Predictive back 动画 | 用（系统行为） | 系统提供，我们只需支持返回链路 |

---

## 4. 布局改造（390×844 竖屏）

### 4.1 窗口尺寸分级

采用 M3 window size classes 做断点（与现有 CSS 断点对齐），参考：[Window size classes](https://m3.material.io/foundations/layout/applying-layout/window-size-classes)：

| 级别 | 宽度 dp | 导航 | 布局 |
| --- | --- | --- | --- |
| Compact | < 600 | 底部 Tab 栏（5 项） | 单列，页边距 16 |
| Medium | 600–839（平板竖屏/手机横屏） | 左侧 Rail（图标 + 短标签，宽 72–80dp） | 单列 + 内容最大宽 720 |
| Expanded | ≥ 840（平板横屏） | 左侧 Rail 展开态（80dp） | 发送页恢复两列（主区 + 320dp 设备面板） |

现有断点是 `640`（`<640` 移动）与 `1024`（两列）。建议**保留 640 不动**（`<640` 已是底部 Tab 的既有实现），新增 Android 专属 `600` 用于「是否 Rail」会与 640 打架——结论：
- **Android 上统一用 600/840 两个 M3 断点**，通过 `:root[data-platform="android"]` 选择器覆盖已有 640 规则（§6.3），桌面继续用 640/1024，两套断点互不影响。
- 平板横向 840–1024 用"两列 + Rail"，≥1024 才与桌面完全一致（届时可复用桌面布局）。

### 4.2 导航形态决策

| 方案 | 优点 | 缺点 | 结论 |
| --- | --- | --- | --- |
| 底部 Tab 栏（5 项） | 单手拇指可达；M3 官方导航栏语义；现有 `<640` 已实现 `.nav` 横向；切换零成本 | 5 项 = 每项 78dp，拥挤；占用 56dp 高度 | **采用** |
| 抽屉（Navigation Drawer） | 省空间 | 需要汉堡按钮 + 两次点击；5 个目的地不值得；Fluent 桌面左导航的直译，但移动端交互退步 | 不采用 |
| 顶部下拉（Dropdown/Spinner） | 极省空间 | 与「页面标题」争夺位置；隐藏导航、降低可发现性；桌面感过强（Windows 下拉） | 不采用（仅用于设置页里的选项选择） |
| 底部 Tab 4 项 + 诊断进设置 | 每项 97dp 更宽松 | 与桌面 5 项不一致，诊断入口变深 | 备选 Plan B：若 320dp 设备（每项 64dp）溢出，则采用 |

**采用 A：5 项底部 Tab**。理由：
1. M3 Navigation bar 明确 3–5 个目的地，5 是上限但合法；
2. `AGENT.md` §16.9 已写死「诊断页在桌面 5th 导航项、移动底部 Tab 5 项」，改动它要连带改设计文档；
3. 390dp / 5 = 78dp，减去左右 8dp 内边距仍有 62dp 命中区，`≥48dp` 触摸下限达标；标签 2 个汉字（11sp ≈ 22dp 宽）不折行。

规格：

```
Tab 栏：高 56dp + 底部安全区；背景 glass(--android-glass-tabbar)+blur40（无 blur → --surface）
        顶部 1px --border；5 个 .nav-item 等分 flex:1
图标：24×24dp SVG，`fill:none;stroke:currentColor;stroke-width:1.6`（沿用现有 icons，等比放大）
指示胶囊：56×30dp，圆角 15dp，--android-tab-indicator(accent-soft)，
         未选中图标/标签 = --text-secondary，选中 = --accent
标签：11sp/16sp，选中 font-weight 500
切换动效：250ms 水平滑入（沿用 .view-slide-forward/back），禁用卡片级联（§6.3）
```

### 4.3 Wireframe

#### 4.3.1 发送页（390×844）

```
┌────────────────────────────────────┐
│ ░░░░░ 状态栏安全区 (inset-top 24) ░░░│
├────────────────────────────────────┤
│ LanPipe                        (i) │ 44dp 标题栏
├────────────────────────────────────┤
│ 发送                               │ 22sp
│ 选择要发送的内容 · 支持多选         │ 13sp 次要
│ ┌────────────────────────────────┐ │
│ │  ┌──────────────────────────┐  │ │
│ │  │  ⬆  选择文件              │  │ │ 52dp 主按钮（SAF 多选）
│ │  └──────────────────────────┘  │ │
│ │  ┌──────────────────────────┐  │ │
│ │  │  ▤  选择文件夹            │  │ │ 52dp 次按钮（SAF 目录）
│ │  └──────────────────────────┘  │ │
│ │  目录保留原结构                 │ │ 13sp 说明
│ └────────────────────────────────┘ │ card radius 8
│ 待发送 · 3 项 · 4.2 GB      清空    │
│ ┌────────────────────────────────┐ │
│ │ ▤ Minecraft1.21Fabric      ⋯   │ │ 56dp 行（长按/⋯ → Sheet）
│ │   文件夹 · 2,954 项 · 3.9 GB   │ │ 13sp 次要
│ │ ▤ mods                     ⋯   │ │
│ │ ▤ README.txt               ⋯   │ │
│ └────────────────────────────────┘ │
│ 附近设备                     刷新   │
│ ┌────────────────────────────────┐ │
│ │ ▣ WIN-2H1782KNJDF        [发送]│ │ .device：56dp 行 + 44dp 按钮
│ │   #005 · 在线                  │ │
│ └────────────────────────────────┘ │
│ ┌────────────────────────────────┐ │
│ │ ▣ WSL-Ubuntu             [发送]│ │
│ └────────────────────────────────┘ │
│ IP 直连                            │
│ ┌──────────────┐ ┌──────────────┐ │
│ │ 只输短 ID 005 │ │    连接      │ │ 输入 44dp / 按钮 44dp
│ └──────────────┘ └──────────────┘ │
├────────────────────────────────────┤
│  发送    接收    历史    诊断   设置│ 56dp Tab 栏
│ ░░░░░ 手势导航条安全区 ░░░░░░░░░░░░│
└────────────────────────────────────┘
```

#### 4.3.2 接收页

```
┌────────────────────────────────────┐
│ 接收                               │
│ 本机 · WSL-Ubuntu · 端口 53311      │
│ ┌ 未完成传输 ────────────────────┐ │
│ │ 863/2,954 · 29%                │ │ .banner（accent-soft 底）
│ │ 等待发送方继续   [ 清除记录 ]  │ │
│ └────────────────────────────────┘ │
│ ┌ 接收中 · 86 MB/s ─────────────┐ │
│ │ ████████████░░░░░░  (8dp 高)   │ │
│ │ 1,842 / 2,954 文件             │ │
│ │ 2.4 / 4.2 GB · 还剩 12 秒      │ │
│ │        [ 打开接收文件夹 ]       │ │ 44dp 按钮
│ └────────────────────────────────┘ │
│ 保存到                             │
│ Downloads/LanPipe            [更改]│
│ 最近接收                   全部 ›  │
│ ┌────────────────────────────────┐ │
│ │ ▤ Minecraft1.21Fabric    完成  │ │
│ │ ▤ 1.12.2                 已中断│ │
│ └────────────────────────────────┘ │
├────────────────────────────────────┤
│  发送    [接收]   历史   诊断   设置│
└────────────────────────────────────┘
```

#### 4.3.3 历史页

```
┌────────────────────────────────────┐
│ 历史                               │
│ 全部 · 今天                        │
│ ┌────────────────────────────────┐ │
│ │ ● Minecraft1.21Fabric      ⌄   │ │ 56dp，点击展开
│ │   发送 · 2,954 项 · 3.9 GB     │ │
│ │   今天 14:32 · 完成            │ │
│ └────────────────────────────────┘ │
│ ┌────────────────────────────────┐ │
│ │ ● 1.12.2                   ⌄   │ │
│ │   接收 · 128 项 · 240 MB       │ │
│ │   昨天 21:05 · 已中断           │ │
│ └────────────────────────────────┘ │
│  长按条目 → 移除记录 / 打开位置     │
├────────────────────────────────────┤
│  发送    接收   [历史]   诊断   设置│
└────────────────────────────────────┘
```

#### 4.3.4 设置页（Android 分组顺序）

```
┌────────────────────────────────────┐
│ 设置                               │
│ ┌ 设备 ──────────────────────────┐ │
│ │ 显示名        [ WSL-Ubuntu   ] │ │
│ │ 保存位置   [Downloads/LanPipe] │ │
│ │ 本机短 ID     #005  （只读）   │ │
│ └────────────────────────────────┘ │
│ ┌ 传输 ──────────────────────────┐ │
│ │ 并发流数   [−] 4 [+]           │ │ 44dp 步进
│ │ 压缩策略   [ 关闭 ▾ ]          │ │ → 底部 Sheet
│ └────────────────────────────────┘ │
│ ┌ 网络 ──────────────────────────┐ │
│ │ UDP 端口   [−] 53310 [+]       │ │
│ │ TCP 端口   [−] 53311 [+]       │ │
│ │ 自动发现   [● 开]              │ │
│ └────────────────────────────────┘ │
│ ┌ 外观与语言 ─────────────────────┐ │
│ │ 主题       [ 跟随系统 ▾ ]      │ │
│ │ 语言       [ 简体中文 ▾ ]      │ │
│ │ 跟随壁纸取色  [○ 关]  (可选)   │ │
│ └────────────────────────────────┘ │
│ ┌ 关于 ──────────────────────────┐ │
│ │ 版本 0.1.2 · 开源协议           │ │
│ │ 检查更新                       │ │ → 改为应用商店/APK 手动更新
│ └────────────────────────────────┘ │
│           [   保存设置   ]         │ 吸底主按钮
├────────────────────────────────────┤
│  发送    接收    历史    诊断  [设置]│
└────────────────────────────────────┘
```

> 隐藏：`启动`（开机自启）整组、`网络` 组里的"防火墙状态 + 放行防火墙"行、桌面版"材质"下拉。

#### 4.3.5 诊断页

```
┌────────────────────────────────────┐
│ 诊断                        [刷新] │
│ ┌ 概览 ──────────────────────────┐ │
│ │ 版本            0.1.2          │ │ 13sp 两列 → 单列
│ │ 系统            Android 15     │ │
│ │ 本机 ID         #005           │ │
│ │ 设备 ID         d8f3…          │ │
│ └────────────────────────────────┘ │
│ ┌ 网络 ──────────────────────────┐ │
│ │ wlan0  192.168.1.42/24  #005   │ │
│ │ 监听    UDP 53310 / TCP 53311  │ │
│ │ 发现    开                     │ │
│ └────────────────────────────────┘ │
│ ┌ 附近设备 ────────────────────────┐ │
│ │ WIN-2H1782KNJDF  192.168.1.7    │ │
│ └────────────────────────────────┘ │
│        [    复制诊断信息     ]      │
├────────────────────────────────────┤
│  发送    接收    历史  [诊断]  设置 │
└────────────────────────────────────┘
```

#### 4.3.6 平板横屏（≥840dp）

```
┌────────────────────────────────────────────────────────────┐
│ LanPipe                                         (i)        │
├──────┬─────────────────────────────────────────────────────┤
│ 发送 │ 发送                                                │
│ ▣    │ ┌──────────────────────────┐ ┌───────────────────┐  │
│ 接收 │ │  ⬆ 选择文件               │ │ 附近设备    刷新  │  │
│ 历史 │ │  ▤ 选择文件夹             │ │ ┌───────────────┐ │  │
│ 诊断 │ └──────────────────────────┘ │ │ WIN-2H17… [发送]│ │  │
│ 设置 │ 待发送 · 3 项 · 4.2 GB       │ └───────────────┘ │  │
│      │ ┌──────────────────────────┐ │ ┌───────────────┐ │  │
│      │ │ ▤ Minecraft1.21Fabric    │ │ │ IP 直连        │ │  │
│      │ └──────────────────────────┘ │ └───────────────┘ │  │
├──────┴─────────────────────────────────────────────────────┤
```

### 4.4 每页改动清单（Android 竖屏）

| 页面 | 现状（`src/views/*.ts`） | Android 改动 |
| --- | --- | --- |
| 发送 | `.send-grid` 两列（`views.css` grid-areas），拖放区 `.dropzone` + `.card.staged` | ① `.send-grid` 单列（`<640` 已有，Android 覆盖到 `<600`）；② `.dropzone` 在 Android **整块替换**为 `.pick-panel`（两枚 52dp 大按钮）；③ 已选列表 `.staged-list` 取消内部滚动（`max-height:none`），跟页面一起滚；④ 设备列表 `.device-list` 取消 `max-height:264px`（嵌套滚动在触摸屏上是反模式）；⑤ `.device .send` 按钮 44dp；⑥ IP 直连输入 `inputmode="numeric"`、`enterkeyhint="go"`、按钮与输入同高 |
| 接收 | `.receive-view` 单列 960 宽、进度条 6dp、`.save-row` | ① 进度条高度 8dp（`.progress` / `.progress.large` 10dp）；② `.save-row` 折行：路径独占一行，按钮右对齐；③ "打开接收文件夹" → `revealPath` 在 Android 应改为 `ACTION_VIEW` 打开目录或弹 Sheet（§5.5）；④ 后台接收时页面顶部加 `.banner`「后台接收中」 |
| 历史 | `.list-row.history-row` 点击展开 `.history-detail` 两列 | ① 行高 56dp，右侧 `⌄` 换 20dp 图标；② `.history-detail` 单列（`<640` 已有）；③ 长按 → ActionSheet（移除记录/打开位置）；④ 空状态 `.empty-illustrated` 保留 |
| 诊断 | `.diag-row` 160px + 1fr 两列 | ① 单列（`<640` 已有，覆盖到 `<600`）；② 长值（设备 ID/路径）允许 `overflow-wrap:anywhere`；③ "复制诊断信息" 吸底 44dp；④ 桌面专属行（WebView2 版本、透明回退、backdrop）在 Android 隐藏 |
| 设置 | `.settings-grid` + `.grid-2` 两列卡片 | ① `.grid-2` 单列；② 分组顺序改为 设备/传输/网络/外观与语言/关于（去掉"启动"）；③ 下拉 `.dd` → 底部 Sheet 单选；④ 步进器 44dp 触控；⑤ 开关行高 48dp；⑥ 保存按钮吸底（`position: sticky; bottom: 0`） |

### 4.5 滚动与吸顶

- `.content` 是唯一滚动容器（Android 上禁用 `body` 滚动）：`overscroll-behavior-y: contain` 防止拉动刷新与页面回弹串台，`-webkit-overflow-scrolling: touch`（现代 WebView 已默认，可省略）。
- 页头（`.page-head`）在长页（设置/诊断）用 `position: sticky; top: 0; background: var(--bg); z-index: 10`，避免滚动后失去上下文；发送/接收页不需要（内容短）。
- 滚动条：Android 上隐藏（`scrollbar-width: none; ::-webkit-scrollbar { display: none }`），桌面保留 10px 细滚动条。

### 4.6 平板/横屏

- **手机横屏（高 < 480dp，宽 < 600dp）**：仍是 Compact，底部 Tab 不变，但内容区上下 padding 减到 12dp，`100dvh` 保证不出现双滚动条。
- **折叠屏/平板分屏（600–839dp）**：左侧 Rail（72dp，图标 + 10sp 短标签），内容区单列最大宽 720dp 居中；`.send-grid` 仍单列。
- **平板横屏（≥840dp）**：Rail 80dp + 发送页两列（主区 + 320dp `.card-devices` 常驻，`grid-template-areas: "head head" "drop devices" "staged direct"`），即复用桌面布局但左导航换 Rail（不是 76dp 桌面导航）。此时状态栏可不显示（`--statusbar-h: 0`）。
- 旋转/分屏不要重启页面：CSS 断点自适应即可；若切到 Rail 需重算 `.nav-indicator`，请在 `resize` 后下一帧重算（现有 `shell.ts` 已有 `matchMedia` 监听可扩展）。

---

## 5. 交互差异

### 5.1 触摸目标与按压反馈

| 项 | 桌面 | Android |
| --- | --- | --- |
| 最小触摸目标 | 32dp（`.btn` 高 32） | **48dp 命中区**，视觉可 44dp（M3 最小 48dp） |
| 列表行 | `.list-row` padding 8 | 行高 56dp（`--android-list-row-h`） |
| 图标按钮 | `.icon-btn` 32×32 | 44×44dp |
| 步进器 | 高 32，按钮宽 26 | 高 44，按钮宽 44 |
| 按压反馈 | `:hover` / `:active` 变色 | `:active` 变色 `--android-state-pressed`，100ms；**不用** `transform: scale()`（卡片/列表禁止 transform，§16.7.7） |

命中区扩展的通用写法（视觉 44、命中 48）：

```css
@media (pointer: coarse) {
  .btn { position: relative; height: 44px; }
  .btn::after { content: ""; position: absolute; inset: -2px 0; }
}
/* 用 ::after 扩大命中区，不影响视觉尺寸与层叠 */
```

### 5.2 没有 hover

- 现有大量 `:hover`（`.btn:hover`、`.card:hover`、`.list-row:hover`、`.device:hover`、`.dd-opt:hover`）在触摸设备上会**粘住**（点完保持高亮）。统一包裹：
  ```css
  @media (hover: hover) and (pointer: fine) {
    /* 桌面/带鼠标平板：保留原 hover 规则 */
  }
  ```
  落地方式：在 `android.css` 里把 Android 端的 hover 一律重置（`@media (pointer: coarse) { .card:hover { box-shadow: var(--shadow-card); } ... }`），桌面规则不动（改动面最小）。
- 触摸反馈层级：`--android-state-pressed`（列表行/卡片）→ `--android-state-pressed-strong`（按钮）→ `--accent-pressed`（主按钮）。

### 5.3 长按替代右键

桌面版本身没有右键菜单（无 context-menu 实现），Android 补的是「列表项操作」：

| 目标 | 长按行为 | Sheet 选项 |
| --- | --- | --- |
| 待发送文件行（`.staged-list .list-row`） | 打开 ActionSheet | 移除 / 清空全部 |
| 历史行（`.history-row`） | 打开 ActionSheet | 打开位置 / 移除记录 |
| 附近设备行（`.device`） | 打开 ActionSheet | 发送到该设备 / 复制地址 |
| 诊断值（设备 ID / 路径） | 系统文本选择 + Android 复制条 | —（`.path` 已 `user-select:text`） |

**ActionSheet 规格**（`src/ui/sheet.ts` 新组件，或扩展 `src/ui/dropdown.ts`）：

```
高度：内容高 + 底部安全区；顶部 16dp 圆角 + 32×4dp 把手
行：56dp，左图标 24dp + 13sp 文本；危险项（移除）文本用 --danger
遮罩：--android-scrim；点击遮罩 / 下滑 / 返回键关闭
动效：200ms ease-decelerate 上滑；`prefers-reduced-motion` 时淡入
层级：z-index 1400（高于 dd-pop 1300、toast 1200）
```

### 5.4 文件选择（拖放的替代）

| 桌面入口 | Android 实现 | 说明 |
| --- | --- | --- |
| 拖入窗口 | — | 不提供 |
| 点「选择文件」→ `open({multiple:true})` | `Intent.ACTION_OPEN_DOCUMENT`，`EXTRA_ALLOW_MULTIPLE=true`，`type="*/*"` | 现有 `actions.ts` `pickFiles()` 走 Tauri dialog 插件，Android 端底层就是 SAF；返回 `content://` URI |
| 点「选择文件夹」→ `open({directory:true})` | `Intent.ACTION_OPEN_DOCUMENT_TREE` | 同样走 SAF，可持久化授权 |
| 拖入后自动 stage | 选择后立即 `stagePaths()` | URI 到「路径」的转换需要落一次缓存（见下） |

**关键工程风险（必须与 Rust 侧一起排期）**：现有 `summarize_paths(paths)` / `start_send(target, paths)` 都假设**文件系统绝对路径**。SAF 返回的是 `content://com.android.providers…` URI，`Path::new(uri)` 无法读取。两条路：

1. **推荐 v1**：选完先在 Kotlin/JS 侧把选中的文件/目录复制到 app 私有缓存（`context.cacheDir/lanpipe-stage/`），把缓存绝对路径交给现有 Rust 逻辑；传输结束后清理。代价：大文件多一次拷贝（局域网 2GB 级文件可接受？需评估）。
2. **推荐 v2**：Rust 侧接入 `ContentResolver`（tauri 自定义 Android 插件，`openInputStream` + `ParcelFileDescriptor`），让 LP/1 传输直接读 URI。工作量较大，但避免拷贝。

设计上先按方案 1 出 UX（进度提示"准备中…"），Rust 侧方案另立 task。参考：[SAF 文档](https://developer.android.com/guide/topics/providers/document-provider)、[ACTION_OPEN_DOCUMENT](https://developer.android.com/reference/android/content/Intent#ACTION_OPEN_DOCUMENT)。

### 5.5 分享面板（ACTION_SEND / ACTION_VIEW）

| 方向 | Intent | 用途 | 需要 |
| --- | --- | --- | --- |
| 外部 → LanPipe | `ACTION_SEND`（`type="*/*"`，`EXTRA_STREAM` 多个） | 相册/文件管理器"分享到 LanPipe"，直接进入发送页并 stage 好 | Manifest `intent-filter` + Kotlin 读取 `EXTRA_STREAM` 后复制到 cache 再 `emit("android:share", paths)` |
| LanPipe → 外部 | `ACTION_VIEW`（`FileProvider.getUriForFile` + 推断 MIME） | 接收完成/历史条目"用其他应用打开" | `FileProvider`（`androidx.core`）+ `res/xml/file_paths.xml` |
| LanPipe → 外部 | `ACTION_SEND`（`EXTRA_STREAM`） | 长按历史条目"分享" | 同上 + `Intent.createChooser` |

接收页的"打开接收文件夹"在 Android 上语义不成立（SAF 没有资源管理器），应改为：
- 传输完成后：**用其他应用打开**（ACTION_VIEW，单文件时）/ **在文件管理器中查看**（`ACTION_VIEW` + `content://` 目录，或 Sheet 显示路径 + 复制路径）。

参考：[Sending simple data](https://developer.android.com/training/sharing/send)、[Receiving simple data](https://developer.android.com/training/sharing/receive)。

### 5.6 通知与前台服务

| 场景 | 方案 |
| --- | --- |
| 应用在前台接收 | 页面内进度 + 状态（现状） |
| 应用在后台接收 | **前台服务**（`ForegroundService`，type=`dataSync`），常驻通知显示"正在接收 · 42% · 86 MB/s"，`setOngoing(true)`、`setOnlyAlertOnce(true)`；进度刷新节流 ≥1s（Android 通知更新频率限制） |
| 接收完成 | 普通通知（`IMPORTANCE_DEFAULT` 带声音），`PendingIntent` 打开接收页；渠道 `transfer` |
| 通知权限 | Android 13+ 需运行时申请 `POST_NOTIFICATIONS`；被拒时降级：应用内显示"后台接收不可见"提示 |
| Android 14+ | 前台服务必须声明 `foregroundServiceType="dataSync"` 且有对应权限；`startForeground` 超时限制 |

参考：[通知渠道](https://developer.android.com/develop/ui/views/notifications/channels)、[前台服务](https://developer.android.com/develop/background-work/services/fgs)、[Android 14 前台服务变更](https://developer.android.com/about/versions/14/changes/fgs-types-required)。

国内 ROM 额外风险：省电策略杀后台 → 首次后台接收失败时引导「设置 → 电池 → 不限制」。

### 5.7 输入与键盘

- IP 直连输入框：`inputmode="numeric"`（数字键盘）、`enterkeyhint="go"`（回车键=连接）、`autocomplete="off"`、`autocapitalize="off"`。
- 键盘弹起时 insets 的 `ime` 值增大：`.content { padding-bottom: calc(16px + var(--android-ime, 0px)) }`，或让输入所在卡片 `scrollIntoView({block:"center"})`（现有 `withFocusPreserved` 已处理焦点，需补滚动）。
- 端口/流数输入：`type="text" inputmode="numeric"` + `pattern="[0-9]*"`，避免 `type=number` 的上下箭头与键盘不一致。
- 键盘可见时返回键先收键盘（§2.5 第 4 点）。

### 5.8 其它触摸细节

```css
:root[data-platform="android"] {
  -webkit-tap-highlight-color: transparent;   /* 去掉 WebView 默认蓝色点按方块 */
  -webkit-text-size-adjust: 100%;
}
:root[data-platform="android"] body {
  touch-action: manipulation;                 /* 去掉 300ms 双击缩放判定 */
  overscroll-behavior-y: none;                /* 禁止下拉刷新/页面回弹串台 */
}
```

---

## 6. 可执行改动清单

### 6.1 文件总表

| 动作 | 文件 | 内容 |
| --- | --- | --- |
| 新增 | `src/styles/android.css` | 平台 token + 安全区 + 组件覆盖 + hover 重置 + Tab 栏样式（§6.2） |
| 修改 | `src/styles/index.css` | 末尾 `@import "./android.css";` |
| 新增 | `src/platform.ts` | `platform` / `isTouch` / `applyPlatform()`（§6.4） |
| 新增 | `src/ui/android-insets.ts` | insets 订阅与 CSS 变量写入（§2.2） |
| 修改 | `src/layout/shell.ts` | 平台分支：隐藏窗口按钮/拖动、Tab 栏、返回可达性、Tab 指示条（§6.4） |
| 修改 | `src/main.ts` | 平台标记、insets 初始化、返回栈、blur 特性检测、`android:share` 监听（§6.5） |
| 新增 | `src/ui/sheet.ts` | 底部 Sheet / ActionSheet（§5.3） |
| 修改 | `src/ui/dropdown.ts` | Android 上 `.dd-pop` 改行为底部 Sheet（或复用 sheet.ts） |
| 修改 | `src/views/send.ts` | `.drag/pick` 区替换为 `.pick-panel`、设备行 ⋯ 入口、长按绑定 |
| 修改 | `src/views/receive.ts` | 后台接收 banner、打开文件夹 → ACTION_VIEW |
| 修改 | `src/views/settings.ts` | Android 隐藏组（启动/防火墙/材质/更新）、保存按钮吸底 |
| 修改 | `src/views/diagnostics.ts` | Android 隐藏 WebView2/backdrop 行 |
| 修改 | `index.html` | `viewport-fit=cover` |
| 修改 | `public/i18n/zh-CN.xml` | 新增 §6.8 的 key |
| 原生 | `AndroidManifest.xml` / `MainActivity.kt` / 新 Kotlin 插件 | §6.9 |

### 6.2 新增 `src/styles/android.css`（骨架，可直接落地）

```css
/* Android 平台覆盖层。只命中 <html data-platform="android">，桌面零影响。
   token 由 tokens.css/fluent-tokens.css 提供（index.css 先导入），这里只做覆盖。 */

:root[data-platform="android"] {
  --android-titlebar-h: 44px;
  --android-tabbar-h: 56px;
  /* 首帧兜底：插件返回真实 px 后由 JS 覆盖这两个变量（值单位均为 px=dp） */
  --android-inset-top: max(env(safe-area-inset-top, 0px), 24px);
  --android-inset-bottom: max(env(safe-area-inset-bottom, 0px), 24px);
  --android-control-h: 44px;
  --android-list-row-h: 56px;
  --android-page-pad: 16px;
  --android-glass-titlebar: rgba(245, 245, 245, .92);
  --android-glass-tabbar: rgba(255, 255, 255, .86);
  --android-state-pressed: rgba(0, 0, 0, .05);
  --android-state-pressed-strong: rgba(0, 0, 0, .09);
  --android-sheet-radius: 16px;
  --android-scrim: rgba(0, 0, 0, .32);
  --android-sheet-shadow: 0 -8px 24px rgba(0, 0, 0, .16);
  --android-tab-indicator: var(--accent-soft);
  -webkit-tap-highlight-color: transparent;
  -webkit-text-size-adjust: 100%;
}
/* 深色（显式 + 跟随系统两路，与 materials.css 写法一致） */
:root[data-platform="android"][data-theme="dark"] { /* …见 §3.4 变量… */ }
@media (prefers-color-scheme: dark) {
  :root[data-platform="android"]:not([data-theme="light"]) { /* …同 §3.4… */ }
}

/* —— 骨架三行栅格 —— */
:root[data-platform="android"] .app-root {
  display: grid;
  grid-template-rows:
    calc(var(--android-titlebar-h) + var(--android-inset-top))
    1fr
    calc(var(--android-tabbar-h) + var(--android-inset-bottom));
  grid-template-columns: 1fr;
  grid-template-areas: "titlebar" "content" "nav";
  height: 100dvh;
  border-radius: 0;
}
:root[data-platform="android"] .titlebar {
  grid-area: titlebar;
  height: calc(var(--android-titlebar-h) + var(--android-inset-top));
  padding: var(--android-inset-top) 16px 0;
  background: var(--android-glass-titlebar);
  backdrop-filter: blur(24px) saturate(1.4);
  border-bottom: 1px solid var(--border);
  z-index: 40;
}
:root[data-platform="android"][data-blur="0"] .titlebar,
:root[data-platform="android"][data-blur="0"] .nav { backdrop-filter: none; }
:root[data-platform="android"][data-blur="0"] .titlebar { background: var(--bg); }
:root[data-platform="android"][data-blur="0"] .nav { background: var(--surface); }

/* —— 底部 Tab 栏（替代 .nav 的桌面左导航）—— */
:root[data-platform="android"] .nav {
  grid-area: nav;
  flex-direction: row;
  align-items: stretch;
  justify-content: space-between;
  gap: 0;
  padding: 0 0 var(--android-inset-bottom);
  height: calc(var(--android-tabbar-h) + var(--android-inset-bottom));
  background: var(--android-glass-tabbar);
  backdrop-filter: blur(40px) saturate(1.6);
  border-right: 0;
  border-top: 1px solid var(--border);
  box-shadow: none;
}
:root[data-platform="android"] .nav-indicator { display: none; } /* 改用每项胶囊 */
:root[data-platform="android"] .nav-item {
  flex: 1 1 0;
  width: auto;
  min-height: var(--android-tabbar-h);
  padding: 8px 0 6px;
  gap: 2px;
  border-radius: 0;
  font-size: 11px;
  line-height: 16px;
  color: var(--text-secondary);
}
:root[data-platform="android"] .nav-item svg { width: 24px; height: 24px; stroke-width: 1.6; }
:root[data-platform="android"] .nav-item[aria-current="page"] {
  background: transparent;         /* 不用桌面整块 accent-soft 底 */
  color: var(--accent);
  font-weight: 500;
}
:root[data-platform="android"] .nav-item[aria-current="page"] .nav-ico {
  background: var(--android-tab-indicator);
  border-radius: 15px;
}
:root[data-platform="android"] .nav-ico {
  width: 56px;
  height: 30px;
  display: grid;
  place-items: center;
  transition: background var(--dur-fast) var(--ease-standard);
}

/* —— 内容区 —— */
:root[data-platform="android"] .content {
  padding: var(--android-page-pad) var(--android-page-pad) 24px;
  scrollbar-width: none;
}
:root[data-platform="android"] .content::-webkit-scrollbar { display: none; }
:root[data-platform="android"] .statusbar { display: none; }
:root[data-platform="android"] .win-controls { display: none; }
:root[data-platform="android"] .brand-sub,
:root[data-platform="android"] .identity { display: none; }

/* —— 触摸尺寸 —— */
:root[data-platform="android"] .btn { height: var(--android-control-h); padding: 0 16px; }
:root[data-platform="android"] .icon-btn { width: 44px; height: 44px; }
:root[data-platform="android"] .input { height: var(--android-control-h); font-size: 15px; }
:root[data-platform="android"] .list-row { min-height: var(--android-list-row-h); padding: 8px 12px; }
:root[data-platform="android"] .switch { min-height: 48px; }
:root[data-platform="android"] .stepper { height: var(--android-control-h); }
:root[data-platform="android"] .stepper .step { width: 44px; }
:root[data-platform="android"] .dd-btn { height: var(--android-control-h); }
:root[data-platform="android"] .device .send { height: 44px; padding: 0 16px; }
:root[data-platform="android"] .progress { height: 8px; }
:root[data-platform="android"] .progress.large { height: 10px; }

/* —— 触摸端禁用 hover 粘滞 —— */
@media (pointer: coarse) {
  :root[data-platform="android"] .card:hover { box-shadow: var(--shadow-card); }
  :root[data-platform="android"] .list-row:hover { background: transparent; }
  :root[data-platform="android"] .device:hover { border-color: var(--border); background: var(--surface); }
  :root[data-platform="android"] .btn:hover { background: var(--surface); }
  :root[data-platform="android"] .btn.primary:hover { background: var(--accent); }
  :root[data-platform="android"] .btn:active { background: var(--android-state-pressed-strong); }
  :root[data-platform="android"] .list-row:active { background: var(--android-state-pressed); }
}

/* —— 发送页：拖放区改为大按钮选择面板 —— */
:root[data-platform="android"] .dropzone { display: none; }   /* 由 .pick-panel 取代 */
:root[data-platform="android"] .pick-panel {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 16px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-card);
  box-shadow: var(--shadow-card);
}
:root[data-platform="android"] .pick-panel .btn { height: 52px; justify-content: flex-start; }
:root[data-platform="android"] .pick-panel .btn.primary { width: 100%; }

/* —— 取消列表内部滚动（跟页面一起滚） —— */
:root[data-platform="android"] .staged-list,
:root[data-platform="android"] .device-list {
  max-height: none;
  overflow: visible;
  overscroll-behavior: auto;
}

/* —— Toast 抬到 Tab 栏上方 —— */
:root[data-platform="android"] .toast {
  bottom: calc(var(--android-tabbar-h) + var(--android-inset-bottom) + 12px);
  max-width: calc(100% - 32px);
}

/* —— 底部 Sheet —— */
.sheet-scrim { position: fixed; inset: 0; background: var(--android-scrim); z-index: 1350; }
.sheet {
  position: fixed; left: 0; right: 0; bottom: 0; z-index: 1400;
  background: var(--surface);
  border-radius: var(--android-sheet-radius) var(--android-sheet-radius) 0 0;
  box-shadow: var(--android-sheet-shadow);
  padding: 8px 8px calc(16px + var(--android-inset-bottom));
  animation: sheet-up 200ms var(--ease-decelerate);
}
@keyframes sheet-up { from { transform: translateY(16px); opacity: 0; } to { transform: none; opacity: 1; } }
.sheet-handle { width: 32px; height: 4px; margin: 4px auto 8px; border-radius: 2px; background: var(--border-strong); }
.sheet-item { display: flex; align-items: center; gap: 12px; height: 56px; padding: 0 12px; border-radius: 8px; font-size: 14px; }
.sheet-item:active { background: var(--android-state-pressed); }
.sheet-item.danger { color: var(--danger); }

/* —— 平板 / 大屏 —— */
@media (min-width: 600px) and (max-width: 839.98px) {
  :root[data-platform="android"] .app-root {
    grid-template-columns: 72px 1fr;
    grid-template-areas: "titlebar titlebar" "nav content";
    grid-template-rows: calc(var(--android-titlebar-h) + var(--android-inset-top)) 1fr;
    padding-bottom: var(--android-inset-bottom);
  }
  :root[data-platform="android"] .nav {
    flex-direction: column; height: auto; padding: 8px 0;
    border-top: 0; border-right: 1px solid var(--border);
  }
  :root[data-platform="android"] .nav-item { flex: 0 0 auto; width: 100%; font-size: 10px; }
}
@media (min-width: 840px) {
  /* 复用桌面两列发送页，但左导航为 80dp Rail */
  :root[data-platform="android"] .app-root { grid-template-columns: 80px 1fr; }
  :root[data-platform="android"] .send-grid {
    grid-template-columns: minmax(0, 1fr) 320px;
    grid-template-areas: "head head" "drop devices" "staged direct";
  }
}
```

### 6.3 现有样式覆盖清单（逐文件）

| 文件 | 选择器 | Android 改动 |
| --- | --- | --- |
| `styles/base.css` | `.app-root` | 三行栅格 + `100dvh` + `border-radius:0`（§6.2 已覆盖） |
| `styles/base.css` | `.titlebar` | 高 44+inset、`padding-top:inset`、玻璃底 |
| `styles/base.css` | `.nav` / `.nav-item` / `.nav-indicator` | 横向 Tab 栏；`.nav-indicator{display:none}`；指示胶囊移到 `.nav-ico` |
| `styles/base.css` | `.statusbar` | `display:none` |
| `styles/base.css` | `.win-controls` / `.brand-sub` / `.identity` | `display:none` |
| `styles/base.css` | `@media (max-width:640px)` 块 | 保留不动（桌面/窄窗通用）；Android 用 `min-width:600px` 系列反向覆盖 |
| `styles/components.css` | `.btn` / `.icon-btn` / `.input` / `.stepper` | 高度改 44dp，`::after` 扩命中区 |
| `styles/components.css` | `.card` | 不改（阴影、圆角、边框已符合）；仅补 `@media(pointer:coarse)` 的 hover 复位 |
| `styles/components.css` | `.dd-pop` | Android 改为 `.sheet` 呈现（`src/ui/dropdown.ts` 分支） |
| `styles/components.css` | `.toast` | 上移 + `max-width` |
| `styles/components.css` | `.switch` / `.track` | 行高 48dp；轨道 40×20 保留 |
| `styles/views.css` | `.send-grid` | 单列（<600）；≥840 恢复两列 320dp |
| `styles/views.css` | `.dropzone` | `display:none`（由 `.pick-panel` 取代） |
| `styles/views.css` | `.staged-list` / `.device-list` | 取消 `max-height` 与内部滚动 |
| `styles/views.css` | `.page-title` | 22sp/28sp（<600）、26sp/32sp（≥600） |
| `styles/views.css` | `.grid-2` | 单列 |
| `styles/views.css` | `.history-detail` | 单列（<600） |
| `styles/views.css` | `.diag-row` | 单列（<600），值 `overflow-wrap:anywhere` |
| `styles/views.css` | `.settings-actions` | `position: sticky; bottom: 0` + 背景 `--bg` + 上边 1px `--border` |
| `styles/materials.css` | `data-backdrop="mica/acrylic"` 全部规则 | Android 不设置 `data-backdrop`（`main.ts` 分支跳过 `applyBackdrop()`），此文件在 Android 无命中 |
| `styles/motion.css` | `.content.view-enter` 的卡片级联 | Android 只保留页级 250ms 滑动，卡片级联 `animation: none`（低端机掉帧） |

### 6.4 平台检测与 `shell.ts` 分支

**新增 `src/platform.ts`**：

```ts
export type Platform = "desktop" | "android";

/** Windows/Linux 桌面 vs Android：Tauri v2 Android 的 UA 含 "Android"。 */
export const platform: Platform = /Android/i.test(navigator.userAgent) ? "android" : "desktop";
export const isAndroid = platform === "android";
export const isTouch = matchMedia("(hover: none) and (pointer: coarse)").matches;

export function applyPlatform(): void {
  const root = document.documentElement;
  root.dataset.platform = platform;
  // 玻璃能力检测：不支持则退回 T0 纯色
  const blurOk = CSS.supports("backdrop-filter", "blur(1px)");
  root.dataset.blur = blurOk && isAndroid ? "1" : "0";
}
```

**`shell.ts` 分支点（保持 `Shell` 接口向后兼容）**：

| 位置 | 桌面 | Android |
| --- | --- | --- |
| 构建 `.win-controls` | 构建 + 绑定最小化/最大化/关闭 | **不构建**（`if (!isAndroid)`） |
| 标题栏 `dblclick` | 绑定 toggleMaximize | 不绑定 |
| `data-tauri-drag-region` | 保留（窗口拖动） | 去掉属性（无意义，且长按会触发文本选择） |
| `#info-btn` / `.info-pop` | 保留 | 移除（IP/短 ID 信息在诊断页已有） |
| `.nav` 结构 | 按钮内直接放 svg + 文本 | **svg 外包一层 `<span class="nav-ico">`**（容纳 M3 式指示胶囊） |
| `setActive()` | 计算 `.nav-indicator` 的 `translateY/X` | **跳过指示条计算**（`if (isAndroid) return;`），只切 `aria-current` |
| Tab 切换动效 | 无 | 复用 `main.ts` 的 `view-slide-forward/back`（已有） |
| `statusbar` | 显示 | 不构建 |
| 返回按钮 | 无 | 可选：标题栏左侧 `.titlebar-back`（20dp 图标 + 44dp 命中区），仅在 `routeStack.length > 1` 时显示 |

代码骨架（约 25 行改动）：

```ts
import { isAndroid } from "../platform";

export function buildShell(root: HTMLElement, onNavigate: (route: Route) => void): Shell {
  // …titlebar：品牌区保留，窗口按钮仅桌面
  if (!isAndroid) {
    titlebar.append(infoBtn, infoPop, winControls);
    titlebar.addEventListener("dblclick", () => { void getCurrentWindow().toggleMaximize(); });
  } else {
    titlebar.setAttribute("data-android", "1"); // 不设置 data-tauri-drag-region
  }

  // nav：现状是 <button class="nav-item">[svg 20×20, span(text)]（shell.ts:187-200）
  // Android 需要包一层 .nav-ico 才能画 M3 式指示胶囊：
  //   <button class="nav-item">[span.nav-ico>svg 24, span.nav-label(text)]
  for (const item of NAV) {
    const btn = el("button", { class: "nav-item", type: "button",
                               onclick: () => onNavigate(item.route) });
    const label = el("span", { class: "nav-label", text: t(item.label) });
    if (isAndroid) {
      const ico = el("span", { class: "nav-ico" });
      ico.append(svgIcon(icons[item.icon], 24));
      btn.append(ico, label);
    } else {
      btn.append(svgIcon(icons[item.icon], 20), label);   // 桌面保持原样
    }
    navButtons.set(item.route, btn);
    nav.append(btn);
  }

  // setActive：Android 走纯 CSS 指示，不算 transform（现状 placeIndicator 用 matchMedia(640) 判断横竖，Android 不适用）
  const setActive = (route: Route) => {
    activeRoute = route;
    navButtons.forEach((b, k) => b.setAttribute("aria-current", k === route ? "page" : "false"));
    if (isAndroid) return;
    requestAnimationFrame(() => { /* …原有 placeIndicator 逻辑… */ });
  };
}
```

> `.nav-label` 与 `.nav-ico` 是**新增的 class**（现状 `shell.ts:196` 的文本 span 无 class），需同步在 `android.css` 里补 `.nav-label { font-size: 11px; line-height: 16px; }`（尺寸由 `.nav-item` 继承也能工作，显式写更稳）。

> `Shell` 接口新增两个可选成员，避免破坏 `main.ts` 调用方：
> `setBackVisible?(visible: boolean): void;`、`setChromeHidden?(hidden: boolean): void;`

### 6.5 `main.ts` 改动

```ts
import { applyPlatform, isAndroid } from "./platform";
import { initAndroidInsets } from "./ui/android-insets";

applyPlatform();                       // 首行：在 buildShell 之前
await initAndroidInsets();             // 同步命令一次取值，再建 shell（失败静默，CSS 有 env() 兜底）
if (!isAndroid) { await applyBackdrop(); }   // Android 不查询 backdrop_info

// 返回栈
const routeStack: Route[] = ["send"];
function navigate(route: Route, push = true) {
  if (route === currentRoute) return;
  if (push) { routeStack.push(route); history.pushState({ route }, ""); }
  // …原有逻辑…
}
window.addEventListener("popstate", () => {
  if (routeStack.length > 1) {
    routeStack.pop();
    navigate(routeStack[routeStack.length - 1]!, false);
  } else if (currentRoute !== "send") {
    navigate("send", false);
  } else {
    armExitOnSecondBack();              // 2 秒内再返回才退出
  }
});

// Android 分享入口（阶段 C）
if (isAndroid) { void listen<string[]>("android:share", (e) => void stagePaths(e.payload)); }
```

### 6.6 新增组件规格

| 组件 | 文件 | 规格 |
| --- | --- | --- |
| `PickPanel` | `views/send.ts` 内联 | 两枚 52dp 按钮（`选择文件` 主 / `选择文件夹` 次）+ 13sp 说明，取代 `.dropzone` |
| `TabBar` | `layout/shell.ts` | 5 项等分、24dp 图标、11sp 标签、`accent-soft` 指示胶囊、56dp+安全区 |
| `Sheet` / `ActionSheet` | `ui/sheet.ts`（新） | 16dp 顶部圆角、把手、56dp 行、scrim、返回键关闭 |
| `SheetSelect` | `ui/dropdown.ts` 分支 | 设置页下拉（主题/语言/压缩）在 Android 上改为底部 Sheet 单选 |
| `InsetHeader` | CSS only | 标题栏 `padding-top: inset-top`，滚动时保持玻璃 |
| `LongPress` helper | `ui/dom.ts` | `onLongPress(el, fn)`：`pointerdown` → 500ms 定时器；`pointermove > 8px` 或 `pointerup/cancel` 取消；触发后 `navigator.vibrate(10)` |
| `SwipeToDismiss`（可选） | `ui/sheet.ts` | 下滑关闭 Sheet，`translateY` 跟手 + 阈值 30% 释放 |

### 6.7 桌面元素隐藏清单

| 元素 | 位置 | Android 处理 | 理由 |
| --- | --- | --- | --- |
| `.win-controls`（最小化/最大化/关闭） | `shell.ts` + `base.css` | 不构建 + `display:none` | 系统窗口没有自绘按钮 |
| 标题栏拖动区 `data-tauri-drag-region` | `shell.ts` | 去属性 | 触摸拖动无意义，且长按会选中文字 |
| 双击标题栏最大化 | `shell.ts` | 不绑定 | 无窗口概念 |
| `.statusbar`（状态 + 版本） | `shell.ts` + `base.css` | 不构建 | 移动端空间宝贵；状态改由 Tab 角标/通知承担，版本在设置页 |
| `.brand-sub`（"局域网直传"副标题） | `base.css` | 隐藏（已有 <640 规则） | 44dp 标题栏放不下 |
| `.identity`（设备名胶囊） | `base.css` | 隐藏 | 设备名在设置页/诊断页 |
| `#info-btn` + `.info-pop`（IP 列表浮层） | `shell.ts` | 移除 | 诊断页已完整展示；移动端浮层体验差 |
| 材质/backdrop 下拉（设置·外观） | `settings.ts` | 隐藏整个"材质"行 | Android 无材质可切，只保留主题 |
| "启动"组（开机自启） | `settings.ts` | 隐藏整组 | Android 无等价物 |
| 防火墙状态 + "放行防火墙" | `settings.ts`（网络组） | 隐藏 | 无防火墙概念 |
| `检查更新`（MSI/updater） | `settings.ts`（关于组） | 隐藏或改为"通过应用商店更新" | Tauri updater 的 Windows 安装链路不适用 |
| 诊断页 WebView2 版本 / 透明回退 / backdrop 行 | `diagnostics.ts` | 隐藏 3 行，改为 Android 版本 / ABI / 安全补丁 | 桌面专有信息 |
| `window_shadow()` 调用 | `main.ts` | 不调用 | 无边框窗口专用 |
| 自定义滚动条（10px） | `base.css` | 隐藏 | 触摸端不需要 |
| `.card:hover` / `.list-row:hover` 等 | 全站 | `@media(pointer:coarse)` 内复位 | 触摸无 hover，会粘滞 |

### 6.8 i18n 新增 key（`public/i18n/zh-CN.xml`）

| key | 中文 | 用途 |
| --- | --- | --- |
| `android.prepare` | 准备中 · {done}/{total} | SAF 选中后复制到缓存的进度 |
| `android.background.title` | 后台接收中 | 应用内 banner |
| `android.background.hint` | 离开应用后可在通知中查看进度 | 同上 |
| `android.notif.channel` | 传输 | 通知渠道名 |
| `android.notif.receiving` | 正在接收 {name} | 前台服务通知 |
| `android.notif.receiving.progress` | {percent}% · {speed} | 通知副行 |
| `android.notif.received` | 已接收 {files} 个文件 | 完成通知 |
| `android.notif.permission` | 允许通知，后台也能看到接收进度 | 权限说明 |
| `sheet.remove` | 移除 | ActionSheet |
| `sheet.clearAll` | 清空全部 | ActionSheet |
| `sheet.openLocation` | 打开位置 | ActionSheet |
| `sheet.share` | 用其他应用打开 | ActionSheet |
| `sheet.copyAddr` | 复制地址 | ActionSheet |
| `action.backAgainToExit` | 再按一次退出 | 返回兜底 toast |
| `receive.openFolderAndroid` | 在文件管理器中查看 | 替代"打开接收文件夹" |
| `settings.about.sideload` | 通过应用商店或 APK 更新 | 替代检查更新 |

命名沿用现有 `area.component.role` 约定，占位符 `{name}` 与 `t()` 兼容。

### 6.9 原生侧改动（Rust / Kotlin，分阶段）

| 阶段 | 改动 | 说明 |
| --- | --- | --- |
| A（v1 必须） | `AndroidManifest.xml`：`android:enableOnBackInvokedCallback="true"`；`MainActivity`：`enableEdgeToEdge()`；新 Kotlin 插件 `InsetsPlugin`（命令 `android_insets` + 事件 `android:insets`） | 支撑 §2 全部内容 |
| A | `MainActivity`：`isForceDarkAllowed = false`；`textZoom = fontScale * 100` | 深色与系统字体缩放 |
| B（v1 建议） | `android_set_bars` 命令（状态栏图标明暗） | 深浅色切换 |
| C（v2） | SAF 落缓存：`ACTION_OPEN_DOCUMENT/_TREE` → 复制到 `cacheDir/lanpipe-stage` → 返回绝对路径 | 让现有 Rust 路径逻辑零改动（§5.4 方案 1） |
| C | `ACTION_SEND` intent-filter + FileProvider（`file_paths.xml`）+ `android:share` 事件 | 分享入口/出口 |
| D（v3） | 前台服务（type=dataSync）+ 通知渠道 + `POST_NOTIFICATIONS` 运行时权限 | 后台接收 |
| — | `tauri.conf.json`：Android 构建时 `bundle.targets` 增加 `apk`/`aab`（不影响 Windows 的 `msi`） | 打包 |

> `Tauri v2 Android` 参考：[Tauri v2 移动端开发](https://v2.tauri.app/develop/)、[Tauri 插件开发](https://v2.tauri.app/develop/plugins/)、[dialog 插件](https://v2.tauri.app/plugin/dialog/)。

### 6.10 `index.html`

```html
<meta name="viewport"
      content="width=device-width, initial-scale=1, viewport-fit=cover" />
```

现缺 `viewport-fit=cover`（这是 `env(safe-area-inset-*)` 生效的前提）；`user-scalable=no` **不要**加（无障碍）。

### 6.11 验收清单

1. 390×844、360×800、430×932 三种竖屏下**无横向滚动**，五页均可完整滚动到底。
2. 挖孔屏/刘海屏：状态栏图标与标题栏内容不重叠；手势导航条不遮挡 Tab 标签（insets 生效）。
3. 三键导航与手势导航切换后布局正确（insets 事件驱动）。
4. 返回链路：Tab 内滚动 → 返回不退出；历史→设置跨页 → 返回回历史；发送页 → 连按两次退出并提示。
5. 所有可点击元素命中区 ≥48dp（含 `.icon-btn`、`.device .send`、`.stepper .step`、Tab 项）。
6. 触摸端无 hover 粘滞（点完卡片/按钮不高亮残留）。
7. 深/浅色下正文对比度 ≥4.5:1（复用 token，直接复用桌面验收结论）。
8. 关闭 `backdrop-filter`（`data-blur="0"`）时 Tab 栏/标题栏为纯色 + 1px 边框，无露底、无接缝。
9. 旋转/分屏后布局按 600/840 断点切换且不重启页面；`100dvh` 不出现双滚动条。
10. 系统字体放大到 130% 时，Tab 标签不折行、按钮文字不裁切（`textZoom` 生效）。
11. 选文件/选文件夹走 SAF，无存储权限弹窗；取消选择不报错。
12. 中文标题在 `font-weight:500` 下不出现合成加粗发虚。

---

## 7. 风险与未决问题

| # | 风险 | 影响 | 缓解 |
| --- | --- | --- | --- |
| R1 | **SAF `content://` 与 Rust 现有路径逻辑不兼容**（`summarize_paths`/`start_send` 需要绝对路径） | 阻塞发送页主流程 | 阶段 C 方案 1（落缓存）先跑通；方案 2 另立 task。这是本方案**最大工程风险** |
| R2 | 5 个 Tab 在 320dp 老设备上每项仅 64dp，标签可能压缩 | 布局拥挤 | 字号降 10sp + `white-space:nowrap`；触发条件出现则切 Plan B（诊断进设置） |
| R3 | `backdrop-filter` 在低端机（Adreno/Mali 老 GPU）掉帧 | Tab 栏滚动卡顿 | 特性检测 + 默认仅 2 处 + 提供"关闭透明效果"开关（可复用桌面 `LANPIPE_BACKDROP=none` 思路） |
| R4 | insets 插件首帧慢 → 标题栏一开始顶到状态栏下 | 闪一下 | `android_insets` 同步命令在 `buildShell` 前 await（快速），CSS 有 `env()` 兜底 |
| R5 | Android 14+ 前台服务启动限制、国产 ROM 杀后台 | 后台接收失败 | 阶段 D 前先做「应用内 banner 提示」降级；引导关闭电池优化 |
| R6 | 通知权限被拒 | 后台进度不可见 | 应用内提示 + 设置页跳转说明 |
| R7 | WebView 字体缩放导致断点内文字溢出 | 无障碍不达标 | 验收清单 #10；必要时对 Tab 标签用 `clamp()` |
| R8 | 预测性返回（Android 14+）与 WebView 内 `history` 的动画不同步 | 返回动画生硬 | 保守：v1 只保证功能正确（不闪退/不丢页），动画后续调 |
| R9 | 国产 ROM 无 `Noto Sans CJK SC` 精确族名 | 中文回退到系统默认（多为思源/苹方），观感略有差异 | 字体栈兜底 `sans-serif`，不依赖族名精确匹配 |
| R10 | `transform` 禁令（§16.7.7）导致不能用 scale 做按压反馈 | 触摸反馈偏弱 | 用 `background/border` 变色 + 100ms，符合 Fluent（Fluent 本身不用 scale） |

**未决问题（需 Lead / 产品确认）**：
1. 诊断页是否保留为第 5 个 Tab（Plan A）还是收进设置（Plan B）？
2. Android 首版是否要「后台接收 + 前台服务」，还是先只做前台可见传输？
3. SAF 落缓存方案 1 的临时空间上限（2GB 文件 × N）如何提示用户？
4. 分享到 LanPipe 的入口是否进 v1（阶段 C）？
5. 是否提供"跟随壁纸取色"开关（T3），还是完全锁死 Fluent 配色？

---

## 8. 参考资料

**Fluent 2 / Windows**
- [Fluent 2 设计系统](https://fluent2.microsoft.design/)
- [Fluent 2 形状（圆角 4/2/8/12）](https://fluent2.microsoft.design/shapes)
- [Fluent 2 快速开始与 token](https://fluent2.microsoft.design/get-started/design)
- [Mica material](https://learn.microsoft.com/en-us/windows/apps/design/style/mica)
- [Acrylic material](https://learn.microsoft.com/en-us/windows/apps/design/style/acrylic)
- [Windows 11 几何与间距](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/geometry)
- [NavigationView（导航范式）](https://learn.microsoft.com/en-us/windows/apps/design/controls/navigationview)

**Material 3（仅借系统行为）**
- [Material 3](https://m3.material.io/)
- [Navigation bar（3–5 个目的地）](https://m3.material.io/components/navigation-bar/overview)
- [Window size classes](https://m3.material.io/foundations/layout/applying-layout/window-size-classes)
- [Elevation](https://m3.material.io/styles/elevation/overview)
- [动态取色 Material You](https://m3.material.io/styles/color/dynamic-color/overview)

**Android 平台**
- [Edge-to-edge 实现指南](https://developer.android.com/develop/ui/views/layout/edge-to-edge)
- [WindowInsets](https://developer.android.com/develop/ui/views/layout/insets)
- [Android 15 行为变更（edge-to-edge 强制）](https://developer.android.com/about/versions/15/behavior-changes-15)
- [Predictive back gesture](https://developer.android.com/guide/navigation/custom-back/predictive-back-gesture)
- [RenderEffect（真模糊，API 31+）](https://developer.android.com/reference/android/graphics/RenderEffect)
- [BlurRenderEffect 指南](https://developer.android.com/develop/ui/views/graphics/blur-render-effect)
- [Storage Access Framework](https://developer.android.com/guide/topics/providers/document-provider)
- [发送简单数据（ACTION_SEND）](https://developer.android.com/training/sharing/send)
- [接收简单数据（ACTION_SEND 入口）](https://developer.android.com/training/sharing/receive)
- [通知渠道](https://developer.android.com/develop/ui/views/notifications/channels)
- [前台服务](https://developer.android.com/develop/background-work/services/fgs)
- [Android 14 前台服务类型要求](https://developer.android.com/about/versions/14/changes/fgs-types-required)

**Tauri v2**
- [Tauri v2 开发（含移动端）](https://v2.tauri.app/develop/)
- [Tauri 插件开发（Kotlin 侧）](https://v2.tauri.app/develop/plugins/)
- [dialog 插件](https://v2.tauri.app/plugin/dialog/)
- [notification 插件](https://v2.tauri.app/plugin/notification/)
