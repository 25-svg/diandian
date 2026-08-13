<script lang="ts">
  import { get, invoke, invokeSensitive } from "../lib/invoker";
  import { scale } from "svelte/transition";
  import QRCode from "qrcode";
  import type { AccountItem, AccountInfo } from "../lib/db";
  import { Ellipsis, Plus } from "lucide-svelte";
  import MacModal from "../lib/components/MacModal.svelte";
  import PageShell from "../lib/components/PageShell.svelte";

  let account_info: AccountInfo = {
    accounts: [],
  };

  let avatar_cache: Map<string, string> = new Map();

  async function update_accounts() {
    let new_account_info = (await invoke("get_accounts")) as AccountInfo;
    // Render account identity immediately. Remote avatar loading is optional and
    // must never keep an already-saved account out of the list.
    account_info = new_account_info;
    for (const account of new_account_info.accounts) {
      if (account.avatar === "") {
        account.avatar = platform_avatar(account.platform);
        continue;
      }
      if (avatar_cache.has(account.avatar)) {
        account.avatar = avatar_cache.get(account.avatar);
        continue;
      }
      try {
        const originalAvatar = account.avatar;
        const avatar_response = await get(originalAvatar);
        if (!avatar_response.ok) {
          throw new Error(`HTTP ${avatar_response.status}`);
        }
        const avatar_blob = await avatar_response.blob();
        const avatar_url = URL.createObjectURL(avatar_blob);
        avatar_cache.set(originalAvatar, avatar_url);
        account.avatar = avatar_url;
      } catch (error) {
        console.warn("加载账号头像失败，使用平台默认图标", error);
        account.avatar = platform_avatar(account.platform);
      }
    }
    account_info = {
      ...new_account_info,
      accounts: [...new_account_info.accounts],
    };
  }

  update_accounts();

  let addModal = false;
  let activeTab = "qr"; // 'qr' or 'manual'
  let selectedPlatform = "douyin";
  let oauth_key = "";
  let check_interval = null;
  let cookie_str = "";
  let douyinLoginStatus = "点击下方按钮，打开抖音官方扫码登录窗口";

  let manualModal = false;

  let activeDropdown = null;

  function toggleDropdown(uid) {
    if (activeDropdown === uid) {
      activeDropdown = null;
    } else {
      activeDropdown = uid;
    }
  }

  // Close dropdown when clicking outside
  function handleClickOutside(event) {
    if (
      activeDropdown !== null &&
      !event.target.closest(".dropdown-container")
    ) {
      activeDropdown = null;
    }
  }

  async function handle_qr() {
    if (check_interval) {
      clearInterval(check_interval);
    }
    let qr_info: { url: string; oauthKey: string } = await invoke("get_qr");
    oauth_key = qr_info.oauthKey;
    const canvas = document.getElementById("qr");
    QRCode.toCanvas(canvas, qr_info.url, function (error) {
      if (error) {
        console.log(error);
        return;
      }
      canvas.style.display = "block";
      check_interval = setInterval(check_qr, 2000);
    });
  }

  async function check_qr() {
    let qr_status: { code: number; cookies: string } = await invoke(
      "get_qr_status",
      { qrcodeKey: oauth_key },
    );
    if (qr_status.code == 0) {
      clearInterval(check_interval);
      await invoke("add_account", {
        cookies: qr_status.cookies,
        platform: selectedPlatform,
      });
      await update_accounts();
      addModal = false;
    }
  }

  async function handle_douyin_login() {
    if (check_interval) {
      clearInterval(check_interval);
    }
    douyinLoginStatus = "请在新窗口点右上角红色「登录」扫码；若出现拒绝访问/403，点刷新或关掉窗口重开一次";
    try {
      await invoke("open_douyin_login");
      check_interval = setInterval(check_douyin_login, 2000);
    } catch (e) {
      douyinLoginStatus = `打开登录窗口失败：${e}`;
    }
  }

  async function check_douyin_login() {
    try {
      const cookies = (await invoke("get_douyin_login_cookies")) as
        | string
        | null;
      if (!cookies) return;

      douyinLoginStatus = "登录成功，正在保存账号…";
      clearInterval(check_interval);
      await invoke("add_account", {
        cookies,
        platform: "douyin",
      });
      // The account is already saved at this point. Closing the login window or
      // loading a remote avatar must not turn a successful login into an error.
      await invoke("close_douyin_login").catch(console.warn);
      await update_accounts();
      addModal = false;
    } catch (e) {
      clearInterval(check_interval);
      douyinLoginStatus = `登录信息校验失败：${e}`;
    }
  }

  async function add_cookie() {
    if (cookie_str == "") {
      return;
    }
    try {
      console.log("add_cookie", selectedPlatform);
      await invoke("add_account", {
        cookies: cookie_str,
        platform: selectedPlatform,
      });
      await update_accounts();
      cookie_str = "";
      addModal = false;
    } catch (e) {
      alert("添加账号失败：" + e);
    }
  }

  function platform_display(platform: string) {
    const platformMap = {
      bilibili: "B站",
      douyin: "抖音",
      huya: "虎牙",
      kuaishou: "快手",
      tiktok: "TikTok",
    };
    return platformMap[platform] || platform;
  }

  function platform_avatar(platform: string) {
    const avatarMap = {
      bilibili: "/imgs/bilibili_avatar.png",
      douyin: "/imgs/douyin.png",
      huya: "/imgs/huya_avatar.png",
      kuaishou: "/imgs/kuaishou.svg",
      tiktok: "/imgs/tiktok.png",
    };
    return avatarMap[platform] || "/imgs/bilibili_avatar.png";
  }
</script>

<svelte:window on:click={handleClickOutside} />

<PageShell title="账号" subtitle={`共 ${account_info.accounts.length} 个账号`}>
  <div slot="actions">
    <button
      type="button"
      class="mac-btn mac-btn-primary"
      on:click={() => {
        addModal = true;
        activeTab = "qr";
      }}
    >
      <Plus class="w-4 h-4" />
      <span>添加账号</span>
    </button>
  </div>

    <!-- Account List -->
    <div class="space-y-3">
      <!-- Online Account -->
      {#each account_info.accounts as account (account.uid)}
        <div
          class="mac-card p-4 hover:border-[color:var(--mac-blue)] transition-colors"
        >
          <div class="flex items-center justify-between">
            <div class="flex items-center space-x-4">
              <div class="relative shrink-0">
                <img
                  alt="avatar"
                  class="w-12 h-12 rounded-full object-cover"
                  src={account.avatar}
                />
              </div>
              <div>
                <div class="flex items-center space-x-2">
                  <span
                    class="inline-flex items-center px-2 py-1 text-xs font-medium rounded-full {account.platform ===
                    'bilibili'
                      ? 'bg-pink-100 text-pink-800 dark:bg-pink-900 dark:text-pink-200'
                      : account.platform === 'douyin' ||
                          account.platform === 'tiktok'
                        ? 'bg-black text-white'
                        : account.platform === 'huya'
                          ? 'text-white'
                          : 'bg-gray-100 text-gray-800 dark:bg-gray-700 dark:text-gray-200'}"
                    style={account.platform === "huya"
                      ? "background-color: #ff9600"
                      : ""}
                  >
                    {platform_display(account.platform)}
                  </span>
                  <h3 class="font-medium text-gray-900 dark:text-white">
                    {account.name || account.uid}
                  </h3>
                </div>
                <p class="text-sm text-gray-600 dark:text-gray-400">
                  UID: {account.uid}
                </p>
              </div>
            </div>
            <div class="flex items-center space-x-3">
              <div class="relative dropdown-container">
                <button
                  class="p-2 rounded-lg hover:bg-[#e5e5e5] dark:hover:bg-[#3a3a3c]"
                  on:click|stopPropagation={() => toggleDropdown(account.uid)}
                >
                  <Ellipsis class="w-5 h-5 dark:icon-white" />
                </button>
                {#if activeDropdown === account.uid}
                  <div
                    class="absolute right-0 mt-2 w-48 rounded-lg shadow-lg bg-white dark:bg-[#3c3c3e] border border-gray-200 dark:border-gray-700 backdrop-blur-xl bg-opacity-90 dark:bg-opacity-90"
                    style="transform-origin: top right;"
                    in:scale={{ duration: 100, start: 0.95 }}
                    out:scale={{ duration: 100, start: 0.95 }}
                  >
                    <button
                      class="w-full px-4 py-2 text-left text-sm text-red-600 hover:bg-[#e5e5e5] dark:hover:bg-[#3a3a3c] rounded-t-lg rounded-b-lg"
                      on:click={async () => {
                        if (
                          !window.confirm(
                            `确定注销账号「${account.name || account.uid}」吗？`
                          )
                        ) {
                          activeDropdown = null;
                          return;
                        }
                        try {
                          await invokeSensitive("remove_account", {
                            platform: account.platform,
                            uid: account.uid,
                          });
                          await update_accounts();
                          activeDropdown = null;
                        } catch (error) {
                          alert(`注销账号失败：${error}`);
                          activeDropdown = null;
                        }
                      }}
                    >
                      注销账号
                    </button>
                  </div>
                {/if}
              </div>
            </div>
          </div>
        </div>
      {/each}

      <!-- Add Account Card -->
      <button
        type="button"
        class="w-full p-4 rounded-[14px] border-2 border-dashed border-[color:var(--mac-separator-strong)] hover:border-[color:var(--mac-blue)] transition-colors"
        on:click={() => {
          addModal = true;
          activeTab = "qr";
        }}
      >
        <div class="flex flex-col items-center justify-center space-y-2">
          <div class="w-12 h-12 rounded-full bg-[color:var(--mac-blue-soft)] flex items-center justify-center">
            <Plus class="w-6 h-6 icon-primary" />
          </div>
          <div class="text-center">
            <p class="text-sm font-medium text-[color:var(--mac-blue)]">添加新账号</p>
            <p class="text-xs text-[color:var(--mac-tertiary)]">添加一个新账号，用于获取直播流和投稿</p>
          </div>
        </div>
      </button>
    </div>
</PageShell>

{#if addModal}
  <MacModal
    title="添加账号"
    panelClass="w-[400px]"
    closeOnBackdrop
    showClose
    on:close={() => (addModal = false)}
  >
    <div class="space-y-6">
      <!-- Platform Selection -->
      <div class="hidden">
        <label
          for="platform"
          class="block text-sm font-medium text-[color:var(--mac-secondary)]"
        >
          平台
        </label>
        <div class="mac-segmented overflow-x-auto custom-scrollbar-light">
          <button
            type="button"
            class="flex-none"
            class:mac-segment-active={selectedPlatform === "bilibili"}
            aria-selected={selectedPlatform === "bilibili"}
            on:click={() => {
              selectedPlatform = "bilibili";
              activeTab = "qr";
              requestAnimationFrame(handle_qr);
            }}
          >
            哔哩哔哩
          </button>
          <button
            type="button"
            class="flex-none"
            class:mac-segment-active={selectedPlatform === "douyin"}
            aria-selected={selectedPlatform === "douyin"}
            on:click={() => {
              selectedPlatform = "douyin";
              activeTab = "qr";
            }}
          >
            抖音
          </button>
          <button
            type="button"
            class="flex-none"
            class:mac-segment-active={selectedPlatform === "huya"}
            aria-selected={selectedPlatform === "huya"}
            on:click={() => {
              selectedPlatform = "huya";
              activeTab = "manual";
            }}
          >
            虎牙
          </button>
          <button
            type="button"
            class="flex-none"
            class:mac-segment-active={selectedPlatform === "kuaishou"}
            aria-selected={selectedPlatform === "kuaishou"}
            on:click={() => {
              selectedPlatform = "kuaishou";
              activeTab = "manual";
            }}
          >
            快手
          </button>
          <button
            type="button"
            class="flex-none"
            class:mac-segment-active={selectedPlatform === "tiktok"}
            aria-selected={selectedPlatform === "tiktok"}
            on:click={() => {
              selectedPlatform = "tiktok";
              activeTab = "manual";
            }}
          >
            TikTok
          </button>
        </div>
      </div>

      <!-- Login Methods (Only show for Bilibili) -->
      {#if selectedPlatform === "bilibili"}
        <div class="mac-segmented w-full">
          <button
            type="button"
            class="flex-1"
            class:mac-segment-active={activeTab === "qr"}
            aria-selected={activeTab === "qr"}
            on:click={() => {
              activeTab = "qr";
              requestAnimationFrame(handle_qr);
            }}
          >
            扫码登录
          </button>
          <button
            type="button"
            class="flex-1"
            class:mac-segment-active={activeTab === "manual"}
            aria-selected={activeTab === "manual"}
            on:click={() => {
              activeTab = "manual";
            }}
          >
            手动输入
          </button>
        </div>
      {/if}

      <!-- Tab Content -->
      <div class="space-y-4">
        {#if selectedPlatform === "bilibili" && activeTab === "qr"}
          <div class="flex flex-col items-center space-y-4">
            <div class="bg-white p-4 rounded-lg">
              <canvas id="qr" />
            </div>
            <p class="text-sm text-center text-[color:var(--mac-tertiary)]">
              请使用 BiliBili App 扫描二维码登录
            </p>
          </div>
        {:else if selectedPlatform === "douyin" && activeTab === "qr"}
          <div class="flex flex-col items-center space-y-4 py-2">
            <div
              class="w-16 h-16 rounded-2xl bg-black flex items-center justify-center"
            >
              <img src="/imgs/douyin.png" alt="抖音" class="w-10 h-10" />
            </div>
            <p class="text-sm text-center text-[color:var(--mac-tertiary)]">
              {douyinLoginStatus}
            </p>
            <button
              type="button"
              class="mac-btn mac-btn-primary w-full"
              on:click={handle_douyin_login}
            >
              打开抖音扫码登录
            </button>
            <p class="text-xs text-[color:var(--mac-quaternary)] text-center">
              登录信息只保存在本机，无需复制或查看 Cookie
            </p>
          </div>
        {:else}
          <div class="space-y-4">
            <textarea
              class="mac-field w-full"
              rows="4"
              bind:value={cookie_str}
              placeholder={`请粘贴 ${selectedPlatform} 账号的 Cookie`}
            ></textarea>
            <div class="flex justify-end items-center gap-3">
              {#if selectedPlatform !== "bilibili"}
                <a
                  href="https://bsr.xinrea.cn/getting-started/config/account.html"
                  class="text-sm text-[color:var(--mac-blue)] hover:underline"
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  Cookie 获取教程</a
                >
              {/if}
              <button
                type="button"
                class="mac-btn mac-btn-primary"
                on:click={() => {
                  add_cookie();
                }}
              >
                添加账号
              </button>
            </div>
          </div>
        {/if}
      </div>
    </div>
  </MacModal>
{/if}
