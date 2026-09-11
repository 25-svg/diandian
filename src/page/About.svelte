<script type="ts">
  import { open } from "../lib/invoker";
  import { BookOpen, MessageCircle, Video, Heart } from "lucide-svelte";
  import MacModal from "../lib/components/MacModal.svelte";
  import PageShell from "../lib/components/PageShell.svelte";
  let version = `v${__APP_VERSION__}`;
  let showDonateModal = false;

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

    <!-- Private updates -->
    <div class="space-y-4">
      <h2 class="text-[15px] font-semibold text-[color:var(--mac-label)]">更新说明</h2>
      <div class="mac-card p-4">
        <p class="text-sm text-[color:var(--mac-secondary)]">
          更新由管理员私密发布。应用会在空闲时自动检查并安装，无需访问公开下载页面。
        </p>
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
