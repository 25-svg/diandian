<script>
  import { fade, scale } from "svelte/transition";
  import { createEventDispatcher, onMount, tick } from "svelte";
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
  const FOCUSABLE_SELECTOR = [
    "a[href]",
    "button:not([disabled])",
    "input:not([disabled]):not([type='hidden'])",
    "select:not([disabled])",
    "textarea:not([disabled])",
    "[contenteditable='true']",
    "[tabindex]:not([tabindex='-1'])",
  ].join(",");
  /** @type {HTMLElement | null} */
  let dialogElement = null;
  /** @type {HTMLElement | null} */
  let previouslyFocusedElement = null;
  let initialFocusFrame = 0;

  /** @returns {HTMLElement[]} */
  function focusableElements() {
    if (!dialogElement) return [];
    return Array.from(
      /** @type {NodeListOf<HTMLElement>} */ (
        dialogElement.querySelectorAll(FOCUSABLE_SELECTOR)
      ),
    )
      .filter((element) => element instanceof HTMLElement && element.getClientRects().length > 0);
  }

  /** @param {KeyboardEvent} event */
  function dialogKeydown(event) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      dispatch("close");
      return;
    }
    if (event.key !== "Tab") return;

    const focusable = focusableElements();
    if (focusable.length === 0) {
      event.preventDefault();
      dialogElement?.focus({ preventScroll: true });
      return;
    }
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    const active = document.activeElement;
    if (event.shiftKey && (active === first || !dialogElement?.contains(active))) {
      event.preventDefault();
      last.focus({ preventScroll: true });
    } else if (!event.shiftKey && (active === last || !dialogElement?.contains(active))) {
      event.preventDefault();
      first.focus({ preventScroll: true });
    }
  }

  onMount(() => {
    previouslyFocusedElement = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
    void tick().then(() => {
      initialFocusFrame = requestAnimationFrame(() => {
        const preferred = dialogElement?.querySelector(
          "[data-modal-initial-focus], [autofocus]",
        );
        const target = preferred instanceof HTMLElement
          ? preferred
          : focusableElements()[0] || dialogElement;
        target?.focus({ preventScroll: true });
      });
    });

    return () => {
      cancelAnimationFrame(initialFocusFrame);
      if (previouslyFocusedElement?.isConnected) {
        previouslyFocusedElement.focus({ preventScroll: true });
      }
    };
  });

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
    bind:this={dialogElement}
    class="mac-modal {panelClass}"
    transition:scale={{ duration: 150, start: 0.95 }}
    role="dialog"
    aria-modal="true"
    aria-labelledby={title ? "mac-modal-title" : undefined}
    tabindex="-1"
    on:click|stopPropagation
    on:keydown={dialogKeydown}
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
