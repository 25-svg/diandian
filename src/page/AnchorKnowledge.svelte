<script lang="ts">
  import { onMount, tick } from "svelte";
  import {
    AlertTriangle,
    BookOpenCheck,
    CheckCircle2,
    Clock3,
    Database,
    FileCheck2,
    Link2,
    Loader2,
    Plus,
    Search,
    Send,
    ShieldCheck,
    UserRound,
    Users,
    XCircle,
  } from "lucide-svelte";
  import { invoke } from "../lib/invoker";
  import {
    anchorKnowledgeAssetTypeLabel,
    anchorKnowledgeCitationText,
    anchorKnowledgeStatusLabel,
    buildAnchorKnowledgeSearchRequest,
    canReviewAnchorKnowledgeAsset,
    canSubmitAnchorKnowledgeAsset,
    friendlyAnchorKnowledgeError,
    type AnchorKnowledgeAsset,
    type AnchorKnowledgeAssetDetail,
    type AnchorKnowledgeAssetType,
    type AnchorKnowledgeProfile,
    type AnchorKnowledgeScope,
    type AnchorKnowledgeStatus,
  } from "../lib/anchorKnowledge";

  let profiles: AnchorKnowledgeProfile[] = [];
  let activeAnchorId = "";
  let scope: AnchorKnowledgeScope = "private";
  let ownerFilter = "";
  let statusFilter: AnchorKnowledgeStatus | "" = "";
  let typeFilter: AnchorKnowledgeAssetType | "" = "";
  let query = "";
  let assets: AnchorKnowledgeAsset[] = [];
  let selected: AnchorKnowledgeAssetDetail | null = null;
  let loadingProfiles = true;
  let loadingAssets = false;
  let runningAction = false;
  let errorText = "";
  let successText = "";
  let showCreateAnchor = false;
  let newAnchorName = "";
  let newAnchorAliases = "";
  let anchorNameInput: HTMLInputElement | null = null;

  $: activeProfile = profiles.find((profile) => profile.anchorId === activeAnchorId) ?? null;

  function clearFeedback(): void {
    errorText = "";
    successText = "";
  }

  async function openCreateAnchor(): Promise<void> {
    showCreateAnchor = true;
    await tick();
    anchorNameInput?.focus();
  }

  async function loadProfiles(preferredAnchorId = activeAnchorId): Promise<void> {
    loadingProfiles = true;
    clearFeedback();
    try {
      profiles = await invoke<AnchorKnowledgeProfile[]>("list_anchor_knowledge_profiles");
      activeAnchorId = profiles.some((profile) => profile.anchorId === preferredAnchorId)
        ? preferredAnchorId
        : profiles[0]?.anchorId ?? "";
      if (activeAnchorId) await searchAssets();
      else assets = [];
    } catch (error) {
      errorText = friendlyAnchorKnowledgeError(error);
    } finally {
      loadingProfiles = false;
    }
  }

  async function searchAssets(): Promise<void> {
    if (!activeAnchorId) {
      assets = [];
      selected = null;
      return;
    }
    loadingAssets = true;
    clearFeedback();
    try {
      const request = buildAnchorKnowledgeSearchRequest({
        requesterAnchorId: activeAnchorId,
        scope,
        ownerAnchorId: scope === "team" ? ownerFilter || null : activeAnchorId,
        query,
        assetType: typeFilter || null,
        reviewStatus: scope === "private" ? statusFilter || null : "published",
      });
      assets = await invoke<AnchorKnowledgeAsset[]>("search_anchor_knowledge", { request });
      if (!assets.some((asset) => asset.assetId === selected?.asset.assetId)) selected = null;
    } catch (error) {
      errorText = friendlyAnchorKnowledgeError(error);
    } finally {
      loadingAssets = false;
    }
  }

  async function loadAsset(asset: AnchorKnowledgeAsset): Promise<void> {
    clearFeedback();
    try {
      selected = await invoke<AnchorKnowledgeAssetDetail>("get_anchor_knowledge_asset", {
        request: {
          requesterAnchorId: activeAnchorId,
          scope,
          assetId: asset.assetId,
        },
      });
    } catch (error) {
      errorText = friendlyAnchorKnowledgeError(error);
    }
  }

  async function createAnchor(): Promise<void> {
    const displayName = newAnchorName.trim();
    if (!displayName) {
      errorText = "请输入主播名称";
      return;
    }
    runningAction = true;
    clearFeedback();
    try {
      const profile = await invoke<AnchorKnowledgeProfile>("create_anchor_knowledge_profile", {
        request: {
          displayName,
          aliases: newAnchorAliases.split(/[，,]/).map((value) => value.trim()).filter(Boolean),
        },
      });
      showCreateAnchor = false;
      newAnchorName = "";
      newAnchorAliases = "";
      await loadProfiles(profile.anchorId);
      successText = `已建立“${profile.displayName}”独立知识域`;
    } catch (error) {
      errorText = friendlyAnchorKnowledgeError(error);
    } finally {
      runningAction = false;
    }
  }

  async function submitAsset(asset: AnchorKnowledgeAsset): Promise<void> {
    runningAction = true;
    clearFeedback();
    try {
      await invoke("submit_anchor_knowledge_asset", {
        request: {
          anchorId: activeAnchorId,
          assetId: asset.assetId,
          submittedBy: activeAnchorId,
        },
      });
      successText = "候选已提交人工审核";
      await loadProfiles(activeAnchorId);
    } catch (error) {
      errorText = friendlyAnchorKnowledgeError(error);
    } finally {
      runningAction = false;
    }
  }

  async function reviewAsset(asset: AnchorKnowledgeAsset, decision: "publish" | "reject"): Promise<void> {
    if (decision === "publish" && !window.confirm("确认来源、原话和商品事实引用无误，并发布到团队知识库？")) return;
    const reason = decision === "reject"
      ? window.prompt("填写驳回原因，便于后续修正：", "来源或内容需要补充")
      : window.prompt("可填写审核说明：", "主播已核对来源与内容");
    if (reason === null) return;
    if (decision === "reject" && !reason.trim()) {
      errorText = "驳回必须填写原因";
      return;
    }
    runningAction = true;
    clearFeedback();
    try {
      await invoke("review_anchor_knowledge_asset", {
        request: {
          anchorId: activeAnchorId,
          assetId: asset.assetId,
          decision,
          reviewerId: activeAnchorId,
          reason: reason.trim(),
        },
      });
      successText = decision === "publish" ? "已审核发布，团队现在可以检索" : "已驳回候选";
      await loadProfiles(activeAnchorId);
    } catch (error) {
      errorText = friendlyAnchorKnowledgeError(error);
    } finally {
      runningAction = false;
    }
  }

  async function rebuildIndex(): Promise<void> {
    runningAction = true;
    clearFeedback();
    try {
      const count = await invoke<number>("rebuild_anchor_knowledge_search_index", {
        requesterAnchorId: activeAnchorId,
        ownerAnchorId: activeAnchorId,
      });
      successText = `索引已从正式资产重建，共 ${count} 条`;
      await searchAssets();
    } catch (error) {
      errorText = friendlyAnchorKnowledgeError(error);
    } finally {
      runningAction = false;
    }
  }

  function dateText(value: string | null): string {
    if (!value) return "—";
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleString("zh-CN", { hour12: false });
  }

  function handleWindowKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && showCreateAnchor && !runningAction) showCreateAnchor = false;
  }

  onMount(() => void loadProfiles());
</script>

<svelte:window on:keydown={handleWindowKeydown} />

<main class="knowledge-page" aria-labelledby="anchor-knowledge-title">
  <header class="page-header">
    <div>
      <p class="eyebrow">独立归属 · 统一事实 · 人工发布</p>
      <h1 id="anchor-knowledge-title"><BookOpenCheck aria-hidden="true" class="h-6 w-6" />主播知识库</h1>
      <p>候选按主播隔离。商品事实只读引用。审核通过后进入团队联合检索。</p>
    </div>
    <div class="header-actions">
      <button type="button" class="mac-btn" on:click={rebuildIndex} disabled={!activeAnchorId || runningAction}>
        <Database aria-hidden="true" class="h-4 w-4" />重建我的索引
      </button>
      <button type="button" class="mac-btn mac-btn-primary" on:click={openCreateAnchor}>
        <Plus aria-hidden="true" class="h-4 w-4" />新增主播知识库
      </button>
    </div>
  </header>

  {#if errorText}
    <div class="feedback error" role="alert"><AlertTriangle aria-hidden="true" class="h-4 w-4" />{errorText}</div>
  {/if}
  {#if successText}
    <div class="feedback success" role="status"><CheckCircle2 aria-hidden="true" class="h-4 w-4" />{successText}</div>
  {/if}

  {#if loadingProfiles}
    <div class="loading-state"><Loader2 aria-hidden="true" class="h-6 w-6 animate-spin" />正在读取主播知识库</div>
  {:else if profiles.length === 0}
    <section class="empty-state">
      <UserRound aria-hidden="true" class="h-10 w-10" />
      <h2>先建立第一个主播知识库</h2>
      <p>建立后，成交切片、话术和分析建议会按稳定主播编号归档。</p>
      <button type="button" class="mac-btn mac-btn-primary" on:click={openCreateAnchor}>
        <Plus aria-hidden="true" class="h-4 w-4" />新增主播知识库
      </button>
    </section>
  {:else}
    <section class="workspace">
      <aside class="anchor-panel" aria-label="主播知识库列表">
        <h2>主播</h2>
        <div class="anchor-list">
          {#each profiles as profile}
            <button
              type="button"
              class:active={profile.anchorId === activeAnchorId}
              aria-pressed={profile.anchorId === activeAnchorId}
              class="anchor-card"
              on:click={() => { activeAnchorId = profile.anchorId; ownerFilter = ""; void searchAssets(); }}
            >
              <span class="anchor-name"><UserRound aria-hidden="true" class="h-4 w-4" />{profile.displayName}</span>
              <span class="anchor-counts">待审 {profile.pendingReviewCount} · 已发布 {profile.publishedCount}</span>
            </button>
          {/each}
        </div>
        {#if activeProfile}
          <div class="scope-note">
            <ShieldCheck aria-hidden="true" class="h-4 w-4" />
            <span>私有候选只属于 {activeProfile.displayName}</span>
          </div>
        {/if}
      </aside>

      <section class="asset-panel" aria-label="知识资产检索">
        <div class="scope-tabs" role="tablist" aria-label="知识范围">
          <button type="button" role="tab" aria-selected={scope === "private"} class:active={scope === "private"} on:click={() => { scope = "private"; ownerFilter = ""; void searchAssets(); }}>
            <UserRound aria-hidden="true" class="h-4 w-4" />我的候选与资产
          </button>
          <button type="button" role="tab" aria-selected={scope === "team"} class:active={scope === "team"} on:click={() => { scope = "team"; statusFilter = ""; void searchAssets(); }}>
            <Users aria-hidden="true" class="h-4 w-4" />团队已发布
          </button>
        </div>

        <form class="filters" on:submit|preventDefault={searchAssets}>
          <label class="search-field">
            <span class="sr-only">关键词</span>
            <Search aria-hidden="true" class="h-4 w-4" />
            <input bind:value={query} placeholder="搜索话术、商品或建议" />
          </label>
          <label>
            <span>类型</span>
            <select bind:value={typeFilter} on:change={searchAssets}>
              <option value="">全部</option>
              <option value="speech">主播话术</option>
              <option value="deal_clip">成交切片</option>
              <option value="analysis_advice">分析建议</option>
            </select>
          </label>
          {#if scope === "private"}
            <label>
              <span>状态</span>
              <select bind:value={statusFilter} on:change={searchAssets}>
                <option value="">全部</option>
                <option value="candidate">候选</option>
                <option value="pending_review">待审核</option>
                <option value="published">已发布</option>
                <option value="rejected">已驳回</option>
              </select>
            </label>
          {:else}
            <label>
              <span>主播</span>
              <select bind:value={ownerFilter} on:change={searchAssets}>
                <option value="">全部主播</option>
                {#each profiles as profile}<option value={profile.anchorId}>{profile.displayName}</option>{/each}
              </select>
            </label>
          {/if}
          <button type="submit" class="mac-btn" disabled={loadingAssets}><Search aria-hidden="true" class="h-4 w-4" />检索</button>
        </form>

        <div class="asset-list" aria-live="polite">
          {#if loadingAssets}
            <div class="inline-state"><Loader2 aria-hidden="true" class="h-5 w-5 animate-spin" />正在检索</div>
          {:else if assets.length === 0}
            <div class="inline-state"><FileCheck2 aria-hidden="true" class="h-5 w-5" />当前范围暂无匹配资产</div>
          {:else}
            {#each assets as asset}
              <button type="button" class="asset-card" class:selected={selected?.asset.assetId === asset.assetId} aria-pressed={selected?.asset.assetId === asset.assetId} on:click={() => void loadAsset(asset)}>
                <div class="asset-card-top">
                  <span class="type-badge">{anchorKnowledgeAssetTypeLabel(asset.assetType)}</span>
                  <span class:published={asset.reviewStatus === "published"} class:pending={asset.reviewStatus === "pending_review"} class:rejected={asset.reviewStatus === "rejected"} class="status-badge">
                    {anchorKnowledgeStatusLabel(asset.reviewStatus)}
                  </span>
                </div>
                <strong>{asset.title}</strong>
                <p>{asset.body}</p>
                <small>{asset.anchorName} · V{asset.version} · {dateText(asset.publishedAt ?? asset.updatedAt)}</small>
              </button>
            {/each}
          {/if}
        </div>
      </section>

      <aside class="detail-panel" aria-label="知识资产详情">
        {#if selected}
          <div class="detail-heading">
            <div>
              <span class="type-badge">{anchorKnowledgeAssetTypeLabel(selected.asset.assetType)}</span>
              <h2>{selected.asset.title}</h2>
            </div>
            <span class="status-badge" class:published={selected.asset.reviewStatus === "published"} class:pending={selected.asset.reviewStatus === "pending_review"}>
              {anchorKnowledgeStatusLabel(selected.asset.reviewStatus)}
            </span>
          </div>
          <div class="detail-body">{selected.asset.body}</div>
          <dl class="metadata">
            <div><dt>主播</dt><dd>{selected.asset.anchorName}</dd></div>
            <div><dt>版本</dt><dd>V{selected.asset.version}{selected.asset.isCurrent ? " · 当前" : " · 历史"}</dd></div>
            <div><dt>商品</dt><dd>{selected.asset.productId || "未指定"}</dd></div>
            <div><dt>审核人</dt><dd>{selected.asset.reviewedBy || "待审核"}</dd></div>
          </dl>
          <section class="sources">
            <h3><Link2 aria-hidden="true" class="h-4 w-4" />来源引用</h3>
            {#each selected.sources as source}
              <article>
                <strong>{source.sourceKind}</strong>
                <p>{anchorKnowledgeCitationText(source)}</p>
                <code>{source.contentHash}</code>
              </article>
            {/each}
          </section>
          {#if scope === "private"}
            <div class="review-actions">
              {#if canSubmitAnchorKnowledgeAsset(selected.asset)}
                <button type="button" class="mac-btn mac-btn-primary" disabled={runningAction} on:click={() => void submitAsset(selected.asset)}><Send aria-hidden="true" class="h-4 w-4" />提交审核</button>
              {:else if canReviewAnchorKnowledgeAsset(selected.asset)}
                <button type="button" class="mac-btn approve" disabled={runningAction} on:click={() => void reviewAsset(selected.asset, "publish")}><CheckCircle2 aria-hidden="true" class="h-4 w-4" />审核发布</button>
                <button type="button" class="mac-btn reject" disabled={runningAction} on:click={() => void reviewAsset(selected.asset, "reject")}><XCircle aria-hidden="true" class="h-4 w-4" />驳回</button>
              {:else}
                <span><Clock3 aria-hidden="true" class="h-4 w-4" />该版本不可直接改写；需要修改时创建新候选版本。</span>
              {/if}
            </div>
          {/if}
        {:else}
          <div class="detail-empty"><FileCheck2 aria-hidden="true" class="h-8 w-8" /><p>选择一条资产查看正文、来源和审核记录</p></div>
        {/if}
      </aside>
    </section>
  {/if}
</main>

{#if showCreateAnchor}
  <div class="modal-backdrop" role="presentation" on:click|self={() => !runningAction && (showCreateAnchor = false)}>
    <section class="modal-card" role="dialog" aria-modal="true" aria-labelledby="create-anchor-title">
      <h2 id="create-anchor-title">新增主播知识库</h2>
      <p>系统生成稳定主播编号。以后改昵称，不影响历史知识归属。</p>
      <label><span>主播名称</span><input bind:this={anchorNameInput} bind:value={newAnchorName} maxlength="80" placeholder="例如：于千惠" /></label>
      <label><span>历史昵称或别名</span><input bind:value={newAnchorAliases} placeholder="多个别名用逗号分隔" /></label>
      <div class="modal-actions">
        <button type="button" class="mac-btn" disabled={runningAction} on:click={() => showCreateAnchor = false}>取消</button>
        <button type="button" class="mac-btn mac-btn-primary" disabled={runningAction} on:click={createAnchor}>
          {#if runningAction}<Loader2 aria-hidden="true" class="h-4 w-4 animate-spin" />{/if}建立知识库
        </button>
      </div>
    </section>
  </div>
{/if}

<style>
  .knowledge-page { height: 100%; min-height: 0; display: flex; flex-direction: column; gap: 12px; padding: 18px; color: var(--mac-label); overflow: hidden; }
  .page-header { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; }
  .page-header h1 { margin: 2px 0 4px; display: flex; align-items: center; gap: 8px; font-size: 24px; font-weight: 700; }
  .page-header p { margin: 0; color: var(--mac-secondary); font-size: 13px; }
  .eyebrow { text-transform: uppercase; letter-spacing: .08em; font-size: 11px !important; color: var(--mac-blue) !important; font-weight: 700; }
  .header-actions, .modal-actions, .review-actions { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .feedback { min-height: 40px; display: flex; align-items: center; gap: 8px; border-radius: 10px; padding: 8px 12px; font-size: 13px; }
  .feedback.error { color: var(--mac-red); background: color-mix(in srgb, var(--mac-red) 10%, transparent); }
  .feedback.success { color: var(--mac-green); background: color-mix(in srgb, var(--mac-green) 10%, transparent); }
  .loading-state, .empty-state { flex: 1; min-height: 240px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; text-align: center; color: var(--mac-secondary); }
  .empty-state h2 { margin: 0; color: var(--mac-label); font-size: 18px; }
  .empty-state p { margin: 0; max-width: 460px; }
  .workspace { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(180px, .72fr) minmax(340px, 1.35fr) minmax(300px, 1fr); gap: 12px; }
  .anchor-panel, .asset-panel, .detail-panel { min-height: 0; border: 1px solid var(--mac-separator); border-radius: 14px; background: var(--mac-bg-card); overflow: hidden; }
  .anchor-panel { padding: 12px; display: flex; flex-direction: column; }
  .anchor-panel h2 { margin: 0 0 10px; font-size: 13px; color: var(--mac-secondary); }
  .anchor-list { display: flex; flex-direction: column; gap: 6px; overflow-y: auto; }
  .anchor-card { min-height: 58px; width: 100%; display: flex; flex-direction: column; align-items: stretch; justify-content: center; gap: 5px; border: 1px solid transparent; border-radius: 10px; padding: 9px 10px; text-align: left; background: transparent; cursor: pointer; }
  .anchor-card:hover { background: var(--mac-fill-hover); }
  .anchor-card.active { border-color: color-mix(in srgb, var(--mac-blue) 42%, transparent); background: color-mix(in srgb, var(--mac-blue) 10%, transparent); }
  .anchor-name { display: flex; align-items: center; gap: 7px; font-size: 13px; font-weight: 650; }
  .anchor-counts { font-size: 11px; color: var(--mac-secondary); }
  .scope-note { margin-top: auto; padding-top: 12px; display: flex; align-items: flex-start; gap: 7px; color: var(--mac-secondary); font-size: 11px; }
  .asset-panel { display: flex; flex-direction: column; }
  .scope-tabs { display: grid; grid-template-columns: 1fr 1fr; border-bottom: 1px solid var(--mac-separator); }
  .scope-tabs button { min-height: 44px; display: flex; align-items: center; justify-content: center; gap: 7px; border: 0; background: transparent; color: var(--mac-secondary); cursor: pointer; }
  .scope-tabs button.active { color: var(--mac-blue); background: color-mix(in srgb, var(--mac-blue) 9%, transparent); box-shadow: inset 0 -2px var(--mac-blue); }
  .filters { display: grid; grid-template-columns: minmax(150px, 1fr) auto auto auto; gap: 8px; padding: 10px; border-bottom: 1px solid var(--mac-separator); }
  .filters label { display: flex; align-items: center; gap: 6px; }
  .filters label > span:not(.sr-only) { font-size: 11px; color: var(--mac-secondary); }
  .filters input, .filters select, .modal-card input { min-height: 40px; border: 1px solid var(--mac-separator); border-radius: 8px; padding: 0 10px; color: var(--mac-label); background: var(--mac-bg); }
  .search-field { min-height: 40px; border: 1px solid var(--mac-separator); border-radius: 8px; padding: 0 10px; background: var(--mac-bg); }
  .search-field input { min-width: 0; width: 100%; min-height: 36px; padding: 0; border: 0; outline: 0; background: transparent; }
  .asset-list { flex: 1; min-height: 0; display: flex; flex-direction: column; gap: 7px; padding: 10px; overflow-y: auto; }
  .asset-card { width: 100%; min-height: 114px; display: flex; flex-direction: column; align-items: stretch; gap: 7px; border: 1px solid var(--mac-separator); border-radius: 11px; padding: 11px; text-align: left; color: var(--mac-label); background: var(--mac-bg); cursor: pointer; }
  .asset-card:hover, .asset-card.selected { border-color: color-mix(in srgb, var(--mac-blue) 48%, var(--mac-separator)); }
  .asset-card.selected { box-shadow: 0 0 0 2px color-mix(in srgb, var(--mac-blue) 12%, transparent); }
  .asset-card-top, .detail-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: 8px; }
  .asset-card strong { font-size: 14px; }
  .asset-card p { margin: 0; display: -webkit-box; overflow: hidden; color: var(--mac-secondary); font-size: 12px; line-height: 1.5; -webkit-line-clamp: 2; -webkit-box-orient: vertical; }
  .asset-card small { color: var(--mac-tertiary); font-size: 10px; }
  .type-badge, .status-badge { display: inline-flex; align-items: center; min-height: 24px; border-radius: 999px; padding: 2px 8px; font-size: 10px; font-weight: 650; white-space: nowrap; }
  .type-badge { color: var(--mac-blue); background: color-mix(in srgb, var(--mac-blue) 10%, transparent); }
  .status-badge { color: var(--mac-secondary); background: var(--mac-fill-hover); }
  .status-badge.pending { color: var(--mac-orange); background: color-mix(in srgb, var(--mac-orange) 12%, transparent); }
  .status-badge.published { color: var(--mac-green); background: color-mix(in srgb, var(--mac-green) 12%, transparent); }
  .status-badge.rejected { color: var(--mac-red); background: color-mix(in srgb, var(--mac-red) 10%, transparent); }
  .inline-state { min-height: 160px; display: flex; align-items: center; justify-content: center; gap: 8px; color: var(--mac-secondary); font-size: 13px; }
  .detail-panel { padding: 14px; overflow-y: auto; }
  .detail-heading h2 { margin: 8px 0 0; font-size: 18px; line-height: 1.35; }
  .detail-body { margin-top: 14px; white-space: pre-wrap; color: var(--mac-label); font-size: 13px; line-height: 1.65; }
  .metadata { margin: 16px 0 0; display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .metadata div { border-radius: 9px; padding: 9px; background: var(--mac-fill); }
  .metadata dt { color: var(--mac-secondary); font-size: 10px; }
  .metadata dd { margin: 3px 0 0; font-size: 12px; }
  .sources { margin-top: 16px; }
  .sources h3 { display: flex; align-items: center; gap: 7px; margin: 0 0 8px; font-size: 13px; }
  .sources article { margin-bottom: 8px; border-left: 3px solid color-mix(in srgb, var(--mac-blue) 45%, transparent); padding: 7px 9px; background: var(--mac-fill); }
  .sources article strong { font-size: 11px; }
  .sources article p { margin: 3px 0; overflow-wrap: anywhere; color: var(--mac-secondary); font-size: 11px; }
  .sources article code { overflow-wrap: anywhere; color: var(--mac-tertiary); font-size: 9px; }
  .review-actions { margin-top: 16px; border-top: 1px solid var(--mac-separator); padding-top: 12px; }
  .review-actions span { display: flex; align-items: flex-start; gap: 6px; color: var(--mac-secondary); font-size: 11px; }
  .mac-btn.approve { color: var(--mac-green); }
  .mac-btn.reject { color: var(--mac-red); }
  .detail-empty { min-height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 9px; text-align: center; color: var(--mac-secondary); font-size: 12px; }
  .modal-backdrop { position: fixed; inset: 0; z-index: 80; display: grid; place-items: center; padding: 16px; background: rgba(0, 0, 0, .38); }
  .modal-card { width: min(460px, 100%); border: 1px solid var(--mac-separator); border-radius: 16px; padding: 18px; color: var(--mac-label); background: var(--mac-bg-card); box-shadow: 0 20px 60px rgba(0, 0, 0, .24); }
  .modal-card h2 { margin: 0; font-size: 19px; }
  .modal-card > p { color: var(--mac-secondary); font-size: 12px; }
  .modal-card label { display: flex; flex-direction: column; gap: 5px; margin-top: 12px; font-size: 12px; }
  .modal-actions { justify-content: flex-end; margin-top: 18px; }
  button:focus-visible, input:focus-visible, select:focus-visible { outline: 2px solid var(--mac-blue); outline-offset: 2px; }
  @media (max-width: 1120px) { .workspace { grid-template-columns: 180px minmax(330px, 1fr); } .detail-panel { grid-column: 1 / -1; max-height: 42vh; } }
  @media (max-width: 760px) { .knowledge-page { overflow-y: auto; } .page-header { flex-direction: column; } .workspace { display: flex; flex-direction: column; overflow: visible; } .anchor-panel, .asset-panel, .detail-panel { min-height: 240px; overflow: visible; } .filters { grid-template-columns: 1fr 1fr; } .search-field { grid-column: 1 / -1; } }
  @media (prefers-reduced-motion: reduce) { * { scroll-behavior: auto !important; transition: none !important; } }
</style>
