import { defineConfig } from "vite";

export default defineConfig({
  clearScreen: false,
  server: {
    host: "127.0.0.1",
    port: 1421,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**", "**/.local/**", "**/engine/**", "**/release/**", "**/dist/**"],
    },
  },
  build: { target: "es2022", assetsInlineLimit: 0 },
});
