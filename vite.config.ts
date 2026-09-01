import { defineConfig, loadEnv } from "vite";
import { resolve } from "path";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import sveltePreprocess from "svelte-preprocess";
import { readFileSync } from "fs";

// Read package.json
const packageJson = JSON.parse(
  readFileSync(resolve(__dirname, "package.json"), "utf-8")
);

// https://vitejs.dev/config/
// @ts-ignore
export default defineConfig(async ({ mode }) => {
  // Load env file based on `mode` in the current working directory.
  // Set the third parameter to '' to load all env regardless of the `VITE_` prefix.
  const env = loadEnv(mode, process.cwd(), "");

  return {
    define: {
      __APP_VERSION__: JSON.stringify(packageJson.version),
      __API_BASE_URL__: JSON.stringify(env.API_BASE_URL || ""),
    },
    optimizeDeps: {
      exclude: ["@ffmpeg/ffmpeg", "@ffmpeg/util"],
    },
    plugins: [
      svelte({
        preprocess: [
          sveltePreprocess({
            typescript: true,
          }),
        ],
      }),
    ],

    // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
    // prevent vite from obscuring rust errors
    clearScreen: false,
    // tauri expects a fixed port, fail if that port is not available
    server: {
      // Keep the dev server on IPv4 loopback. The desktop WebView can resolve
      // `localhost` to 127.0.0.1 while Vite otherwise listens only on ::1.
      host: "127.0.0.1",
      port: 8054,
      strictPort: true,
      watch: {
        // `dist` is the packaged app output. The running Tauri executable can
        // lock its assets on Windows, which otherwise makes Vite crash while
        // trying to watch them.
        ignored: ["**/src-tauri/target/**", "**/dist/**"],
      },
    },
    // to make use of `TAURI_DEBUG` and other env variables
    // https://tauri.studio/v1/api/config#buildconfig.beforedevcommand
    envPrefix: ["VITE_", "TAURI_"],
    build: {
      rollupOptions: {
        input: {
          main: resolve(__dirname, "index.html"),
          live: resolve(__dirname, "index_live.html"),
          clip: resolve(__dirname, "index_clip.html"),
        },
      },
      // Tauri supports es2021
      target:
        process.env.TAURI_PLATFORM == "windows" ? "chrome105" : "safari13",
      // don't minify for debug builds
      minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
      // produce sourcemaps for debug builds
      sourcemap: !!process.env.TAURI_DEBUG,
    },
  };
});
