<script lang="ts">
  import { createEventDispatcher } from "svelte";
  export let daysRemaining: number;
  export let busy = false;
  const dispatch = createEventDispatcher<{ renew: void }>();
</script>

<aside aria-label="离线授权提醒">
  <p role="status" aria-live="polite">离线授权剩余 {daysRemaining} 天，请联网续期。</p>
  <button type="button" disabled={busy} on:click={() => dispatch("renew")}>{busy ? "正在续期…" : "联网续期"}</button>
</aside>

<style>
  aside { position: fixed; z-index: 1000; bottom: 12px; right: 12px; box-sizing: border-box; max-width: calc(100vw - 24px); padding: 10px 14px; display: flex; align-items: center; flex-wrap: wrap; gap: 8px 16px; border: 1px solid #aa720f; border-radius: 12px; background: #fff7df; color: #674300; box-shadow: 0 4px 20px #17203320; font-family: system-ui, -apple-system, "Segoe UI", sans-serif; }
  p { margin: 0; font-size: 13px; line-height: 1.6; overflow-wrap: anywhere; }
  button { min-height: 44px; padding: 8px 12px; border: 1px solid #866017; border-radius: 8px; background: #fff; color: #674300; font: inherit; font-size: 13px; cursor: pointer; }
  button:disabled { cursor: not-allowed; }
  button:focus-visible { outline: 3px solid #145ac5; outline-offset: 3px; }
  @media (prefers-reduced-motion: reduce) { * { animation: none; transition: none; } }
</style>
