<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import { createLicenseClient, resolveLicenseView, type LicenseStatusDto } from "../license";
  import LicenseBanner from "./LicenseBanner.svelte";

  export let bypass = false;

  const client = createLicenseClient();
  const dispatch = createEventDispatcher<{ authorized: void }>();
  let license: LicenseStatusDto | null = null;
  let loading = !bypass;
  let pending: "" | "activate" | "renew" = "";
  let code = "";
  let label = "";
  let showActivation = false;
  let announced = false;
  let mounted = true;
  $: view = resolveLicenseView(license);
  $: allowed = bypass || view === "app" || view === "app-with-warning";
  $: if (allowed && !announced) {
    announced = true;
    dispatch("authorized");
  }

  async function refresh() {
    if (pending || loading) return;
    pending = "renew";
    const result = await client.renew();
    if (mounted) { license = result; pending = ""; }
  }
  async function activate() {
    if (pending || !code.trim() || !label.trim()) return;
    pending = "activate";
    const result = await client.activate(code, label);
    if (mounted) {
      license = result;
      pending = "";
      const activatedView = resolveLicenseView(result);
      if (activatedView === "app" || activatedView === "app-with-warning") { code = ""; showActivation = false; }
    }
  }
  onMount(() => {
    if (bypass) {
      loading = false;
      return;
    }
    void (async () => {
      let result = await client.status();
      // Confirm existing authorization online before mounting business content.
      if (["valid", "offline_grace", "expired", "clock_invalid"].includes(result.status)) result = await client.renew();
      if (mounted) { license = result; loading = false; }
    })();
    // Re-evaluate the local deadline while a long-running window remains open.
    const timer = setInterval(() => {
      if (loading || pending || !allowed) return;
      void client.status().then(result => { if (mounted) license = result; });
    }, 60_000);
    return () => { mounted = false; clearInterval(timer); };
  });
</script>

{#if loading}
  <div class="license-screen" role="status" aria-live="polite">正在验证设备授权…</div>
{:else if allowed}
  <slot />
  {#if view === "app-with-warning"}
    <LicenseBanner daysRemaining={license?.daysRemaining ?? 1} busy={pending !== ""} on:renew={refresh} />
  {/if}
{:else}
  <main class="license-screen">
    <section class="license-card" aria-labelledby="license-title" aria-busy={pending !== ""}>
      <p class="eyebrow">典典直播切片 · 设备授权</p>
      <h1 id="license-title">{view === "activation" ? "激活此电脑" : "需要验证设备授权"}</h1>
      <p class="explanation">首次使用请向管理员领取激活码。授权绑定此电脑，离线最多可使用 7 天。</p>
      <p class:error={view === "blocked"} role={view === "blocked" ? "alert" : "status"} aria-live="polite">{license?.message}</p>
      {#if view === "activation" || showActivation}
        <form on:submit|preventDefault={activate}>
          <label for="license-code">激活码</label>
          <input id="license-code" bind:value={code} maxlength="256" required autocomplete="off" autocapitalize="off" spellcheck="false" aria-describedby="license-code-help" disabled={pending !== ""} />
          <p id="license-code-help" class="helper">粘贴管理员提供的完整激活码，请勿修改其中的字符。</p>
          <label for="license-label">电脑备注</label>
          <input id="license-label" bind:value={label} maxlength="80" required autocomplete="off" placeholder="例如：一号直播间电脑" disabled={pending !== ""} />
          <button class="primary" type="submit" disabled={pending !== "" || !code.trim() || !label.trim()}>{pending === "activate" ? "正在激活…" : "激活此电脑"}</button>
        </form>
      {:else}
        <button class="primary" type="button" disabled={pending !== ""} on:click={refresh}>{pending === "renew" ? "正在验证…" : "联网重试"}</button>
        <button class="secondary" type="button" disabled={pending !== ""} on:click={() => showActivation = true}>使用激活码</button>
      {/if}
      {#if showActivation && view !== "activation"}
        <button class="secondary" type="button" disabled={pending !== ""} on:click={() => showActivation = false}>返回授权验证</button>
      {/if}
    </section>
  </main>
{/if}

<style>
  .license-screen { min-height: 100vh; box-sizing: border-box; display: grid; place-items: center; padding: 24px 16px; background: #edf3fa; color: #172033; font-family: system-ui, -apple-system, "Segoe UI", sans-serif; }
  .license-card { box-sizing: border-box; width: 100%; max-width: 460px; padding: 28px; border: 1px solid #d1dae6; border-radius: 20px; background: #fff; box-shadow: 0 16px 50px #23395b14; overflow-wrap: anywhere; }
  .eyebrow { margin: 0 0 10px; color: #145ac5; font-size: 13px; font-weight: 650; }
  h1 { margin: 0 0 14px; font-size: 25px; line-height: 1.3; }
  p { font-size: 14px; line-height: 1.7; }
  .explanation, .helper { color: #475467; }
  .error { padding: 12px; border-radius: 10px; background: #fff1f0; color: #a02121; }
  form { display: grid; gap: 10px; }
  label { font-size: 14px; font-weight: 650; }
  input { box-sizing: border-box; min-width: 0; width: 100%; min-height: 46px; padding: 10px 12px; border: 1px solid #8996a8; border-radius: 9px; background: #fff; color: #172033; font: inherit; }
  .helper { margin: 0 0 6px; font-size: 12px; }
  button { width: 100%; min-height: 46px; padding: 10px 16px; border-radius: 10px; font: inherit; font-weight: 650; cursor: pointer; }
  .primary { margin-top: 12px; border: 1px solid #145ac5; background: #145ac5; color: #fff; }
  .secondary { margin-top: 8px; background: transparent; border: 1px solid #8996a8; color: #344054; }
  button:disabled { cursor: not-allowed; background: #e2e8f0; border-color: #cbd5e1; color: #475467; }
  input:focus-visible, button:focus-visible { outline: 3px solid #145ac5; outline-offset: 3px; }
  @media (max-width: 420px) { .license-card { padding: 22px 18px; } }
  @media (prefers-reduced-motion: reduce) { * { animation: none; transition: none; } }
</style>
