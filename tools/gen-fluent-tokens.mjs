// 从微软官方 @fluentui/tokens（Fluent 2 设计体系）生成 CSS 变量。
// 用法: node tools/gen-fluent-tokens.mjs
// 说明：品牌色保留 WinUI 3 默认强调色（#005FB8 / #4CC2FF），其余中性色、
// 状态色、阴影、圆角、字体均取自官方 token，避免手工维护色板。
import { writeFileSync } from "node:fs";
import { webDarkTheme, webLightTheme } from "@fluentui/tokens";

const map = {
  bg: "colorNeutralBackground3",
  surface: "colorNeutralBackground1",
  "surface-alt": "colorNeutralBackground2",
  "surface-hover": "colorNeutralBackground1Hover",
  "surface-pressed": "colorNeutralBackground1Pressed",
  border: "colorNeutralStroke2",
  "border-strong": "colorNeutralStroke1",
  "border-input": "colorNeutralStrokeAccessible",
  text: "colorNeutralForeground1",
  "text-secondary": "colorNeutralForeground2",
  muted: "colorNeutralForeground3",
  disabled: "colorNeutralForegroundDisabled",
  success: "colorStatusSuccessForeground1",
  danger: "colorStatusDangerForeground1",
  warning: "colorStatusWarningForeground1",
};

const accents = {
  light: {
    accent: "#005fb8",
    "accent-hover": "#1a6fc4",
    "accent-pressed": "#004f99",
    "accent-soft": "#e5f1fb",
    "accent-text": "#ffffff",
  },
  dark: {
    accent: "#4cc2ff",
    "accent-hover": "#60cbff",
    "accent-pressed": "#3aa7e0",
    "accent-soft": "#1b3a52",
    "accent-text": "#0a1a26",
  },
};

function block(theme, accent) {
  const lines = Object.entries(map).map(([name, key]) => `  --${name}: ${theme[key]};`);
  lines.push(...Object.entries(accent).map(([name, value]) => `  --${name}: ${value};`));
  lines.push(
    `  --shadow-card: ${theme.shadow2};`,
    `  --shadow-card-hover: ${theme.shadow4};`,
    `  --shadow-flyout: ${theme.shadow16};`,
    `  --shadow-accent: ${theme.shadow2};`,
    `  --radius-control: ${theme.borderRadiusMedium};`,
    `  --radius-card: ${theme.borderRadiusXLarge};`,
    `  --radius-overlay: ${theme.borderRadiusXLarge};`,
    `  --font: ${theme.fontFamilyBase.replaceAll('"', "'")};`,
    `  --font-size-base: ${theme.fontSizeBase300};`,
    `  --line-height-base: ${theme.lineHeightBase300};`,
  );
  return lines.join("\n");
}

const withCjk = (text) => text.replaceAll(", sans-serif", ", 'Microsoft YaHei UI', 'Microsoft YaHei', sans-serif");

const css = `/* 由 tools/gen-fluent-tokens.mjs 生成，请勿手工编辑。
 * 来源: @fluentui/tokens webLightTheme / webDarkTheme（Fluent 2）
 * 品牌色: WinUI 3 默认强调色（与 Fluent 2 brand 不同，按设计稿保留）。
 */

:root {
${block(webLightTheme, accents.light)}
}

:root[data-theme="dark"] {
${block(webDarkTheme, accents.dark)}
}

@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]):not([data-theme="dark"]) {
${block(webDarkTheme, accents.dark)
  .split("\n")
  .map((line) => `  ${line}`)
  .join("\n")}
  }
}
`;

writeFileSync(
  new URL("../src/styles/fluent-tokens.css", import.meta.url),
  withCjk(css),
  "utf8",
);
console.log("wrote src/styles/fluent-tokens.css");
