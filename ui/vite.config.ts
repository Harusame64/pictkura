import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauriのwebviewに読み込ませる静的ビルドが主目的。
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  build: {
    target: "es2021",
  },
});
