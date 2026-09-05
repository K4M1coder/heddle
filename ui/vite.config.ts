// From `vitest/config`, not `vite`: the `test` block below is Vitest's own
// extension of Vite's config type and `vite`'s `defineConfig` does not know it.
import { defineConfig } from "vitest/config";

// `root` is `src/` and `outDir` is `../dist`, so `index.html` lives beside the
// TypeScript it loads. Both paths are named again in
// `src-tauri/tauri.conf.json` (`devUrl`, `frontendDist`) and the two files must
// agree: Tauri serves this dev server in `tauri dev` and loads `dist/` in a
// built app.
export default defineConfig({
  root: "src",
  // Tauri's own output must not be swallowed by Vite's screen clearing.
  clearScreen: false,
  server: {
    port: 1420,
    // A moved port would silently disagree with `devUrl` below.
    strictPort: true,
  },
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    target: "es2021",
  },
  test: {
    // Node, not jsdom: `chatState.ts` is a pure reducer with no DOM in it, and
    // that is the point of keeping it separate from `main.ts` (Constitution I).
    environment: "node",
    include: ["**/*.test.ts"],
  },
});
