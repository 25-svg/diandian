<script>
  import { fade, scale } from "svelte/transition";
  import { createEventDispatcher } from "svelte";
  import { X } from "lucide-svelte";

  /** @type {string} */
  export let title = "";
  /** Extra classes on the panel (width, selectors like delete-modal) */
  export let panelClass = "w-[400px]";
  /** @type {boolean} */
  export let closeOnBackdrop = false;
  /** @type {boolean} */
  export let showClose = false;
  /** Skip header chrome — content owns full layout */
  export let bare = false;

  const dispatch = createEventDispatcher();

  /**
   * @param {MouseEvent} event
   */
  function backdropClick(event) {
    if (!closeOnBackdrop) return;
    if (event.target === event.currentTarget) {
      dispatch("close");
    }
  }
</script>

<!-- svelte-ignore a11y-click-events-have-key-events -->
<!-- svelte-ignore a11y-no-static-element-interactions -->
<div
  class="mac-modal-backdrop"
  transition:fade={{ duration: 200 }}
  on:click={backdropClick}
  role="presentation"
>
  <div
    class="mac-modal {panelClass}"
    transition:scale={{ duration: 150, start: 0.95 }}
    role="dialog"
    aria-modal="true"
    aria-labelledby={title ? "mac-modal-title" : undefined}
    on:click|stopPropagation
  >
    {#if !bare && (title || $$slots.header || showClose)}
      <div class="mac-modal-header">
        <div class="mac-modal-header-main min-w-0 flex-1">
          <slot name="header">
            {#if title}
              <h2 id="mac-modal-title" class="mac-modal-title">{title}</h2>
            {/if}
          </slot>
        </div>
        {#if showClose}
          <button
            type="button"
            class="mac-modal-close"
            aria-label="关闭"
            on:click={() => dispatch("close")}
          >
            <X class="w-5 h-5" />
          </button>
        {/if}
      </div>
    {/if}

    <div class="mac-modal-body" class:mac-modal-body-flush={bare}>
      <slot />
    </div>

    {#if $$slots.actions}
      <div class="mac-modal-footer">
        <slot name="actions" />
      </div>
    {/if}
  </div>
</div>
