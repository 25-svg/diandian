<script>
  import { Info, LayoutDashboard, Settings, Users, Video } from "lucide-svelte";
  import { createEventDispatcher } from "svelte";

  const dispatch = createEventDispatcher();

  // acitveUrl is shared between project
  export let activeUrl = "总览";
  export let label = "";
  export let dot = false;
</script>

<button
  on:click={() => dispatch("activeChange", label)}
  class="sidebar-item"
  class:active={activeUrl === label}
>
  <span class="item-icon"><slot name="icon"></slot></span>
  <span>{label}</span>
  {#if dot}
    <div class="update-dot"></div>
  {/if}
</button>

<style>
  .sidebar-item {
    position: relative;
    width: 100%;
    height: 39px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 11px;
    border: 0;
    border-radius: var(--mac-radius-md);
    color: var(--mac-secondary);
    background: transparent;
    cursor: pointer;
    font-size: 13px;
    font-weight: 520;
    text-align: left;
    transition: background 0.18s ease, color 0.18s ease, transform 0.18s ease, box-shadow 0.18s ease;
  }
  .sidebar-item:hover { color: var(--mac-label); background: var(--mac-fill); }
  .sidebar-item:active { transform: scale(0.985); }
  .sidebar-item.active {
    color: var(--mac-blue);
    background: rgba(255, 255, 255, 0.92);
    box-shadow: 0 5px 14px rgba(33, 48, 70, 0.08), inset 0 0 0 1px rgba(255, 255, 255, 0.75);
  }
  .item-icon { width: 20px; height: 20px; display: grid; place-items: center; color: var(--mac-tertiary); }
  .sidebar-item.active .item-icon { color: var(--mac-blue); }
  .update-dot { position: absolute; right: 11px; width: 6px; height: 6px; border-radius: 50%; background: var(--mac-red); }
  :global(.dark) .sidebar-item { color: var(--mac-secondary); }
  :global(.dark) .sidebar-item:hover { color: var(--mac-label); background: var(--mac-fill); }
  :global(.dark) .sidebar-item.active {
    color: var(--mac-label);
    background: rgba(255, 255, 255, 0.12);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.06);
  }
  :global(.dark) .item-icon,
  :global(.dark) .sidebar-item.active .item-icon { color: var(--mac-blue); }
</style>
