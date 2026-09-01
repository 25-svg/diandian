<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { AlertCircle, ArrowRight, CheckCircle2, Radio } from "lucide-svelte";
  import type { StreamerVirtualAvatarSelection } from "../../streamerAvatar";
  import StreamerVirtualAvatar from "./StreamerVirtualAvatar.svelte";

  type DirectoryProfile = {
    key: string;
    name: string;
    avatarIndex: number;
    virtualAvatar: StreamerVirtualAvatarSelection;
    liveCount: number;
    totalDurationSeconds: number;
    recentLiveAt: string | number | Date | null;
    analysisCompleteCount: number;
    skillDimensionCount: number;
    analysisGenerated: boolean;
    trainingStatus: string;
    scoreConfidence: string | number | null;
    isLive?: boolean;
  };

  export let profiles: DirectoryProfile[] = [];
  export let pendingCount = 0;

  const dispatch = createEventDispatcher<{
    select: { key: string };
    pending: void;
  }>();

  function formatDuration(value: number) {
    const totalMinutes = Math.max(0, Math.floor((Number(value) || 0) / 60));
    const hours = Math.floor(totalMinutes / 60);
    const minutes = totalMinutes % 60;
    if (hours <= 0) return `${minutes} 分钟`;
    return minutes > 0 ? `${hours} 小时 ${minutes} 分` : `${hours} 小时`;
  }

  function formatRecentLive(value: DirectoryProfile["recentLiveAt"]) {
    if (value === null || value === undefined || value === "") return "暂无记录";
    const date = value instanceof Date ? value : new Date(value);
    if (Number.isNaN(date.getTime())) return "暂无记录";
    return new Intl.DateTimeFormat("zh-CN", {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
    }).format(date);
  }

  function formatConfidence(value: DirectoryProfile["scoreConfidence"]) {
    if (value === null || value === undefined || value === "") return "数据不足";
    if (typeof value === "number") {
      const percent = value <= 1 ? value * 100 : value;
      return `${Math.max(0, Math.min(100, Math.round(percent)))}%`;
    }
    return value;
  }

  function analysisProgress(profile: DirectoryProfile) {
    const liveCount = Math.max(0, Number(profile.liveCount) || 0);
    const completeCount = Math.max(
      0,
      Math.min(liveCount, Number(profile.analysisCompleteCount) || 0),
    );
    return `${completeCount}/${liveCount}`;
  }

  function profileAccessibleLabel(profile: DirectoryProfile): string {
    return [
      `打开${profile.name}的主播档案`,
      `${Math.max(0, Number(profile.liveCount) || 0)}场直播`,
      `累计${formatDuration(profile.totalDurationSeconds)}`,
      `最近直播${formatRecentLive(profile.recentLiveAt)}`,
      `训练状态${profile.trainingStatus || "未开始"}`,
      `分析完成${analysisProgress(profile)}`,
      profile.analysisGenerated
        ? `AI能力画像已生成${Math.max(0, Number(profile.skillDimensionCount) || 0)}维`
        : "AI能力画像尚未生成",
      `评分置信度${formatConfidence(profile.scoreConfidence)}`,
      profile.isLive === true ? "正在直播" : "",
    ].filter(Boolean).join("；");
  }
</script>

<section class="directory" aria-labelledby="streamer-directory-title">
  <div class="directory-heading">
    <div>
      <p class="eyebrow">公司主播</p>
      <h2 id="streamer-directory-title">主播档案</h2>
      <p class="description">按主播查看直播记录、能力成长和训练进度。</p>
    </div>
    {#if profiles.length > 0}
      <span class="profile-count">{profiles.length} 位已确认主播</span>
    {/if}
  </div>

  {#if profiles.length === 0 && pendingCount <= 0}
    <div class="empty-state">
      <div class="empty-icon" aria-hidden="true"><AlertCircle size={22} /></div>
      <strong>暂无主播档案</strong>
      <span>主播身份确认后，档案会显示在这里。</span>
    </div>
  {:else}
    <div class="profile-grid">
      {#each profiles as profile (profile.key)}
        <button
          type="button"
          class="profile-card"
          data-streamer-key={profile.key}
          aria-label={profileAccessibleLabel(profile)}
          on:click={() => dispatch("select", { key: profile.key })}
        >
          <span class="card-top">
            <StreamerVirtualAvatar
              name={profile.name}
              selection={profile.virtualAvatar}
              size="md"
            />
            <span class="identity">
              <span class="name-row">
                <strong class="profile-name">{profile.name}</strong>
                {#if profile.isLive === true}
                  <span class="live-badge">
                    <span class="live-dot" aria-hidden="true"></span>
                    <Radio size={12} aria-hidden="true" />
                    正在直播
                  </span>
                {/if}
                {#if profile.analysisGenerated}
                  <span class="analysis-badge is-complete" role="status">
                    <CheckCircle2 size={12} aria-hidden="true" />
                    AI 分析已生成 · {profile.skillDimensionCount}/8 维
                  </span>
                {/if}
              </span>
              <span class="training-status">训练状态：{profile.trainingStatus || "未开始"}</span>
            </span>
            <span class="open-icon" aria-hidden="true"><ArrowRight size={17} /></span>
          </span>

          <span class="metric-grid">
            <span class="metric">
              <span class="metric-label">直播场次</span>
              <strong>{Math.max(0, Number(profile.liveCount) || 0)} 场</strong>
            </span>
            <span class="metric">
              <span class="metric-label">累计时长</span>
              <strong>{formatDuration(profile.totalDurationSeconds)}</strong>
            </span>
            <span class="metric">
              <span class="metric-label">最近直播</span>
              <strong>{formatRecentLive(profile.recentLiveAt)}</strong>
            </span>
            <span class="metric">
              <span class="metric-label">分析完成</span>
              <strong>{analysisProgress(profile)}</strong>
            </span>
          </span>

          <span class="confidence-row">
            <span>评分置信度</span>
            <strong>{formatConfidence(profile.scoreConfidence)}</strong>
          </span>
        </button>
      {/each}

      {#if pendingCount > 0}
        <button
          type="button"
          class="profile-card pending-card"
          data-streamer-key="__pending__"
          aria-label={`打开待确认主播列表；共${pendingCount}条录播；身份未识别或存在冲突；不会自动合并到主播档案`}
          on:click={() => dispatch("pending")}
        >
          <span class="card-top">
            <span class="pending-avatar" aria-hidden="true"><AlertCircle size={24} /></span>
            <span class="identity">
              <span class="name-row"><strong class="profile-name">待确认</strong></span>
              <span class="training-status">身份未识别或存在冲突</span>
            </span>
            <span class="open-icon" aria-hidden="true"><ArrowRight size={17} /></span>
          </span>
          <span class="pending-summary">
            <strong>{pendingCount}</strong>
            <span>条录播等待人工确认，不会自动合并到主播档案。</span>
          </span>
        </button>
      {/if}
    </div>
  {/if}
</section>

<style>
  .directory {
    width: 100%;
    min-width: 0;
    --streamer-live-bg: #fff1f5;
    --streamer-live-fg: #9f1239;
    --streamer-live-border: rgba(190, 24, 93, 0.24);
    --streamer-pending-bg: #fff7e6;
    --streamer-pending-fg: #854d0e;
    --streamer-pending-border: rgba(217, 119, 6, 0.24);
    --streamer-success-bg: rgba(48, 209, 88, 0.14);
    --streamer-success-fg: #0b6b38;
    --streamer-success-border: rgba(20, 125, 69, 0.3);
  }

  :global(.dark) .directory {
    --streamer-live-bg: #3b1823;
    --streamer-live-fg: #ffb3c2;
    --streamer-live-border: rgba(255, 179, 194, 0.4);
    --streamer-pending-bg: #392807;
    --streamer-pending-fg: #fcd34d;
    --streamer-pending-border: rgba(252, 211, 77, 0.38);
    --streamer-success-bg: rgba(48, 209, 88, 0.14);
    --streamer-success-fg: #86efac;
    --streamer-success-border: rgba(134, 239, 172, 0.36);
  }

  .directory-heading {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 20px;
    margin-bottom: 18px;
  }

  .eyebrow {
    margin: 0 0 4px;
    color: var(--mac-secondary);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.1em;
  }

  h2 {
    margin: 0;
    color: var(--mac-label);
    font-size: 24px;
    line-height: 1.2;
    letter-spacing: -0.025em;
  }

  .description {
    margin: 7px 0 0;
    color: var(--mac-secondary);
    font-size: 13px;
    line-height: 1.5;
  }

  .profile-count {
    flex: 0 0 auto;
    padding: 6px 10px;
    border: 1px solid var(--mac-separator);
    border-radius: 999px;
    color: var(--mac-secondary);
    background: var(--mac-bg-elevated);
    font-size: 12px;
    font-weight: 650;
    white-space: nowrap;
  }

  .profile-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 310px), 1fr));
    gap: 14px;
    width: 100%;
    min-width: 0;
  }

  .profile-card {
    display: flex;
    min-width: 0;
    min-height: 238px;
    padding: 18px;
    overflow: hidden;
    flex-direction: column;
    gap: 17px;
    border: 1px solid var(--mac-separator);
    border-radius: 18px;
    color: var(--mac-label);
    background:
      radial-gradient(circle at 92% 0%, rgba(0, 103, 192, 0.08), transparent 37%),
      var(--mac-bg-card);
    box-shadow: var(--mac-shadow-md);
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition: transform 150ms ease, border-color 150ms ease, box-shadow 150ms ease;
  }

  .profile-card:hover {
    border-color: var(--mac-blue);
    box-shadow: var(--mac-shadow-lg);
    transform: translateY(-2px);
  }

  .profile-card:focus-visible {
    outline: 3px solid var(--mac-blue);
    outline-offset: 3px;
    border-color: var(--mac-blue);
  }

  .card-top {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .identity {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 5px;
  }

  .name-row {
    display: flex;
    min-width: 0;
    align-items: center;
    flex-wrap: wrap;
    gap: 7px;
  }

  .profile-name {
    min-width: 0;
    overflow: hidden;
    font-size: 17px;
    line-height: 1.25;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .training-status {
    overflow: hidden;
    color: var(--mac-secondary);
    font-size: 12px;
    line-height: 1.4;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .open-icon {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    border-radius: 9px;
    color: var(--mac-secondary);
    background: var(--mac-fill);
    transition: color 150ms ease, transform 150ms ease;
  }

  .profile-card:hover .open-icon {
    color: var(--mac-blue);
    transform: translateX(2px);
  }

  .live-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 7px;
    border: 1px solid var(--streamer-live-border);
    border-radius: 999px;
    color: var(--streamer-live-fg);
    background: var(--streamer-live-bg);
    font-size: 10px;
    font-weight: 700;
    white-space: nowrap;
  }

  .analysis-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 7px;
    border: 1px solid var(--streamer-success-border);
    border-radius: 999px;
    color: var(--streamer-success-fg);
    background: var(--streamer-success-bg);
    font-size: 10px;
    font-weight: 750;
    white-space: nowrap;
  }

  .live-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--streamer-live-fg);
    box-shadow: 0 0 0 3px rgba(225, 29, 72, 0.12);
  }

  .metric-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 9px;
  }

  .metric {
    display: flex;
    min-width: 0;
    padding: 10px;
    flex-direction: column;
    gap: 4px;
    border: 1px solid var(--mac-separator);
    border-radius: 11px;
    background: var(--mac-bg-elevated);
  }

  .metric-label {
    color: var(--mac-secondary);
    font-size: 10px;
  }

  .metric strong {
    overflow: hidden;
    font-size: 12px;
    line-height: 1.35;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .confidence-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-top: auto;
    padding-top: 11px;
    border-top: 1px solid var(--mac-separator);
    color: var(--mac-secondary);
    font-size: 11px;
  }

  .confidence-row strong {
    color: var(--mac-label);
  }

  .pending-card {
    justify-content: flex-start;
    border-style: dashed;
    background:
      radial-gradient(circle at 92% 0%, rgba(217, 119, 6, 0.09), transparent 37%),
      var(--mac-bg-card);
  }

  .pending-avatar,
  .empty-icon {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    width: 58px;
    height: 58px;
    border: 1px solid var(--streamer-pending-border);
    border-radius: 16px;
    color: var(--streamer-pending-fg);
    background: var(--streamer-pending-bg);
  }

  .pending-summary {
    display: flex;
    min-width: 0;
    align-items: baseline;
    gap: 10px;
    padding: 18px 16px;
    border: 1px solid rgba(217, 119, 6, 0.11);
    border-radius: 12px;
    color: var(--mac-secondary);
    background: var(--mac-fill);
    font-size: 12px;
    line-height: 1.55;
  }

  .pending-summary strong {
    flex: 0 0 auto;
    color: var(--mac-label);
    font-size: 28px;
    line-height: 1;
  }

  .empty-state {
    display: flex;
    min-height: 210px;
    padding: 28px;
    align-items: center;
    justify-content: center;
    flex-direction: column;
    gap: 8px;
    border: 1px dashed var(--mac-separator-strong);
    border-radius: 18px;
    color: var(--mac-secondary);
    background: var(--mac-bg-elevated);
    text-align: center;
  }

  .empty-state .empty-icon {
    width: 48px;
    height: 48px;
    margin-bottom: 4px;
    border-color: rgba(15, 23, 42, 0.08);
    color: #64748b;
    background: #f1f5f9;
  }

  .empty-state strong {
    color: var(--mac-label);
    font-size: 15px;
  }

  .empty-state span:last-child {
    font-size: 12px;
    line-height: 1.5;
  }

  @media (max-width: 640px) {
    .directory-heading {
      align-items: flex-start;
      flex-direction: column;
      gap: 10px;
    }

    .profile-grid {
      grid-template-columns: minmax(0, 1fr);
    }

    .profile-card {
      min-height: 0;
      padding: 15px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .profile-card,
    .open-icon {
      transition: none;
    }
  }
</style>
