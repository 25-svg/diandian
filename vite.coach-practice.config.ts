import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import sveltePreprocess from 'svelte-preprocess';
export default defineConfig({plugins:[svelte({preprocess:sveltePreprocess({typescript:true})})],
  cacheDir:'node_modules/.vite-coach-practice',
  optimizeDeps:{entries:['tests/fixtures/coach-practice-harness.html']},
  server:{watch:{ignored:['**/src-tauri/**','**/dist/**','**/.funasr-venv/**']}}});
