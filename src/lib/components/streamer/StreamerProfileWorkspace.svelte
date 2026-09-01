<script lang="ts">
  import { createEventDispatcher, tick } from "svelte";
  import { ArrowLeft, CheckCircle2, ClipboardCheck, Dumbbell, Eye, Palette, X } from "lucide-svelte";
  import {
    STREAMER_VIRTUAL_AVATARS,
    getStreamerVirtualAvatar,
    type StreamerAvatarStyle,
    type StreamerVirtualAvatarId,
    type StreamerVirtualAvatarSelection,
  } from "../../streamerAvatar";
  import StreamerRadarChart from "./StreamerRadarChart.svelte";
  import StreamerVirtualAvatar from "./StreamerVirtualAvatar.svelte";

  type StreamerProfileTab =
    | "streams"
    | "trends"
    | "training"
    | "clips"
    | "knowledge";

  type StreamerProfileHeader = {
    name: string;
    avatarIndex: number;
    virtualAvatar: StreamerVirtualAvatarSelection;
    sessionCount: number;
    totalDurationLabel: string;
    recentLiveLabel: string;
    trainingStatus: string;
    dataRangeLabel: string;
    scoreConfidenceLabel: string;
    scoreConfidenceDetail?: string;
  };

  type StreamerRadarSeries = {
    key: "current" | "previous";
    label: string;
    values: Record<string, number | null>;
  };

  type StreamerScoreRow = {
    dimensionKey: string;
    dimensionLabel: string;
    currentScore: number | null;
    previousScore: number | null;
    currentSampleCount: number;
    previousSampleCount: number;
    basis: string;
    evidenceCount: number;
  };

  type StreamerTrendPoint = {
    id: string;
    periodLabel: string;
    dimensionLabel: string;
    score: number | null;
    sampleCount: number;
    confidenceLabel?: string;
  };

  type StreamerTrainingIssue = {
    id: string;
    title: string;
    dimensionKey: string;
    dimensionLabel: string;
    detail: string;
    priorityLabel?: string;
    evidenceCount: number;
  };

  type StreamerExcellentClip = {
    id: string;
    title: string;
    dimensionKey: string;
    dimensionLabel: string;
    sourceLabel: string;
    durationLabel?: string;
    evidenceCount: number;
  };

  type StreamerKnowledgeAsset = {
    id: string;
    title: string;
    typeLabel: string;
    statusLabel: string;
    updatedAtLabel?: string;
  };

  type StreamerSkillProfileSetup = {
    totalSessionCount: number;
    analyzedSessionCount: number;
    evidenceCandidateCount: number;
    qualifiedDimensionCount: number;
    minimumDimensionsForRadar: number;
    preparation?: {
      status: "idle" | "preparing" | "ready" | "partial";
      totalCount: number;
      readyCount: number;
      failedCount: number;
    };
  };

  type RadarDimension = { key: string; label: string };
  type EvidenceSource = "score-table" | "training-issue" | "excellent-clip";

  const dimensions: readonly RadarDimension[] = [
    { key: "needs_confirmation", label: "需求确认" },
    { key: "product_expertise", label: "产品专业" },
    { key: "trust_building", label: "信任建立" },
    { key: "expression_structure", label: "表达结构" },
    { key: "pacing_control", label: "节奏控场" },
    { key: "objection_handling", label: "异议处理" },
    { key: "conversion_advancement", label: "成交推进" },
    { key: "risk_compliance", label: "风险合规" },
  ];

  const tabs: readonly { key: StreamerProfileTab; label: string }[] = [
    { key: "streams", label: "全部直播" },
    { key: "trends", label: "能力趋势" },
    { key: "training", label: "待训练问题" },
    { key: "clips", label: "优秀切片" },
    { key: "knowledge", label: "知识库资产" },
  ];

  const emptyProfile: StreamerProfileHeader = {
    name: "待确认主播",
    avatarIndex: 0,
    virtualAvatar: { streamerKey: "", avatarId: "pixel-nova", updatedAt: 0 },
    sessionCount: 0,
    totalDurationLabel: "0 分钟",
    recentLiveLabel: "暂无直播",
    trainingStatus: "未开始",
    dataRangeLabel: "暂无数据",
    scoreConfidenceLabel: "数据不足",
  };
  const emptySkillProfileSetup: StreamerSkillProfileSetup = {
    totalSessionCount: 0,
    analyzedSessionCount: 0,
    evidenceCandidateCount: 0,
    qualifiedDimensionCount: 0,
    minimumDimensionsForRadar: 5,
  };

  export let profile: StreamerProfileHeader = emptyProfile;
  export let radarSeries: readonly StreamerRadarSeries[] = [];
  export let scoreRows: readonly StreamerScoreRow[] = [];
  export let trendPoints: readonly StreamerTrendPoint[] = [];
  export let trainingIssues: readonly StreamerTrainingIssue[] = [];
  export let excellentClips: readonly StreamerExcellentClip[] = [];
  export let knowledgeAssets: readonly StreamerKnowledgeAsset[] = [];
  export let skillProfileSetup: StreamerSkillProfileSetup = emptySkillProfileSetup;
  export let activeTab: StreamerProfileTab = "streams";

  const dispatch = createEventDispatcher<{
    back: void;
    training: void;
    tab: { tab: StreamerProfileTab };
    evidence: {
      dimensionKey: string;
      dimensionLabel: string;
      evidenceCount: number;
      source: EvidenceSource;
      itemId?: string;
    };
    setup: void;
    avatar: { avatarId: StreamerVirtualAvatarId };
  }>();

  let showAvatarPicker = false;
  let avatarPickerStyle: StreamerAvatarStyle = "pixel";
  let pendingAvatarId: StreamerVirtualAvatarId = "pixel-nova";
  let avatarPickerDialog: HTMLDivElement | null = null;

  $: shouldShowRadar = skillProfileSetup.qualifiedDimensionCount
    >= skillProfileSetup.minimumDimensionsForRadar;
  $: skillSetupMessage = skillProfileSetup.qualifiedDimensionCount > 0
    ? `已形成 ${skillProfileSetup.qualifiedDimensionCount}/8 个有效能力维度；补齐至少 ${skillProfileSetup.minimumDimensionsForRadar} 个维度后显示雷达图。`
    : skillProfileSetup.preparation?.status === "preparing"
    ? `基础档案已建立，正在后台准备 ${skillProfileSetup.preparation.readyCount}/${skillProfileSetup.preparation.totalCount} 场录播文稿；完成前不会显示猜测分数。`
    : skillProfileSetup.preparation?.status === "ready"
    ? `基础档案已建立，${skillProfileSetup.preparation.readyCount} 场录播文稿已准备完成；现在可继续由 AI 生成能力画像。`
    : skillProfileSetup.preparation?.status === "partial"
    ? `基础档案已建立，已准备 ${skillProfileSetup.preparation.readyCount}/${skillProfileSetup.preparation.totalCount} 场录播文稿；失败场次可稍后重试。`
    : "先选择代表性录播，再审核能力证据，系统才会生成真实能力线。";
  $: selectedVirtualAvatar = getStreamerVirtualAvatar(profile.virtualAvatar?.avatarId);
  $: growthStage = skillProfileSetup.qualifiedDimensionCount >= 8
    ? "成熟期"
    : skillProfileSetup.qualifiedDimensionCount >= 5
      ? "稳定期"
      : skillProfileSetup.qualifiedDimensionCount >= 2
        ? "成长期"
        : "探索期";
  $: avatarOptions = STREAMER_VIRTUAL_AVATARS.filter((avatar) => avatar.style === avatarPickerStyle);

  function scoreRow(
    dimension: RadarDimension,
    rows: readonly StreamerScoreRow[],
  ): StreamerScoreRow {
    return rows.find((row) => row.dimensionKey === dimension.key) ?? {
      dimensionKey: dimension.key,
      dimensionLabel: dimension.label,
      currentScore: null,
      previousScore: null,
      currentSampleCount: 0,
      previousSampleCount: 0,
      basis: "等待可用分析证据",
      evidenceCount: 0,
    };
  }

  function formatScore(score: number | null): string {
    return typeof score === "number" && Number.isFinite(score)
      ? `${Math.round(score)} 分`
      : "数据不足";
  }

  function tabId(tab: StreamerProfileTab): string {
    return `streamer-profile-tab-${tab}`;
  }

  function panelId(tab: StreamerProfileTab): string {
    return `streamer-profile-panel-${tab}`;
  }

  async function selectTab(tab: StreamerProfileTab, focusTab = false): Promise<void> {
    activeTab = tab;
    dispatch("tab", { tab });
    if (focusTab) {
      await tick();
      document.getElementById(tabId(tab))?.focus();
    }
  }

  function handleTabKeydown(event: KeyboardEvent, tab: StreamerProfileTab): void {
    const index = tabs.findIndex((entry) => entry.key === tab);
    let nextIndex = index;
    if (event.key === "ArrowRight") nextIndex = (index + 1) % tabs.length;
    else if (event.key === "ArrowLeft") nextIndex = (index - 1 + tabs.length) % tabs.length;
    else if (event.key === "Home") nextIndex = 0;
    else if (event.key === "End") nextIndex = tabs.length - 1;
    else return;
    event.preventDefault();
    void selectTab(tabs[nextIndex].key, true);
  }

  function showEvidence(
    dimensionKey: string,
    dimensionLabel: string,
    evidenceCount: number,
    source: EvidenceSource,
    itemId?: string,
  ): void {
    dispatch("evidence", { dimensionKey, dimensionLabel, evidenceCount, source, itemId });
  }

  async function openAvatarPicker(): Promise<void> {
    pendingAvatarId = profile.virtualAvatar?.avatarId || "pixel-nova";
    avatarPickerStyle = getStreamerVirtualAvatar(pendingAvatarId).style;
    showAvatarPicker = true;
    await tick();
    avatarPickerDialog?.focus();
  }

  function closeAvatarPicker(): void {
    showAvatarPicker = false;
  }

  function commitAvatarSelection(): void {
    dispatch("avatar", { avatarId: pendingAvatarId });
    closeAvatarPicker();
  }

  function handleWindowKeydown(event: KeyboardEvent): void {
    if (showAvatarPicker && event.key === "Escape") {
      event.preventDefault();
      closeAvatarPicker();
    }
  }
</script>

<svelte:window on:keydown={handleWindowKeydown} />

<section class="profile-workspace" aria-label={`${profile.name}主播个人档案`}>
  <header class="profile-card">
    <div class="profile-toolbar">
      <button class="quiet-button" type="button" on:click={() => dispatch("back")}>
        <ArrowLeft size={17} aria-hidden="true" />
        返回主播列表
      </button>
      <button class="primary-button" type="button" on:click={() => dispatch("training")}>
        <Dumbbell size={17} aria-hidden="true" />
        进入情景训练
      </button>
    </div>

    <div class="identity-row">
      <StreamerVirtualAvatar selection={profile.virtualAvatar} name={profile.name} size="lg" />
      <div class="identity-copy">
        <p class="eyebrow">公司主播档案</p>
        <h2 class="profile-name-heading">{profile.name}</h2>
        <span class="status-pill"><i aria-hidden="true"></i>{profile.trainingStatus}</span>
        <div class="avatar-growth-row">
          <span>角色：{selectedVirtualAvatar.name}</span>
          <span>成长阶段：{growthStage} · {skillProfileSetup.qualifiedDimensionCount}/8 项能力已有有效证据</span>
          <button class="avatar-picker-trigger" type="button" on:click={openAvatarPicker}>
            <Palette size={15} aria-hidden="true" />
            更换形象
          </button>
        </div>
      </div>
    </div>

    <dl class="profile-metrics">
      <div>
        <dt>直播场次</dt>
        <dd>{profile.sessionCount} 场</dd>
      </div>
      <div>
        <dt>累计时长</dt>
        <dd>{profile.totalDurationLabel}</dd>
      </div>
      <div>
        <dt>最近直播</dt>
        <dd>{profile.recentLiveLabel}</dd>
      </div>
      <div>
        <dt>训练状态</dt>
        <dd>{profile.trainingStatus}</dd>
      </div>
      <div>
        <dt>数据范围</dt>
        <dd>{profile.dataRangeLabel}</dd>
      </div>
      <div>
        <dt>评分置信度</dt>
        <dd>{profile.scoreConfidenceLabel}</dd>
        {#if profile.scoreConfidenceDetail}
          <small>{profile.scoreConfidenceDetail}</small>
        {/if}
      </div>
    </dl>
  </header>

  <section class="skill-card" aria-labelledby="streamer-skill-title">
    <div class="section-heading">
      <div>
        <p class="eyebrow">技能树</p>
        <h2 id="streamer-skill-title">八维能力画像</h2>
      </div>
      <div class="skill-analysis-summary">
        <span
          class="skill-analysis-status"
          class:is-complete={shouldShowRadar}
          class:is-progress={!shouldShowRadar && skillProfileSetup.qualifiedDimensionCount > 0}
          role="status"
          aria-live="polite"
        >
          {#if shouldShowRadar}
            <CheckCircle2 size={14} aria-hidden="true" />
            AI 分析已生成 · {skillProfileSetup.qualifiedDimensionCount}/8 维
          {:else if skillProfileSetup.qualifiedDimensionCount > 0}
            分析进行中 · {skillProfileSetup.qualifiedDimensionCount}/8 维
          {:else}
            待生成 AI 分析
          {/if}
        </span>
        <p>仅统计可追溯的已分析证据或原始录播文稿；当前周期最多与上一周期对比。</p>
      </div>
    </div>

    <div class="skill-layout">
      <div class="radar-region" class:radar-empty={!shouldShowRadar}>
        {#if shouldShowRadar}
          <StreamerRadarChart {dimensions} series={radarSeries.slice(0, 2)} title={`${profile.name}八维能力雷达图`} />
        {:else}
          <div class="skill-setup-empty" role="status" aria-live="polite">
            <ClipboardCheck size={28} aria-hidden="true" />
            <strong>{profile.name}的能力画像尚未建立</strong>
            <p>{skillSetupMessage}</p>
            <dl class="skill-setup-metrics" aria-label="能力画像建档进度">
              <div><dt>已归档</dt><dd>{skillProfileSetup.totalSessionCount} 场</dd></div>
              <div><dt>已完成分析</dt><dd>{skillProfileSetup.analyzedSessionCount} 场</dd></div>
              <div><dt>可审核证据</dt><dd>{skillProfileSetup.evidenceCandidateCount} 条</dd></div>
              {#if skillProfileSetup.preparation && skillProfileSetup.preparation.status !== "idle"}
                <div><dt>后台准备</dt><dd>{skillProfileSetup.preparation.readyCount}/{skillProfileSetup.preparation.totalCount} 场</dd></div>
              {/if}
            </dl>
            <button
              class="primary-button skill-setup-action"
              type="button"
              data-skill-profile-setup
              on:click={() => dispatch("setup")}
            >
              <ClipboardCheck size={16} aria-hidden="true" />
              {skillProfileSetup.preparation?.status === "preparing" ? "查看准备进度" : skillProfileSetup.preparation?.status === "ready" ? "继续 AI 评估" : "建立能力画像"}
            </button>
          </div>
        {/if}
      </div>

      <!-- svelte-ignore a11y-no-noninteractive-tabindex -->
      <div class="score-table-wrap" role="region" tabindex="0" aria-label="八维能力分数表，可横向滚动；使用方向键浏览完整表格">
        <table>
          <caption class="sr-only">{profile.name}能力精确分数、样本、评分依据与证据</caption>
          <thead>
            <tr>
              <th scope="col">能力维度</th>
              <th scope="col">当前周期</th>
              <th scope="col">上一周期</th>
              <th scope="col">样本数</th>
              <th scope="col">评分依据</th>
              <th scope="col">操作</th>
            </tr>
          </thead>
          <tbody>
            {#each dimensions as dimension (dimension.key)}
              {@const row = scoreRow(dimension, scoreRows)}
              <tr>
                <th scope="row">{row.dimensionLabel}</th>
                <td>
                  <span class:insufficient={row.currentScore === null}>
                    {formatScore(row.currentScore)}
                  </span>
                </td>
                <td>
                  <span class:insufficient={row.previousScore === null}>
                    {formatScore(row.previousScore)}
                  </span>
                </td>
                <td class="samples">
                  <span>本期 {row.currentSampleCount}</span>
                  <span>上期 {row.previousSampleCount}</span>
                </td>
                <td class="basis">{row.basis}</td>
                <td>
                  <div class="row-actions">
                    <button
                      type="button"
                      on:click={() => showEvidence(
                        dimension.key,
                        row.dimensionLabel,
                        row.evidenceCount,
                        "score-table",
                      )}
                      aria-label={`查看${row.dimensionLabel}的 ${row.evidenceCount} 条证据`}
                    >
                      <Eye size={15} aria-hidden="true" />
                      证据 {row.evidenceCount}
                    </button>
                  </div>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
  </section>

  <section class="content-card" aria-label="主播档案内容">
    <div class="profile-tabs" role="tablist" aria-label="主播档案分类">
      {#each tabs as tab (tab.key)}
        <button
          id={tabId(tab.key)}
          type="button"
          role="tab"
          aria-selected={activeTab === tab.key}
          aria-controls={panelId(tab.key)}
          tabindex={activeTab === tab.key ? 0 : -1}
          class:active={activeTab === tab.key}
          on:click={() => selectTab(tab.key)}
          on:keydown={(event) => handleTabKeydown(event, tab.key)}
        >{tab.label}</button>
      {/each}
    </div>

    <div
      class="tab-panel"
      id={panelId(activeTab)}
      role="tabpanel"
      aria-labelledby={tabId(activeTab)}
      tabindex="0"
    >
      {#if activeTab === "streams"}
        <slot />
      {:else if activeTab === "trends"}
        {#if trendPoints.length}
          <!-- svelte-ignore a11y-no-noninteractive-tabindex -->
          <div class="asset-table-wrap" role="region" tabindex="0" aria-label="能力趋势表，可横向滚动；使用方向键浏览完整表格">
            <table class="asset-table">
              <thead>
                <tr>
                  <th scope="col">周期</th>
                  <th scope="col">能力维度</th>
                  <th scope="col">分数</th>
                  <th scope="col">样本</th>
                  <th scope="col">置信度</th>
                </tr>
              </thead>
              <tbody>
                {#each trendPoints as point (point.id)}
                  <tr>
                    <td>{point.periodLabel}</td>
                    <th scope="row">{point.dimensionLabel}</th>
                    <td><span class:insufficient={point.score === null}>{formatScore(point.score)}</span></td>
                    <td>{point.sampleCount}</td>
                    <td>{point.confidenceLabel ?? "待计算"}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {:else}
          <div class="empty-state">
            <strong>暂无能力趋势</strong>
            <p>至少形成两个经过审核的评分周期后，这里才会显示变化。</p>
          </div>
        {/if}
      {:else if activeTab === "training"}
        {#if trainingIssues.length}
          <div class="asset-grid">
            {#each trainingIssues as issue (issue.id)}
              <article class="asset-item">
                <div class="asset-heading">
                  <span>{issue.dimensionLabel}</span>
                  {#if issue.priorityLabel}<small>{issue.priorityLabel}</small>{/if}
                </div>
                <h3>{issue.title}</h3>
                <p>{issue.detail}</p>
                <button type="button" on:click={() => showEvidence(
                  issue.dimensionKey,
                  issue.dimensionLabel,
                  issue.evidenceCount,
                  "training-issue",
                  issue.id,
                )}>
                  <Eye size={15} aria-hidden="true" />
                  查看证据 {issue.evidenceCount}
                </button>
              </article>
            {/each}
          </div>
        {:else}
          <div class="empty-state">
            <strong>暂无待训练问题</strong>
            <p>经过审核并确认需要训练的问题会汇总到这里。</p>
          </div>
        {/if}
      {:else if activeTab === "clips"}
        {#if excellentClips.length}
          <div class="asset-grid">
            {#each excellentClips as clip (clip.id)}
              <article class="asset-item">
                <div class="asset-heading">
                  <span>{clip.dimensionLabel}</span>
                  {#if clip.durationLabel}<small>{clip.durationLabel}</small>{/if}
                </div>
                <h3>{clip.title}</h3>
                <p>{clip.sourceLabel}</p>
                <button type="button" on:click={() => showEvidence(
                  clip.dimensionKey,
                  clip.dimensionLabel,
                  clip.evidenceCount,
                  "excellent-clip",
                  clip.id,
                )}>
                  <Eye size={15} aria-hidden="true" />
                  查看证据 {clip.evidenceCount}
                </button>
              </article>
            {/each}
          </div>
        {:else}
          <div class="empty-state">
            <strong>暂无优秀切片</strong>
            <p>人工审核通过的优秀片段会按能力维度归档。</p>
          </div>
        {/if}
      {:else}
        {#if knowledgeAssets.length}
          <div class="knowledge-list">
            {#each knowledgeAssets as asset (asset.id)}
              <article>
                <div>
                  <span>{asset.typeLabel}</span>
                  <h3>{asset.title}</h3>
                </div>
                <div class="knowledge-meta">
                  <strong>{asset.statusLabel}</strong>
                  {#if asset.updatedAtLabel}<small>{asset.updatedAtLabel}</small>{/if}
                </div>
              </article>
            {/each}
          </div>
        {:else}
          <div class="empty-state">
            <strong>暂无知识库资产</strong>
            <p>这里只展示已经关联到该主播的资产，不在此页面执行发布。</p>
          </div>
        {/if}
      {/if}
    </div>
  </section>
</section>

{#if showAvatarPicker}
  <div class="avatar-picker-backdrop" role="presentation" on:click|self={closeAvatarPicker}>
    <div
      class="avatar-picker-dialog"
      bind:this={avatarPickerDialog}
      role="dialog"
      aria-modal="true"
      aria-labelledby="avatar-picker-title"
      tabindex="-1"
    >
      <header>
        <div>
          <p class="eyebrow">角色形象</p>
          <h2 id="avatar-picker-title">选择你的虚拟角色</h2>
          <p>角色只代表个人外观，不会改变能力分数、直播归属或证据。</p>
        </div>
        <button class="dialog-close" type="button" aria-label="关闭角色选择" on:click={closeAvatarPicker}>
          <X size={18} aria-hidden="true" />
        </button>
      </header>
      <div class="avatar-style-tabs" role="tablist" aria-label="角色风格">
        <button type="button" role="tab" aria-selected={avatarPickerStyle === "pixel"} class:active={avatarPickerStyle === "pixel"} on:click={() => avatarPickerStyle = "pixel"}>像素角色</button>
        <button type="button" role="tab" aria-selected={avatarPickerStyle === "virtual"} class:active={avatarPickerStyle === "virtual"} on:click={() => avatarPickerStyle = "virtual"}>虚拟人物</button>
      </div>
      <div class="avatar-option-grid" role="group" aria-label="可选虚拟角色">
        {#each avatarOptions as avatar (avatar.id)}
          <button
            type="button"
            class:selected={pendingAvatarId === avatar.id}
            aria-pressed={pendingAvatarId === avatar.id}
            on:click={() => pendingAvatarId = avatar.id}
          >
            <StreamerVirtualAvatar selection={{ streamerKey: profile.virtualAvatar.streamerKey, avatarId: avatar.id, updatedAt: 0 }} name={avatar.name} size="xl" decorative />
            <strong>{avatar.name}</strong>
            <span>{avatar.style === "pixel" ? "像素角色" : "虚拟人物"}</span>
          </button>
        {/each}
      </div>
      <footer>
        <button class="quiet-button" type="button" on:click={closeAvatarPicker}>取消</button>
        <button class="primary-button" type="button" on:click={commitAvatarSelection}>使用此形象</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .profile-workspace {
    display: grid;
    gap: 16px;
    min-width: 0;
    color: var(--mac-label);
    --streamer-on-primary: #fff;
    --skill-success-bg: rgba(48, 209, 88, 0.14);
    --skill-success-fg: #0b6b38;
    --skill-success-border: rgba(20, 125, 69, 0.3);
  }
  :global(.dark) .profile-workspace {
    --streamer-on-primary: #111214;
    --skill-success-bg: rgba(48, 209, 88, 0.14);
    --skill-success-fg: #86efac;
    --skill-success-border: rgba(134, 239, 172, 0.36);
  }
  .profile-card,
  .skill-card,
  .content-card {
    min-width: 0;
    border: 1px solid var(--mac-separator);
    border-radius: var(--mac-radius-xl);
    background: var(--mac-bg-card);
    box-shadow: 0 10px 30px rgba(15, 23, 42, 0.045);
  }
  .profile-card {
    padding: 20px;
    background:
      radial-gradient(circle at 92% 8%, rgba(0, 113, 227, 0.10), transparent 32%),
      var(--mac-bg-card);
  }
  .profile-toolbar,
  .identity-row,
  .section-heading,
  .asset-heading,
  .knowledge-list article { display: flex; align-items: center; }
  .profile-toolbar { justify-content: space-between; gap: 12px; margin-bottom: 18px; }
  button {
    min-height: 38px;
    border: 1px solid var(--mac-separator);
    border-radius: 10px;
    color: var(--mac-label);
    background: var(--mac-fill);
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: background 120ms ease, border-color 120ms ease, opacity 120ms ease;
  }
  button:hover { background: var(--mac-fill-hover); }
  button:active { opacity: 0.72; }
  button:focus-visible,
  [tabindex="0"]:focus-visible { outline: 3px solid var(--mac-blue); outline-offset: 2px; border-color: var(--mac-blue); }
  .quiet-button,
  .primary-button,
  .row-actions button,
  .asset-item button { display: inline-flex; align-items: center; justify-content: center; gap: 7px; padding: 0 12px; }
  .primary-button { border-color: var(--mac-blue); color: var(--streamer-on-primary); background: var(--mac-blue); }
  .primary-button:hover { background: var(--mac-blue); filter: brightness(0.94); }
  .identity-row { gap: 16px; }
  .identity-copy { min-width: 0; }
  .avatar-growth-row { display: flex; flex-wrap: wrap; align-items: center; gap: 7px 10px; margin-top: 9px; color: var(--mac-secondary); font-size: 11px; line-height: 1.4; }
  .avatar-growth-row > span + span { padding-left: 10px; border-left: 1px solid var(--mac-separator); }
  .avatar-picker-trigger { display: inline-flex; min-height: 30px; align-items: center; gap: 6px; padding: 0 9px; border-color: color-mix(in srgb, var(--mac-blue) 28%, var(--mac-separator)); color: var(--mac-blue); background: var(--mac-blue-soft); font-size: 11px; }
  .eyebrow { margin: 0 0 4px; color: var(--mac-tertiary); font-size: 11px; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; }
  h2, h3, p { overflow-wrap: anywhere; }
  .profile-name-heading { margin: 0; font-size: clamp(24px, 3vw, 32px); line-height: 1.18; letter-spacing: -0.035em; }
  h2 { margin: 0; font-size: 18px; letter-spacing: -0.015em; }
  h3 { margin: 0; font-size: 14px; line-height: 1.4; }
  .status-pill { display: inline-flex; align-items: center; gap: 7px; margin-top: 8px; padding: 4px 9px; border-radius: 999px; color: var(--mac-secondary); background: var(--mac-fill); font-size: 11px; font-weight: 600; }
  .status-pill i { width: 7px; height: 7px; border-radius: 50%; background: var(--mac-blue); box-shadow: 0 0 0 3px var(--mac-blue-soft); }
  .profile-metrics { display: grid; grid-template-columns: repeat(6, minmax(112px, 1fr)); gap: 8px; margin: 20px 0 0; }
  .profile-metrics > div { min-width: 0; padding: 11px 12px; border: 1px solid var(--mac-separator); border-radius: var(--mac-radius-md); background: var(--mac-bg-elevated); }
  .profile-metrics dt { color: var(--mac-secondary); font-size: 10px; font-weight: 600; }
  .profile-metrics dd { margin: 5px 0 0; font-size: 13px; font-weight: 650; line-height: 1.35; }
  .profile-metrics small { display: block; margin-top: 4px; color: var(--mac-secondary); font-size: 10px; line-height: 1.35; }
  .skill-card { padding: 20px; }
  .section-heading { align-items: flex-start; justify-content: space-between; gap: 24px; margin-bottom: 14px; }
  .skill-analysis-summary { display: grid; justify-items: end; gap: 7px; max-width: 520px; }
  .skill-analysis-summary > p { margin: 0; color: var(--mac-secondary); font-size: 12px; line-height: 1.55; text-align: right; }
  .skill-analysis-status { display: inline-flex; min-height: 26px; align-items: center; gap: 5px; padding: 3px 9px; border: 1px solid var(--mac-separator); border-radius: 999px; color: var(--mac-secondary); background: var(--mac-fill); font-size: 11px; font-weight: 700; white-space: nowrap; }
  .skill-analysis-status.is-progress { border-color: color-mix(in srgb, var(--mac-blue) 28%, var(--mac-separator)); color: var(--mac-blue); background: var(--mac-blue-soft); }
  .skill-analysis-status.is-complete { border-color: var(--skill-success-border); color: var(--skill-success-fg); background: var(--skill-success-bg); font-weight: 750; }
  .skill-layout { display: grid; grid-template-columns: minmax(300px, 0.82fr) minmax(0, 1.35fr); gap: 18px; align-items: center; }
  .radar-region { min-width: 0; padding: 8px; border-radius: var(--mac-radius-lg); background: var(--mac-bg); }
  .radar-region.radar-empty { display: grid; min-height: 320px; }
  .skill-setup-empty { display: grid; place-items: center; align-content: center; gap: 10px; min-height: 100%; padding: 24px; border: 1px dashed var(--mac-separator-strong); border-radius: calc(var(--mac-radius-lg) - 4px); color: var(--mac-secondary); text-align: center; }
  .skill-setup-empty > :global(svg) { color: var(--mac-blue); }
  .skill-setup-empty strong { color: var(--mac-label); font-size: 15px; }
  .skill-setup-empty p { max-width: 36ch; margin: 0; font-size: 12px; line-height: 1.55; }
  .skill-setup-metrics { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); width: min(100%, 420px); margin: 4px 0 2px; border: 1px solid var(--mac-separator); border-radius: var(--mac-radius-md); background: var(--mac-bg-card); }
  .skill-setup-metrics > div { min-width: 0; padding: 9px 7px; }
  .skill-setup-metrics > div + div { border-left: 1px solid var(--mac-separator); }
  .skill-setup-metrics dt { color: var(--mac-secondary); font-size: 10px; font-weight: 600; }
  .skill-setup-metrics dd { margin: 4px 0 0; color: var(--mac-label); font-size: 12px; font-weight: 700; }
  .skill-setup-action { min-height: 38px; }
  .score-table-wrap,
  .asset-table-wrap { min-width: 0; overflow-x: auto; border: 1px solid var(--mac-separator); border-radius: var(--mac-radius-lg); }
  table { width: 100%; min-width: 730px; border-collapse: collapse; font-size: 12px; }
  th, td { padding: 9px 8px; border-bottom: 1px solid var(--mac-separator); text-align: left; vertical-align: middle; }
  thead th { color: var(--mac-secondary); background: var(--mac-bg); font-size: 12px; font-weight: 700; white-space: nowrap; }
  tbody th { font-weight: 650; white-space: nowrap; }
  tbody tr:last-child > * { border-bottom: 0; }
  .samples span { display: block; white-space: nowrap; }
  .samples span + span { margin-top: 3px; color: var(--mac-secondary); }
  .basis { min-width: 150px; max-width: 240px; color: var(--mac-secondary); line-height: 1.45; }
  .insufficient { color: var(--mac-secondary); font-weight: 600; white-space: nowrap; }
  .row-actions { display: flex; gap: 5px; }
  .row-actions button { min-height: 32px; padding: 0 8px; white-space: nowrap; }
  .content-card { overflow: hidden; }
  .profile-tabs { display: flex; gap: 4px; padding: 8px; overflow-x: auto; border-bottom: 1px solid var(--mac-separator); background: var(--mac-bg-card); }
  .profile-tabs button { flex: 0 0 auto; min-height: 38px; padding: 0 14px; border-color: transparent; background: transparent; white-space: nowrap; }
  .profile-tabs button.active { border-color: var(--mac-blue); color: var(--mac-label); background: var(--mac-blue-soft); box-shadow: inset 0 -2px var(--mac-blue); }
  .tab-panel { min-width: 0; padding: 16px; }
  .asset-table { min-width: 620px; }
  .asset-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 10px; }
  .asset-item { display: grid; align-content: start; gap: 9px; min-width: 0; padding: 14px; border: 1px solid var(--mac-separator); border-radius: var(--mac-radius-lg); background: var(--mac-bg); }
  .asset-heading { justify-content: space-between; gap: 8px; }
  .asset-heading span { color: var(--mac-blue); font-size: 10px; font-weight: 700; }
  .asset-heading small { color: var(--mac-tertiary); font-size: 10px; }
  .asset-item p { margin: 0; color: var(--mac-secondary); font-size: 11px; line-height: 1.5; }
  .asset-item button { justify-self: start; margin-top: 2px; min-height: 34px; }
  .knowledge-list { display: grid; gap: 8px; }
  .knowledge-list article { justify-content: space-between; gap: 16px; padding: 13px 14px; border: 1px solid var(--mac-separator); border-radius: var(--mac-radius-md); background: var(--mac-bg); }
  .knowledge-list span { color: var(--mac-tertiary); font-size: 10px; }
  .knowledge-list h3 { margin-top: 3px; }
  .knowledge-meta { display: grid; flex: 0 0 auto; gap: 3px; text-align: right; }
  .knowledge-meta strong { color: var(--mac-secondary); font-size: 11px; }
  .knowledge-meta small { color: var(--mac-tertiary); font-size: 10px; }
  .empty-state { display: grid; place-items: center; gap: 6px; min-height: 150px; padding: 24px; border: 1px dashed var(--mac-separator-strong); border-radius: var(--mac-radius-lg); color: var(--mac-secondary); text-align: center; }
  .empty-state strong { color: var(--mac-label); font-size: 14px; }
  .empty-state p { max-width: 520px; margin: 0; font-size: 11px; line-height: 1.55; }
  .sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0; }
  .avatar-picker-backdrop { position: fixed; z-index: 60; inset: 0; display: grid; place-items: center; padding: 20px; background: rgba(17, 24, 39, .38); backdrop-filter: blur(5px); }
  .avatar-picker-dialog { width: min(680px, 100%); max-height: min(720px, calc(100vh - 40px)); overflow: auto; padding: 20px; border: 1px solid var(--mac-separator); border-radius: 22px; color: var(--mac-label); background: var(--mac-bg-card); box-shadow: 0 24px 80px rgba(15, 23, 42, .28); outline: none; }
  .avatar-picker-dialog header { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; }
  .avatar-picker-dialog header p:not(.eyebrow) { max-width: 47ch; margin: 7px 0 0; color: var(--mac-secondary); font-size: 12px; line-height: 1.55; }
  .dialog-close { display: inline-grid; flex: 0 0 auto; width: 36px; min-height: 36px; place-items: center; padding: 0; }
  .avatar-style-tabs { display: flex; gap: 8px; margin-top: 18px; }
  .avatar-style-tabs button { min-height: 36px; padding: 0 12px; }
  .avatar-style-tabs button.active { border-color: var(--mac-blue); color: var(--mac-blue); background: var(--mac-blue-soft); }
  .avatar-option-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 10px; margin-top: 14px; }
  .avatar-option-grid button { display: grid; min-width: 0; justify-items: center; gap: 6px; min-height: 0; padding: 11px 7px 10px; color: var(--mac-label); background: var(--mac-bg); text-align: center; }
  .avatar-option-grid button.selected { border-color: var(--mac-blue); background: var(--mac-blue-soft); box-shadow: 0 0 0 2px color-mix(in srgb, var(--mac-blue) 20%, transparent); }
  .avatar-option-grid strong { font-size: 11px; line-height: 1.25; }
  .avatar-option-grid span { color: var(--mac-secondary); font-size: 10px; }
  .avatar-picker-dialog footer { display: flex; justify-content: flex-end; gap: 8px; margin-top: 18px; }
  @media (max-width: 1180px) {
    .profile-metrics { grid-template-columns: repeat(3, minmax(120px, 1fr)); }
    .skill-layout { grid-template-columns: 1fr; }
    .radar-region { max-width: 620px; margin: 0 auto; }
  }
  @media (max-width: 840px) {
    .profile-card, .skill-card { padding: 16px; }
    .profile-toolbar { align-items: stretch; }
    .profile-metrics { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .section-heading { display: grid; gap: 8px; }
    .skill-analysis-summary { max-width: none; justify-items: start; }
    .skill-analysis-summary > p { text-align: left; }
    .asset-grid { grid-template-columns: 1fr; }
    .avatar-option-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  }
  @media (max-width: 560px) {
    .profile-toolbar { display: grid; grid-template-columns: 1fr; }
    .quiet-button, .primary-button { width: 100%; min-height: 42px; }
    .identity-row { align-items: flex-start; }
    .avatar-growth-row { display: grid; gap: 7px; }
    .avatar-growth-row > span + span { padding-left: 0; border-left: 0; }
    .profile-metrics { grid-template-columns: 1fr; }
    .tab-panel { padding: 12px; }
    .knowledge-list article { align-items: flex-start; }
    .skill-setup-metrics { grid-template-columns: 1fr; }
    .skill-setup-metrics > div + div { border-top: 1px solid var(--mac-separator); border-left: 0; }
    .avatar-picker-backdrop { padding: 10px; }
    .avatar-picker-dialog { max-height: calc(100vh - 20px); padding: 16px; border-radius: 18px; }
    .avatar-option-grid { gap: 8px; }
    .avatar-picker-dialog footer { display: grid; grid-template-columns: 1fr; }
  }
  @media (prefers-reduced-motion: reduce) {
    button { transition: none; }
  }
</style>
