import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath } from "node:url";
import type { Plugin } from "vite";

/** Fontsource CSS lists a .woff fallback after each .woff2; both webviews we target load
 *  woff2, so drop the fallbacks instead of embedding ~7 MB of unused font files. */
function woff2Only(): Plugin {
  return {
    name: "woff2-only",
    enforce: "pre",
    transform(code, id) {
      if (!id.includes("@fontsource") || !id.endsWith(".css")) return null;
      let css = code.replace(/,\s*url\([^)]*\.woff\)\s*format\(['"]woff['"]\)/g, "");
      // The bundled emoji subsets can't shape emoji + U+FE0F (✈️, ❤️ drew a box), so they
      // get their own family name and only back up a complete system emoji font.
      if (id.includes("noto-color-emoji")) css = css.replace(/font-family: 'Noto Color Emoji'/g, "font-family: 'BlackBubbles Emoji'");
      return css;
    },
  };
}

// Tauri loads http://localhost:5173 in dev and ui/dist in release builds.
export default defineConfig({
  plugins: [woff2Only(), react(), tailwindcss()],
  resolve: { alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) } },
  clearScreen: false,
  server: { port: 5173, strictPort: true, host: "127.0.0.1" },
  build: { target: ["es2022", "safari16"], outDir: "dist", emptyOutDir: true, chunkSizeWarningLimit: 1200 },
});
