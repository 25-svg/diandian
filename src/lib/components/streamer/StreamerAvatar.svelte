<script lang="ts">
  import {
    Bird,
    Cat,
    Dog,
    Fish,
    Rabbit,
    Snail,
    Squirrel,
    Turtle,
  } from "lucide-svelte";

  export let name = "";
  export let avatarIndex = 0;
  export let size: "sm" | "md" | "lg" = "md";

  const icons = [Cat, Dog, Rabbit, Bird, Squirrel, Turtle, Fish, Snail];
  const palettes = [
    ["#e8f2ff", "#0067c0"],
    ["#fff1df", "#b45309"],
    ["#f4ecff", "#7c3aed"],
    ["#e8f8ee", "#147d45"],
    ["#ffe9ef", "#be185d"],
    ["#e8f8f7", "#0f766e"],
    ["#eef0ff", "#4338ca"],
    ["#fff3e8", "#c2410c"],
  ];

  $: normalizedIndex = Math.abs(Number(avatarIndex) || 0) % icons.length;
  $: icon = icons[normalizedIndex];
  $: palette = palettes[normalizedIndex];
</script>

<span
  class="avatar avatar-{size}"
  style={`--avatar-bg:${palette[0]};--avatar-fg:${palette[1]}`}
  role="img"
  aria-label={name ? `${name}的动物头像` : "待确认主播头像"}
>
  <svelte:component this={icon} aria-hidden="true" />
</span>

<style>
  .avatar {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    border: 1px solid rgba(60, 60, 67, 0.12);
    border-radius: 28%;
    color: var(--avatar-fg);
    background:
      radial-gradient(circle at 28% 20%, rgba(255, 255, 255, 0.9), transparent 38%),
      var(--avatar-bg);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.78), 0 5px 14px rgba(15, 23, 42, 0.08);
  }
  .avatar :global(svg) { width: 52%; height: 52%; stroke-width: 1.75; }
  .avatar-sm { width: 40px; height: 40px; border-radius: 12px; }
  .avatar-md { width: 58px; height: 58px; border-radius: 16px; }
  .avatar-lg { width: 82px; height: 82px; border-radius: 22px; }
</style>
