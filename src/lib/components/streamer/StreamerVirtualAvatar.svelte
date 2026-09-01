<script lang="ts">
  import {
    getStreamerVirtualAvatar,
    type StreamerVirtualAvatarSelection,
  } from "../../streamerAvatar";

  export let selection: StreamerVirtualAvatarSelection;
  export let name = "";
  export let size: "sm" | "md" | "lg" | "xl" = "md";
  export let decorative = false;

  $: avatar = getStreamerVirtualAvatar(selection?.avatarId);
</script>

<span
  class="virtual-avatar virtual-avatar-{size}"
  style={`--avatar-bg:${avatar.background};--avatar-primary:${avatar.primary};--avatar-secondary:${avatar.secondary};--avatar-skin:${avatar.skin}`}
  role={decorative ? undefined : "img"}
  aria-label={decorative ? undefined : name ? `${name}的虚拟角色：${avatar.name}` : `虚拟角色：${avatar.name}`}
  aria-hidden={decorative ? "true" : undefined}
>
  {#if avatar.style === "pixel"}
    <svg viewBox="0 0 96 96" shape-rendering="crispEdges" aria-hidden="true">
      <rect x="8" y="8" width="80" height="80" rx="18" fill="var(--avatar-bg)" />
      <rect x="24" y="22" width="48" height="44" fill="var(--avatar-secondary)" />
      <rect x="28" y="26" width="40" height="36" fill="var(--avatar-skin)" />
      <rect x="28" y="20" width="40" height="16" fill="var(--avatar-primary)" />
      <rect x="20" y="30" width="12" height="18" fill="var(--avatar-primary)" />
      <rect x="64" y="26" width="12" height="22" fill="var(--avatar-primary)" />
      <rect x="36" y="42" width="6" height="6" fill="var(--avatar-secondary)" />
      <rect x="56" y="42" width="6" height="6" fill="var(--avatar-secondary)" />
      <rect x="42" y="54" width="14" height="4" fill="var(--avatar-secondary)" />
      <rect x="22" y="64" width="52" height="20" fill="var(--avatar-primary)" />
      <rect x="30" y="66" width="36" height="8" fill="var(--avatar-secondary)" opacity=".22" />
      <rect x="32" y="76" width="12" height="8" fill="var(--avatar-secondary)" />
      <rect x="52" y="76" width="12" height="8" fill="var(--avatar-secondary)" />
    </svg>
  {:else}
    <svg viewBox="0 0 96 96" aria-hidden="true">
      <circle cx="48" cy="48" r="40" fill="var(--avatar-bg)" />
      <path d="M22 84c3-19 12-29 26-29s23 10 26 29" fill="var(--avatar-primary)" />
      <circle cx="48" cy="42" r="20" fill="var(--avatar-skin)" />
      <path d="M28 39c0-16 10-25 22-25 13 0 21 9 20 25-5-6-12-8-20-8-8 0-15 3-22 8Z" fill="var(--avatar-primary)" />
      <path d="M30 32c7-13 27-16 38-4-8-2-14-2-20 1-6 2-11 2-18 3Z" fill="var(--avatar-secondary)" opacity=".3" />
      <circle cx="40" cy="43" r="2.4" fill="var(--avatar-secondary)" />
      <circle cx="56" cy="43" r="2.4" fill="var(--avatar-secondary)" />
      <path d="M42 52c4 3 8 3 12 0" fill="none" stroke="var(--avatar-secondary)" stroke-width="2.5" stroke-linecap="round" />
      <path d="M33 68c5 6 25 6 30 0l8 16H25l8-16Z" fill="var(--avatar-secondary)" opacity=".3" />
    </svg>
  {/if}
</span>

<style>
  .virtual-avatar { display: inline-flex; flex: 0 0 auto; align-items: center; justify-content: center; overflow: hidden; border: 1px solid rgba(60,60,67,.14); border-radius: 24%; background: var(--avatar-bg); box-shadow: inset 0 1px 0 rgba(255,255,255,.72), 0 5px 14px rgba(15,23,42,.1); }
  .virtual-avatar svg { width: 100%; height: 100%; }
  .virtual-avatar-sm { width: 40px; height: 40px; border-radius: 12px; }
  .virtual-avatar-md { width: 58px; height: 58px; border-radius: 16px; }
  .virtual-avatar-lg { width: 82px; height: 82px; border-radius: 22px; }
  .virtual-avatar-xl { width: 118px; height: 118px; border-radius: 28px; }
</style>
