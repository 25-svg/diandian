<script lang="ts">
  import AppAuthGate from "../../src/lib/components/AppAuthGate.svelte";
  import { canAccessRoute, defaultRoute, type AppAuthClient, type AppUser } from "../../src/lib/appAuth";

  let gate: AppAuthGate;
  let user: AppUser | null = null;
  let pendingUser: AppUser | null = null;
  const users: Record<string, AppUser> = {
    admin: { id: "manager-1", username: "admin", displayName: "默认管理员", role: "operations_manager", anchorId: null, managedTeamIds: ["team-a"], mustChangePassword: true },
    anchor: { id: "anchor-1", username: "anchor", displayName: "主播甲", role: "anchor", anchorId: "anchor-a", managedTeamIds: [], mustChangePassword: true },
  };
  const client: AppAuthClient = {
    async login(username, password) { if ((username === "admin" ? password !== "111" : password !== "Correct-Password-1") || !users[username]) throw new Error("INVALID_CREDENTIALS"); pendingUser = { ...users[username] }; return pendingUser; },
    async me() { return user; },
    async changePassword(_current, next) { if (next.length < 10 || !pendingUser) throw new Error("INVALID_PASSWORD"); pendingUser = { ...pendingUser, mustChangePassword: false }; return pendingUser; },
    async logout() {},
    hasSession() { return Boolean(user); },
  };
</script>

<AppAuthGate bind:this={gate} {client} on:authenticated={(event) => user = event.detail} on:signedOut={() => user = null}>
  {#if user}
    <section data-testid="workspace">
      <h1>{user.role === "operations_manager" ? "负责人端" : "主播端"}</h1>
      <p data-testid="default-route">{defaultRoute(user.role)}</p>
      <p data-testid="route-scope">{canAccessRoute(user.role, "运营总览") ? "可访问运营总览" : "不可访问运营总览"}</p>
      <button type="button" on:click={() => gate.logout()}>退出登录</button>
    </section>
  {/if}
</AppAuthGate>
