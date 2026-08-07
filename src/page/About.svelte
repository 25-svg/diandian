<script type="ts">
  import { open } from "../lib/invoker";
  import { BookOpen, MessageCircle, Video, Heart } from "lucide-svelte";
  import MacModal from "../lib/components/MacModal.svelte";
  import PageShell from "../lib/components/PageShell.svelte";
  import { hasNewVersion, latestVersion } from "../lib/stores/version";
  let version = `v${__APP_VERSION__}`;
  let showDonateModal = false;
  let releases = [];

  // get releases from github api
  fetch("https://api.github.com/repos/Xinrea/bili-shadowreplay/releases")
    .then((response) => response.json())
    .then((data) => {
      // Filter out prerelease versions
      const stableReleases = data.filter((release) => !release.prerelease);
      const latest = stableReleases[0]?.tag_name;
      latestVersion.set(latest);
      // Compare versions and set hasNewVersion
      if (version && latest !== version) {
        hasNewVersion.set(true);
      }
      releases = stableReleases.slice(0, 3).map((release) => ({
        version: release.tag_name,
        date: new Date(release.published_at).toLocaleDateString(),
        description: release.body,
        url: release.html_url,
      }));
    });

  function formatReleaseNotes(notes) {
    if (!notes) return [];
    return notes
      .split("\n")
      .filter(
        (line) => line.trim().startsWith("*") || line.trim().startsWith("-"),
      )
      .map((line) => {
        line = line.trim().replace(/^[*-]\s*/, "");
        // Remove commit hash at the end (- hash or hash)
        line = line
          .replace(/\s*-\s*[a-f0-9]{40}$/, "")
          .replace(/\s+[a-f0-9]{40}$/, "");
        return line;
      })
      .filter((line) => line.length > 0);
  }

  function toggleDonateModal() {
    showDonateModal = !showDonateModal;
  }
</script>

<PageShell title="关于" subtitle="产品信息、版本说明与支持渠道。">
  <div class="max-w-2xl mx-auto space-y-8">
    <!-- App Info -->
    <div class="text-center space-y-4">
      <div
        class="w-24 h-24 mx-auto rounded-[22px] shadow-mac flex items-center justify-center"
        style="background:linear-gradient(145deg,#43a7ff 0%,var(--mac-blue) 55%,#0058c9 100%)"
      >
        <Video class="w-12 h-12 icon-white" />
      </div>
      <div>
        <h1 class="mac-page-title" style="font-size:28px">典典直播切片</h1>
        <p class="mac-page-subtitle">Version {version}</p>
      </div>
    </div>

    <!-- Quick Actions -->
    <div class="grid grid-cols-3 gap-4">
      <button
        type="button"
        class="mac-card p-4 hover:bg-[color:var(--mac-fill)] transition-colors"
        on:click={() => {
          // tauri open url
          open("https://bsr.xinrea.cn/");
        }}
      >
        <div class="flex flex-col items-center space-y-2">
          <div
            class="w-10 h-10 rounded-full bg-[color:var(--mac-blue-soft)] flex items-center justify-center"
          >
            <BookOpen class="w-5 h-5 icon-primary" />
          </div>
          <span class="text-sm font-medium text-gray-900 dark:text-white"
            >说明</span
          >
        </div>
      </button>
      <button
        type="button" class="mac-card p-4 hover:bg-[color:var(--mac-fill)] transition-colors"
        on:click={() => {
          // tauri open url
          open("https://qm.qq.com/q/v4lrE6gyum");
        }}
      >
        <div class="flex flex-col items-center space-y-2">
          <div
            class="w-10 h-10 rounded-full bg-[color:var(--mac-blue-soft)] flex items-center justify-center"
          >
            <MessageCircle class="w-5 h-5 icon-primary" />
          </div>
          <span class="text-sm font-medium text-gray-900 dark:text-white"
            >反馈交流群</span
          >
        </div>
      </button>
      <button
        type="button" class="mac-card p-4 hover:bg-[color:var(--mac-fill)] transition-colors"
        on:click={toggleDonateModal}
      >
        <div class="flex flex-col items-center space-y-2">
          <div
            class="w-10 h-10 rounded-full bg-pink-500/10 flex items-center justify-center"
          >
            <Heart class="w-5 h-5 text-pink-500" />
          </div>
          <span class="text-sm font-medium text-gray-900 dark:text-white"
            >打赏支持</span
          >
        </div>
      </button>
    </div>

    <!-- What's New -->
    <div class="space-y-4">
      <h2 class="text-[15px] font-semibold text-[color:var(--mac-label)]">更新说明</h2>
      <div class="mac-card">
        {#each releases as release}
          <!-- svelte-ignore a11y-click-events-have-key-events -->
          <div
            class="p-4 cursor-pointer {release !== releases[releases.length - 1]
              ? 'border-b border-gray-200 dark:border-gray-700'
              : ''}"
            on:click={() => {
              open(release.url);
            }}
          >
            <div class="flex items-center justify-between">
              <h3 class="text-sm font-medium text-gray-900 dark:text-white">
                Version {release.version}
              </h3>
              <span class="text-xs text-gray-500 dark:text-gray-400"
                >Released on {release.date}</span
              >
            </div>
            <ul class="mt-2 space-y-1 text-sm text-gray-600 dark:text-gray-300">
              {#each formatReleaseNotes(release.description) as note}
                <li class="flex items-start space-x-2">
                  <span class="text-blue-500">•</span>
                  <span>{note}</span>
                </li>
              {/each}
            </ul>
          </div>
        {/each}
      </div>
    </div>
  </div>
</PageShell>

{#if showDonateModal}
  <MacModal
    title="打赏支持"
    panelClass="w-full max-w-md"
    closeOnBackdrop
    showClose
    on:close={toggleDonateModal}
  >
    <div class="flex justify-center">
      <img
        src="/imgs/donate.png"
        class="max-w-full h-auto rounded-lg"
        alt="打赏二维码"
      />
    </div>
    <p class="mt-4 text-center text-sm text-[color:var(--mac-secondary)]">
      感谢您的支持！
    </p>
  </MacModal>
{/if}
