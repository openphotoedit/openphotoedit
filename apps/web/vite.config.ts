import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  base: "./",
  plugins: [svelte()],
  worker: { format: "es" },
  build: { target: "es2022", sourcemap: true, chunkSizeWarningLimit: 4000 },
  optimizeDeps: { exclude: ["onnxruntime-web"] },
  server: {
    // Cross-origin isolation lets onnxruntime use threads when it can.
    headers: { "Cross-Origin-Opener-Policy": "same-origin", "Cross-Origin-Embedder-Policy": "require-corp" },
  },
  preview: {
    headers: { "Cross-Origin-Opener-Policy": "same-origin", "Cross-Origin-Embedder-Policy": "require-corp" },
  },
});
