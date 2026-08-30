import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: "es2021",
    outDir: "dist",
    rollupOptions: {
      // 双入口:index.html → 面板,settings.html → 设置窗口
      input: {
        main: "index.html",
        settings: "settings.html",
      },
    },
  },
});
