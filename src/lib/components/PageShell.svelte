<script>
  /** @type {string} */
  export let title = "";
  /** @type {string} */
  export let subtitle = "";
  /** @type {boolean} */
  export let paddedBottom = false;
</script>

<div class="mac-page custom-scrollbar-light" class:pb-28={paddedBottom} on:scroll>
  <div class="mac-page-stack">
    <header class="mac-page-header">
      <div class="min-w-0">
        <h1 class="mac-page-title">{title}</h1>
        {#if subtitle}
          <p class="mac-page-subtitle">{subtitle}</p>
        {/if}
      </div>
      {#if $$slots.actions}
        <div class="mac-page-actions">
          <slot name="actions" />
        </div>
      {/if}
    </header>

    <slot />
  </div>
</div>

<style>
  .mac-page-stack {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .mac-page-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .pb-28 {
    padding-bottom: 7rem;
  }
</style>
