<script>
  import {
    FileVideo,
    Info,
    LayoutDashboard,
    List,
    Settings,
    Users,
    Video,
    Brain,
    History,
    Scissors,
    Sparkles,
    ChartNoAxesCombined,
    MonitorUp,
  } from "lucide-svelte";
  import { hasNewVersion } from "../stores/version";
  import SidebarItem from "./SidebarItem.svelte";
  import { createEventDispatcher } from "svelte";

  const dispatch = createEventDispatcher();

  export let activeUrl = "总览";

  /**
   * @param {{ detail: String; }} route
   */
  function navigate(route) {
    dispatch("activeChange", route.detail);
  }
</script>

<aside class="dd-sidebar">
  <div class="brand">
    <div class="brand-mark" aria-hidden="true">
      <Scissors size={20} strokeWidth={2.4} />
    </div>
    <div class="brand-copy">
      <strong>典典直播切片</strong>
      <span>智能录播 · 成交复盘</span>
    </div>
  </div>

  <div class="nav-label">工作台</div>
  <nav class="nav-list">
    <SidebarItem label="总览" {activeUrl} on:activeChange={navigate}>
      <div slot="icon">
        <LayoutDashboard class="w-5 h-5" />
      </div>
    </SidebarItem>
    <SidebarItem label="直播间" {activeUrl} on:activeChange={navigate}>
      <div slot="icon">
        <Video class="w-5 h-5" />
      </div>
    </SidebarItem>
    <SidebarItem label="录播" {activeUrl} on:activeChange={navigate}>
      <div slot="icon">
        <History class="w-5 h-5" />
      </div>
    </SidebarItem>
    <SidebarItem label="切片" {activeUrl} on:activeChange={navigate}>
      <div slot="icon">
        <FileVideo class="w-5 h-5" />
      </div>
    </SidebarItem>
    <SidebarItem label="任务" {activeUrl} on:activeChange={navigate}>
      <div slot="icon">
        <List class="w-5 h-5" />
      </div>
    </SidebarItem>
    <SidebarItem label="培养看板" {activeUrl} on:activeChange={navigate}>
      <div slot="icon">
        <ChartNoAxesCombined class="w-5 h-5" />
      </div>
    </SidebarItem>
    <SidebarItem label="直播数据大屏" {activeUrl} on:activeChange={navigate}>
      <div slot="icon">
        <MonitorUp class="w-5 h-5" />
      </div>
    </SidebarItem>
    <SidebarItem label="助手" {activeUrl} on:activeChange={navigate}>
      <div slot="icon">
        <Brain class="w-5 h-5" />
      </div>
    </SidebarItem>
    <SidebarItem label="账号" {activeUrl} on:activeChange={navigate}>
      <div slot="icon">
        <Users class="w-5 h-5" />
      </div>
    </SidebarItem>
    <SidebarItem label="设置" {activeUrl} on:activeChange={navigate}>
      <div slot="icon">
        <Settings class="w-5 h-5" />
      </div>
    </SidebarItem>
    <SidebarItem
      label="关于"
      {activeUrl}
      on:activeChange={navigate}
      dot={$hasNewVersion}
    >
      <div slot="icon">
        <Info class="w-5 h-5" />
      </div>
    </SidebarItem>
  </nav>

  <div class="sidebar-footer">
    <div class="ready-dot"></div>
    <div>
      <strong><Sparkles size={13} /> AI 复盘已就绪</strong>
      <span>自动转写并发现片段</span>
    </div>
  </div>
</aside>

<style>
  .dd-sidebar {
    width: 224px;
    height: 100%;
    display: flex;
    flex-direction: column;
    flex: 0 0 224px;
    padding: 18px 12px 14px;
    color: #1d1d1f;
    background: rgba(244, 244, 247, 0.78);
    border-right: 1px solid rgba(0, 0, 0, 0.06);
    backdrop-filter: blur(28px) saturate(160%);
  }

  .brand { display: flex; align-items: center; gap: 11px; padding: 4px 8px 22px; }
  .brand-mark {
    width: 38px;
    height: 38px;
    display: grid;
    place-items: center;
    flex: 0 0 38px;
    color: white;
    border-radius: 12px;
    background: linear-gradient(145deg, #43a7ff 0%, #0877ed 55%, #0058c9 100%);
    box-shadow: 0 8px 18px rgba(0, 113, 227, .24), inset 0 1px 0 rgba(255,255,255,.35);
  }
  .brand-copy { min-width: 0; display: flex; flex-direction: column; }
  .brand-copy strong { font-size: 15px; letter-spacing: -.25px; white-space: nowrap; }
  .brand-copy span { margin-top: 2px; color: #86868b; font-size: 10px; white-space: nowrap; }
  .nav-label { padding: 0 12px 7px; color: #9a9aa0; font-size: 10px; font-weight: 650; letter-spacing: .08em; }
  .nav-list { display: flex; flex-direction: column; gap: 3px; }
  .sidebar-footer {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-top: auto;
    padding: 11px;
    border: 1px solid rgba(255,255,255,.86);
    border-radius: 14px;
    background: rgba(255,255,255,.58);
    box-shadow: 0 5px 18px rgba(30, 35, 45, .05);
  }
  .ready-dot { width: 8px; height: 8px; flex: 0 0 8px; border-radius: 50%; background: #30d158; box-shadow: 0 0 0 4px rgba(48,209,88,.12); }
  .sidebar-footer > div:last-child { min-width: 0; display: flex; flex-direction: column; }
  .sidebar-footer strong { display: flex; align-items: center; gap: 4px; color: #3a3a3c; font-size: 10px; }
  .sidebar-footer span { margin-top: 2px; color: #8e8e93; font-size: 9px; }

  :global(.dark) .dd-sidebar { color: #f5f5f7; background: rgba(28,28,30,.84); border-right-color: rgba(255,255,255,.08); }
  :global(.dark) .sidebar-footer { border-color: rgba(255,255,255,.08); background: rgba(255,255,255,.06); }
  :global(.dark) .sidebar-footer strong { color: #f5f5f7; }

  :global(img.text-\[\#0A84FF\]) {
    filter: invert(48%) sepia(85%) saturate(2229%) hue-rotate(198deg)
      brightness(100%) contrast(101%);
  }

  :global(img.text-gray-700) {
    filter: invert(23%) sepia(10%) saturate(532%) hue-rotate(182deg)
      brightness(94%) contrast(90%);
  }

  :global(.dark img.text-\[\#0A84FF\]) {
    filter: invert(48%) sepia(85%) saturate(2229%) hue-rotate(198deg)
      brightness(100%) contrast(101%);
  }
</style>
