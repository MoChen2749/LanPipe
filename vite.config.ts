import { defineConfig } from "vite";

// Tauri 在开发模式下从固定端口加载前端
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/crates/**"] }
  },
  build: {
    target: "chrome105",
    outDir: "dist",
    emptyOutDir: true
  }
});
