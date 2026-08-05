<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { CheckCircle2, Loader2, RotateCcw, Scale, ShieldAlert } from "lucide-svelte";
  import { masterComparisonPresentation } from "../../archiveAnalysis";
  import {
    evidenceGradeLabel,
    presentSegmentScoreDetails,
    riskStatusLabel,
    retryMasterUpgradeReview,
    segmentDecisionLabel,
    upgradeDecisionLabel,
    upgradeDecisionNextAction,
    type MasterBaseline,
    type MasterComparisonResult,
  } from "../../masterScript";

  export let baseline: MasterBaseline | null = null;
  export let selectedSectionId = 0;
  export let result: MasterComparisonResult | null = null;
  export let loading = false;
  export let error = "";
  export let matchStatus: "idle" | "matched" | "ambiguous" | "unmatched" = "idle";
  const dispatch = createEventDispatcher();
  let retryingUpgrade = false;
  let localUpgradeError = "";

  $: selectedSection = baseline?.sections.find((item) => item.id === selectedSectionId) || null;
  $: qualityReview = result?.comparison.qualityReview || null;
  $: scoreDetails = presentSegmentScoreDetails(qualityReview?.scoreDetails);
  $: upgradeReview = result?.upgradeReview || null;
  $: upgradeReviewError = localUpgradeError || result?.upgradeReviewError || "";
  $: view = result
    ? result.comparison.isMasterSource
      ? {
        tone: "success" as const,
        label: "母稿原文基准",
        detail: "本段来自当前母稿的原始录播，仅作为基准，不参与辅稿筛选。",
      }
      : masterComparisonPresentation({
        admission: result.comparison.admission,
        totalScore: result.comparison.totalScore,
        reasons: result.comparison.gates?.reasons || [],
      })
    : null;

  async function retryUpgradeReview(): Promise<void> {
    if (!result?.upgradeReviewId || retryingUpgrade) return;
    retryingUpgrade = true;
    localUpgradeError = "";
    try {
      const retried = await retryMasterUpgradeReview(result.upgradeReviewId);
      dispatch("upgrade-retried", retried);
    } catch (reason: any) {
      localUpgradeError = reason?.message || String(reason);
    } finally {
      retryingUpgrade = false;
    }
  }
</script>

<section class="master-compare">
  <header>
    <Scale size={17}/>
    <div>
      <strong>和企业标准话术对照</strong>
      <small>{baseline ? `${baseline.master.title} · V${baseline.master.version}` : "尚未发布金典拍拍企业母稿"}</small>
    </div>
  </header>
  <p class="master-review-notice">以下为企业可复用价值评估，须经人工定稿后方可进入母稿培训。</p>

  {#if baseline}
    {#if selectedSection}
      <div class="matched"><CheckCircle2 size={15}/><span>正在对照的标准场景</span><strong>{selectedSection.title}</strong></div>
    {/if}
    {#if loading}
      <div class="state"><Loader2 class="spin" size={20}/>正在按母稿进行片段复盘…</div>
    {:else if error}
      <div class="state warning"><ShieldAlert size={20}/>{error}</div>
    {:else if qualityReview}
      <article
        class:view-success={qualityReview.decision === "support_candidate" || qualityReview.decision === "golden_sentence"}
        class:view-warning={qualityReview.riskStatus !== "passed"}
      >
        <div class="score">{qualityReview.totalScore}<small>企业母稿可复用价值分</small></div>
        <div>
          <h3>{segmentDecisionLabel(qualityReview.decision)}</h3>
          <p>{qualityReview.segmentType} · 只评价本段话术，不代表真实成交效果。</p>
        </div>
      </article>
      <div class="review-status">
        <span>证据：{evidenceGradeLabel(qualityReview.evidenceGrade)}</span>
        <span class:needs-review={qualityReview.riskStatus !== "passed"}>风险：{riskStatusLabel(qualityReview.riskStatus)}</span>
      </div>
      {#if qualityReview.whatIsGood.length}
        <div class="points">
          <strong>为什么值得学</strong>
          {#each qualityReview.whatIsGood as item}<p><CheckCircle2 size={14}/>{item}</p>{/each}
        </div>
      {/if}
      {#if qualityReview.whatNeedsImprovement.length}
        <div class="improvements">
          <strong>下次怎么说更好</strong>
          {#each qualityReview.whatNeedsImprovement as item}<p>{item}</p>{/each}
        </div>
      {/if}
      {#if qualityReview.factsToConfirm.length}
        <div class="risks">
          <strong>使用前还要确认</strong>
          {#each qualityReview.factsToConfirm as item}<p><ShieldAlert size={14}/>{item}</p>{/each}
        </div>
      {/if}
      {#if qualityReview.reusableOriginalSentence}
        <div class="original-sentence">
          <strong>主播原话，仅供学习分析</strong>
          <blockquote>“{qualityReview.reusableOriginalSentence}”</blockquote>
        </div>
      {/if}
      {#if qualityReview.suggestedTrainingVersion}
        <div class="training-version">
          <strong>建议稿，不是主播原话</strong>
          <p>{qualityReview.suggestedTrainingVersion}</p>
        </div>
      {/if}
      {#if qualityReview.recommendedMasterSection}
        <div class="insert">
          <strong>审核通过后建议放到</strong>
          <p>{qualityReview.recommendedMasterSection}</p>
        </div>
      {/if}
      <details class="score-evidence">
        <summary>查看六项评分依据</summary>
        {#each scoreDetails as { label, detail }}
          <div class="score-detail">
            <div><strong>{label}</strong><b>{detail.score}/{detail.maxScore}</b></div>
            <p>{detail.reason}</p>
            {#each detail.evidence as evidence}<small>{evidence}</small>{/each}
          </div>
        {/each}
      </details>
      {#if upgradeReview}
        <section class="upgrade-review">
          <header>
            <div>
              <small>与企业母稿比较</small>
              <strong>{upgradeDecisionLabel(upgradeReview.comparisonDecision)}</strong>
            </div>
            <span>{upgradeReview.sameScene ? "同一场景" : "不同场景"}</span>
          </header>
          <div class="upgrade-block value">
            <strong>比母稿新增了什么</strong>
            <p>{upgradeReview.newValue}</p>
          </div>
          {#if upgradeReview.whyBetter.length}
            <div class="upgrade-block">
              <strong>为什么更好</strong>
              {#each upgradeReview.whyBetter as item}<p><CheckCircle2 size={14}/>{item}</p>{/each}
            </div>
          {/if}
          {#if upgradeReview.duplicateContent.length}
            <div class="upgrade-block">
              <strong>与母稿重复的部分</strong>
              {#each upgradeReview.duplicateContent as item}<p>{item}</p>{/each}
            </div>
          {/if}
          {#if upgradeReview.riskOrUncertainty.length}
            <div class="upgrade-block risk">
              <strong>使用前还要确认</strong>
              {#each upgradeReview.riskOrUncertainty as item}<p><ShieldAlert size={14}/>{item}</p>{/each}
            </div>
          {/if}
          <div class="original-sentence">
            <strong>主播原话</strong>
            <blockquote>“{upgradeReview.originalHostWords}”</blockquote>
          </div>
          <div class="training-version">
            <strong>建议稿，不是主播原话</strong>
            <p>{upgradeReview.trainingSuggestion}</p>
          </div>
          <div class="insert">
            <strong>下一步</strong>
            <p>{upgradeDecisionNextAction(upgradeReview.comparisonDecision)}</p>
            <p>{upgradeReview.recommendedAction}</p>
          </div>
        </section>
      {:else if upgradeReviewError}
        <section class="upgrade-retry">
          <div><strong>母稿比较暂未完成</strong><p>{upgradeReviewError}</p></div>
          <button disabled={!result?.upgradeReviewId || retryingUpgrade} on:click={retryUpgradeReview}>
            {#if retryingUpgrade}<Loader2 class="spin" size={14}/>{:else}<RotateCcw size={14}/>{/if}
            {retryingUpgrade ? "正在重试" : "只重试母稿比较"}
          </button>
        </section>
      {/if}
    {:else if view}
      <article class:view-success={view.tone === "success"} class:view-warning={view.tone === "warning"}>
        <div class="score">{result?.comparison.isMasterSource ? "基准" : result?.comparison.totalScore ?? "-"}<small>{result?.comparison.isMasterSource ? "企业标准原文" : "可复用分"}</small></div>
        <div><h3>{view.label}</h3><p>{view.detail}</p></div>
      </article>
      {#if result?.comparison.verdict}
        <div class="verdict"><strong>系统结论</strong><p>{result.comparison.verdict}</p></div>
      {/if}
      {#if result?.comparison.moduleTags.length}
        <div class="modules"><strong>这段在做什么</strong><div>{#each result.comparison.moduleTags as tag}<span>{tag}</span>{/each}</div></div>
      {/if}
      {#if result?.comparison.improvements.length}
        <div class="points"><strong>值得学习的地方</strong>{#each result.comparison.improvements as item}<p><CheckCircle2 size={14}/>{item}</p>{/each}</div>
      {/if}
      {#if result?.comparison.risks.length}
        <div class="risks"><strong>使用前请确认</strong>{#each result.comparison.risks as item}<p><ShieldAlert size={14}/>{item}</p>{/each}</div>
      {/if}
      {#if result?.comparison.suggestedInsertionPoint}
        <div class="insert"><strong>建议放进企业话术的位置</strong><p>{result.comparison.suggestedInsertionPoint}</p></div>
      {/if}
    {:else if matchStatus === "idle"}
      <div class="state">识别到销售片段后，系统会自动匹配母稿场景并评分。</div>
    {:else}
      <div class="state">正在确认本段对应的母稿场景…</div>
    {/if}
  {:else}
    <div class="state">请先在“切片”中汇总并发布金典拍拍企业母稿。</div>
  {/if}
</section>

<style>
  .master-review-notice {
    margin: 0;
    padding: 9px 12px;
    border-left: 3px solid #d97706;
    background: #fffbeb;
    color: #92400e;
    font-size: 12px;
    line-height: 1.55;
  }

  .master-compare{display:grid;gap:10px;margin:12px 0;padding:13px;border:1px solid rgba(0,0,0,.07);border-radius:8px;background:#f8f9fb}.master-compare>header{display:flex;align-items:center;gap:8px}.master-compare>header div{display:grid;gap:2px}.master-compare>header small{color:#86868b;font-size:10px}.matched{display:flex;align-items:center;gap:6px;padding:8px 10px;color:#087c42;background:#edf9f2;font-size:10px}.matched strong{margin-left:auto;color:#344054}.state{display:flex;align-items:center;gap:8px;padding:12px;color:#6e6e73;font-size:12px}.warning{color:#9a6700}article{display:flex;align-items:center;gap:12px;padding:12px;border-left:3px solid #8e8e93;background:white}.view-success{border-left-color:#34c759}.view-warning{border-left-color:#ff9f0a}.score{display:grid;min-width:72px;font-size:24px;font-weight:700}.score small{max-width:72px;font-size:9px;font-weight:500;line-height:1.3;color:#86868b}h3,p{margin:0}h3{font-size:13px}article p,.verdict p,.points p,.risks p,.insert p,.improvements p,.training-version p,.score-detail p{margin-top:4px;color:#6e6e73;font-size:11px;line-height:1.5}.verdict,.points,.risks,.insert,.modules,.improvements,.original-sentence,.training-version,.score-evidence{padding:10px;background:white}.verdict{border-left:3px solid #1687f8}.verdict>strong,.points>strong,.risks>strong,.insert>strong,.modules>strong,.improvements>strong,.original-sentence>strong,.training-version>strong{font-size:11px}.modules>div,.review-status{display:flex;flex-wrap:wrap;gap:5px}.modules span,.review-status span{padding:3px 6px;border-radius:5px;color:#365c7d;background:#edf5fb;font-size:10px}.review-status .needs-review{color:#9a6700;background:#fff7e6}.points p,.risks p{display:flex;gap:6px}.risks{border-left:3px solid #ff9f0a}.improvements{border-left:3px solid #1687f8}.original-sentence blockquote{margin:7px 0 0;padding-left:9px;border-left:2px solid #34c759;color:#344054;font-size:11px;line-height:1.6}.training-version{background:#fff8e8}.score-evidence summary{cursor:pointer;color:#344054;font-size:11px;font-weight:600}.score-detail{display:grid;gap:3px;padding:9px 0;border-bottom:1px solid #eceef2}.score-detail:last-child{border-bottom:0}.score-detail>div{display:flex;justify-content:space-between;font-size:11px}.score-detail small{display:block;color:#667085;font-size:10px;line-height:1.45}.upgrade-review{display:grid;gap:8px;padding-top:10px;border-top:1px solid #d7dce4}.upgrade-review header{display:flex;align-items:center;justify-content:space-between;padding:10px;background:#eef6ff}.upgrade-review header div{display:grid;gap:2px}.upgrade-review header small{color:#667085;font-size:10px}.upgrade-review header strong{font-size:13px}.upgrade-review header span{padding:3px 6px;border-radius:5px;background:#fff;color:#365c7d;font-size:10px}.upgrade-block{padding:10px;background:#fff}.upgrade-block>strong{font-size:11px}.upgrade-block p{display:flex;gap:6px;margin-top:4px;color:#667085;font-size:11px;line-height:1.5}.upgrade-block.value{border-left:3px solid #1687f8}.upgrade-block.risk{border-left:3px solid #ff9f0a}.upgrade-retry{display:flex;align-items:center;justify-content:space-between;gap:12px;padding:11px;border:1px solid #f1c27d;background:#fffaf0}.upgrade-retry strong{font-size:12px}.upgrade-retry p{margin-top:3px;color:#8a5a00;font-size:10px}.upgrade-retry button{display:flex;align-items:center;gap:5px;height:30px;padding:0 9px;border:1px solid #f0a33a;border-radius:6px;background:#fff;color:#8a5a00;font-size:10px;cursor:pointer}.upgrade-retry button:disabled{cursor:not-allowed;opacity:.55}:global(.spin){animation:spin 1s linear infinite}@keyframes spin{to{transform:rotate(360deg)}}
</style>
