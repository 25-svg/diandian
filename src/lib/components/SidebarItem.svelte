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
    border-radius: 11px;
    color: #4a4a4f;
    background: transparent;
    cursor: pointer;
    font-size: 13px;
    font-weight: 520;
    text-align: left;
    transition: background .18s ease, color .18s ease, transform .18s ease, box-shadow .18s ease;
  }
  .sidebar-item:hover { color: #1d1d1f; background: rgba(0,0,0,.045); }
  .sidebar-item:active { transform: scale(.985); }
  .sidebar-item.active {
    color: #005fc7;
    background: rgba(255,255,255,.9);
    box-shadow: 0 5px 14px rgba(33,48,70,.08), inset 0 0 0 1px rgba(255,255,255,.75);
  }
  .item-icon { width: 20px; height: 20px; display: grid; place-items: center; color: #6e6e73; }
  .sidebar-item.active .item-icon { color: #0071e3; }
  .update-dot { position: absolute; right: 11px; width: 6px; height: 6px; border-radius: 50%; background: #ff453a; }
  :global(.dark) .sidebar-item { color: #d1d1d6; }
  :global(.dark) .sidebar-item:hover { color: white; background: rgba(255,255,255,.07); }
  :global(.dark) .sidebar-item.active { color: white; background: rgba(255,255,255,.12); box-shadow: inset 0 0 0 1px rgba(255,255,255,.06); }
  :global(.dark) .item-icon, :global(.dark) .sidebar-item.active .item-icon { color: #64b5ff; }
</style>
