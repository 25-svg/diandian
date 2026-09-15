<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { KeyRound, LockKeyhole, Scissors, UserRound } from "lucide-svelte";
  import { createAppAuthClient, type AppAuthClient, type AppUser } from "../appAuth";

  export let client: AppAuthClient = createAppAuthClient();
  const dispatch = createEventDispatcher<{ authenticated: AppUser; signedOut: void }>();
  let user: AppUser | null = null;
  let username = "";
  let password = "";
  let currentPassword = "";
  let newPassword = "";
  let confirmPassword = "";
  let busy = false;
  let error = "";

  function message(cause: unknown): string {
    const code = cause instanceof Error ? cause.message : "";
    if (code === "AUTH_NOT_CONFIGURED") return "登录服务尚未配置，请联系系统管理员。";
    if (code === "INVALID_CREDENTIALS") return "账号或密码错误，连续输错 5 次将暂时锁定。";
    return "登录服务暂时不可用，请检查网络后重试。";
  }

  async function login() {
    if (busy || !username.trim() || !password) return;
    busy = true; error = "";
    try {
      user = await client.login(username, password);
      currentPassword = password; password = "";
      if (!user.mustChangePassword) dispatch("authenticated", user);
    } catch (cause) { error = message(cause); } finally { busy = false; }
  }

  async function changePassword() {
    if (busy || !user) return;
    if (newPassword.length < 10) { error = "新密码至少 10 位。"; return; }
    if (newPassword !== confirmPassword) { error = "两次输入的新密码不一致。"; return; }
    busy = true; error = "";
    try {
      user = await client.changePassword(currentPassword, newPassword);
      currentPassword = ""; newPassword = ""; confirmPassword = "";
      dispatch("authenticated", user);
    } catch { error = "当前密码错误或新密码不符合要求。"; } finally { busy = false; }
  }

  export async function logout() {
    if (busy) return;
    busy = true;
    try { await client.logout(); } finally {
      user = null; username = ""; password = ""; currentPassword = ""; newPassword = ""; confirmPassword = ""; error = ""; busy = false;
      dispatch("signedOut");
    }
  }
</script>

{#if user && !user.mustChangePassword}
  <slot />
{:else}
  <main class="auth-screen">
    <section class="auth-card" aria-labelledby="auth-title" aria-busy={busy}>
      <div class="brand-mark" aria-hidden="true"><Scissors size={25} strokeWidth={2.4} /></div>
      <p class="eyebrow">典典直播切片</p>
      {#if user?.mustChangePassword}
        <h1 id="auth-title">首次登录，请修改密码</h1>
        <p class="explanation">新密码保存后，将按账号角色进入对应工作台。</p>
        {#if error}<div class="error" role="alert" tabindex="-1">{error}</div>{/if}
        <form on:submit|preventDefault={changePassword}>
          <label for="new-password"><LockKeyhole size={16} aria-hidden="true" /> 新密码</label>
          <input id="new-password" type="password" bind:value={newPassword} autocomplete="new-password" minlength="10" maxlength="128" required disabled={busy} />
          <p class="helper">至少 10 位，建议同时包含字母、数字和符号。</p>
          <label for="confirm-password"><LockKeyhole size={16} aria-hidden="true" /> 再次输入新密码</label>
          <input id="confirm-password" type="password" bind:value={confirmPassword} autocomplete="new-password" minlength="10" maxlength="128" required disabled={busy} />
          <button type="submit" disabled={busy || newPassword.length < 10 || newPassword !== confirmPassword}>{busy ? "正在保存…" : "保存并进入"}</button>
        </form>
      {:else}
        <h1 id="auth-title">登录工作台</h1>
        <p class="explanation">管理员进入负责人端，主播进入主播端。账号角色不可自行切换。</p>
        {#if error}<div class="error" role="alert" tabindex="-1">{error}</div>{/if}
        <form on:submit|preventDefault={login}>
          <label for="auth-username"><UserRound size={16} aria-hidden="true" /> 账号</label>
          <input id="auth-username" bind:value={username} autocomplete="username" autocapitalize="none" spellcheck="false" maxlength="64" required disabled={busy} />
          <label for="auth-password"><KeyRound size={16} aria-hidden="true" /> 密码</label>
          <input id="auth-password" type="password" bind:value={password} autocomplete="current-password" maxlength="128" required disabled={busy} />
          <button type="submit" disabled={busy || !username.trim() || !password}>{busy ? "正在登录…" : "登录"}</button>
        </form>
        <p class="helper bottom">忘记密码或账号被停用，请联系系统管理员。</p>
      {/if}
    </section>
  </main>
{/if}

<style>
  .auth-screen { min-height: 100vh; box-sizing: border-box; display: grid; place-items: center; padding: 24px 16px; background: radial-gradient(circle at 50% 0%, #dcecff 0, transparent 38%), #edf3fa; color: #172033; font-family: system-ui, -apple-system, "Segoe UI", sans-serif; }
  .auth-card { box-sizing: border-box; width: min(440px, 100%); padding: 30px; border: 1px solid #d1dae6; border-radius: 22px; background: rgba(255,255,255,.96); box-shadow: 0 20px 64px rgba(35,57,91,.14); }
  .brand-mark { width: 50px; height: 50px; display: grid; place-items: center; margin-bottom: 18px; border-radius: 15px; background: linear-gradient(145deg,#43a7ff,#0877e8 58%,#0058c9); color: #fff; box-shadow: 0 9px 22px rgba(0,113,227,.24); }
  .eyebrow { margin: 0 0 8px; color: #145ac5; font-size: 13px; font-weight: 700; }
  h1 { margin: 0; font-size: 27px; line-height: 1.3; letter-spacing: -.02em; }
  .explanation { margin: 10px 0 22px; color: #475467; font-size: 14px; line-height: 1.7; }
  form { display: grid; gap: 10px; }
  label { display: flex; align-items: center; gap: 7px; margin-top: 4px; font-size: 14px; font-weight: 700; }
  input { box-sizing: border-box; width: 100%; min-height: 48px; padding: 11px 13px; border: 1px solid #8996a8; border-radius: 10px; background: #fff; color: #172033; font: inherit; }
  input:focus-visible, button:focus-visible { outline: 3px solid #145ac5; outline-offset: 2px; }
  button { min-height: 48px; margin-top: 10px; border: 1px solid #145ac5; border-radius: 10px; background: #145ac5; color: #fff; font: inherit; font-weight: 700; cursor: pointer; }
  button:disabled { border-color: #cbd5e1; background: #e2e8f0; color: #667085; cursor: not-allowed; }
  .error { margin-bottom: 15px; padding: 11px 12px; border: 1px solid #f4c7c7; border-radius: 9px; background: #fff1f0; color: #9f1d1d; font-size: 13px; line-height: 1.5; }
  .helper { margin: -2px 0 4px; color: #667085; font-size: 12px; line-height: 1.5; }
  .helper.bottom { margin: 18px 0 0; text-align: center; }
  @media (max-width: 420px) { .auth-card { padding: 24px 18px; } h1 { font-size: 24px; } }
  @media (prefers-reduced-motion: reduce) { * { animation: none !important; transition: none !important; } }
</style>
