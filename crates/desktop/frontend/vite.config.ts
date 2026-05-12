import { defineConfig } from "vite";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    // After splitting heavy vendor libs into separate chunks the main app
    // chunk drops well under 500kB. Cap at 800kB to keep a regression alarm.
    chunkSizeWarningLimit: 800,
    rollupOptions: {
      output: {
        // Group vendor packages into named chunks so the main app bundle
        // stays small. Heavy editors and the terminal emulator load from
        // their own chunks, parsed lazily by Vite's dynamic-import
        // inference where the call sites use `await import(...)`.
        manualChunks(id: string): string | undefined {
          if (id.includes("node_modules")) {
            // Milkdown ships as a thin wrapper around ProseMirror; ship
            // both together so the Markdown tab pulls one chunk instead
            // of fragmenting prosemirror-* across many tiny chunks.
            if (id.includes("@milkdown") || id.includes("prosemirror")) {
              return "vendor-milkdown";
            }
            if (
              id.includes("codemirror") ||
              id.includes("@codemirror") ||
              id.includes("@replit/codemirror-vim")
            ) {
              return "vendor-codemirror";
            }
            if (id.includes("@xterm")) return "vendor-xterm";
            if (
              id.includes("marked") ||
              id.includes("highlight.js")
            ) {
              return "vendor-markdown";
            }
            if (id.includes("@tauri-apps")) return "vendor-tauri";
            // Catch-all for small libs (lib0, w3c-keyname, etc.). Keeping
            // them separate from the app chunk lets the browser cache
            // them across deploys that only touch the app code.
            return "vendor";
          }
          return undefined;
        },
      },
    },
  },
});
