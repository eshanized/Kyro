/// <reference types="vitest" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./kyroui"),
    },
  },
  clearScreen: false,
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: "./kyroui/test/setup.ts",
    coverage: {
      provider: "v8",
      reporter: ["text", "json", "lcov", "html"],
      include: ["kyroui/**/*.{ts,tsx}"],
      exclude: [
        "kyroui/test/**",
        "kyroui/**/*.d.ts",
        "kyroui/types/**",
      ],
      all: true,
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/libkyro/**"],
    },
  },
}));
