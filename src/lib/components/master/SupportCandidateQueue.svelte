<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import { Check, Clock3, Download, FileText, RotateCcw, X } from "lucide-svelte";
  import { save } from "@tauri-apps/plugin-dialog";
  import { invoke, TAURI_ENV } from "../../invoker";
  import {
    candidateDecisionStatus,
    buildTrainingGuide,
    decideSupportCandidate,
    getMasterBaseline,
    friendlyMasterError,
    listSupportCandidates,
    previewMasterUpgrade,
    publishMasterUpgrade,
    supportCandidatePresentation,
    supportCandidateCanUpgradeMaster,
    supportCandidateUpgradeReview,
    supportCandidateVersionPresentation,
    trainingGuideFileName,
    upgradePublishGate,
    upgradeDecisionLabel,
    type UpgradeComparisonDecision,
    type MasterUpgradePreview,
    type MasterBaseline,
    type CandidateDecisionLabel,
    type SupportCandidate,
  } from "../../masterScript";
  import MasterVersionDiff from "./MasterVersionDiff.svelte";
  import {
    buildTrainingPack,
    formatTrainingPackMarkdown,
    trainingPackFileName,
  } from "../../trainingPack";
  const dispatch = createEventDispatcher();
  let candidates: SupportCandidate[] = [];
  let loading = true;
  let error = "";
  let runningId = 0;
  let approved: SupportCandidate[] = [];
  let localMaterials: SupportCandidate[] = [];
  let staleCandidates: SupportCandidate[] = [];
  let reviewRequiredCandidates: SupportCandidate[] = [];
  let mergedCandidates: SupportCandidate[] = [];
  let activeMaster: MasterBaseline | null = null;
  let upgrade: MasterUpgradePreview | null = null;
  let diffConfirmed = false;
  let publishing = false;
  let trainingExporting = false;
  let trainingExportMessage = "";
  const actions: CandidateDecisionLabel[] = ["加入下一版企业话术", "保留为局部素材", "不采用"];
  const queueGroups: Array<{ decision: UpgradeComparisonDecision; title: string }> = [
    { decision: "add_as_support", title: "建议新增辅稿" },
    { decision: "add_as_golden_sentence", title: "建议加入金句话术" },
    { decision: "replace_existing", title: "建议人工审核后替换" },
    { decision: "merge_with_existing", title: "建议与现有章节合并" },
  ];
  const time = (ms: number) => { const s = Math.floor(ms / 1000); return `${Math.floor(s / 60)}:${String(s % 60).padStart(2,"0")}`; };
  const explainError = (reason: unknown) => {
    const friendly = friendlyMasterError(reason);
    return `${friendly.title}：${friendly.detail} ${friendly.nextAction}`;
  };
  async function load(): Promise<void> {
    loading = true; error = "";
    try {
      const activeKey = activeScriptKey();
      const [allCandidates, baseline] = await Promise.all([
        listSupportCandidates(),
        activeKey ? getMasterBaseline(activeKey) : Promise.resolve(null),
      ]);
      activeMaster = baseline;
      const activeMasterId = baseline?.master.id ?? null;
      const isCurrent = (candidate: SupportCandidate) => activeMasterId !== null && candidate.masterScriptId === activeMasterId;
      candidates = allCandidates.filter((candidate) => candidate.status === "pending_review" && isCurrent(candidate) && supportCandidateCanUpgradeMaster(candidate));
      approved = allCandidates.filter((candidate) => candidate.status === "approved" && isCurrent(candidate) && supportCandidateCanUpgradeMaster(candidate));
      localMaterials = allCandidates.filter((candidate) => candidate.status === "held");
      staleCandidates = allCandidates.filter((candidate) => ["pending_review", "approved"].includes(candidate.status) && !isCurrent(candidate));
      reviewRequiredCandidates = allCandidates.filter((candidate) =>
        ["pending_review", "approved"].includes(candidate.status)
        && isCurrent(candidate)
        && !supportCandidateCanUpgradeMaster(candidate)
      );
      mergedCandidates = allCandidates.filter((candidate) => candidate.status === "merged");
    }
    catch (reason: any) { error = explainError(reason); }
    finally { loading = false; }
  }
  async function decide(candidate: SupportCandidate, label: CandidateDecisionLabel): Promise<void> {
    runningId = candidate.id; error = "";
    try {
      const updated = await decideSupportCandidate(candidate.id, candidateDecisionStatus(label));
      candidates = candidates.filter((item) => item.id !== candidate.id);
      if (updated.status === "approved") approved = [...approved, updated];
      if (updated.status === "held") localMaterials = [...localMaterials, updated];
      if (updated.status === "approved") {
        trainingExportMessage = "该话术已通过人工审核，可导出母稿培养包用于培训讨论。";
      }
    }
    catch (reason: any) { error = explainError(reason); }
    finally { runningId = 0; }
  }
  function activeScriptKey(): string {
    try { return JSON.parse(localStorage.getItem("bsr:active-master") || "{}").scriptKey || ""; } catch { return ""; }
  }
  async function previewUpgrade(): Promise<void> {
    error = "";
    try { upgrade = await previewMasterUpgrade(activeScriptKey(), approved.map((item) => item.id)); diffConfirmed = false; }
    catch (reason: any) { error = explainError(reason); }
  }
  async function publishUpgrade(): Promise<void> {
    if (!upgradePublishGate(diffConfirmed, approved.length).allowed || !upgrade) return;
    publishing = true; error = "";
    try {
      const master = await publishMasterUpgrade(upgrade.scriptKey, approved.map((item) => item.id), diffConfirmed);
      localStorage.setItem("bsr:active-master", JSON.stringify({ scriptKey: master.scriptKey, masterScriptId: master.id }));
      upgrade = null;
      await load();
    } catch (reason: any) { error = explainError(reason); }
    finally { publishing = false; }
  }
  async function exportTrainingGuide(format: "rtf" | "html"): Promise<void> {
    if (!activeMaster) return;
    trainingExporting = true;
    trainingExportMessage = "";
    try {
      const generatedOn = new Date().toLocaleDateString("zh-CN").replaceAll("/", "-");
      const guide = buildTrainingGuide(activeMaster, generatedOn);
      const fileName = trainingGuideFileName(guide, format);
      const content = format === "rtf" ? guide.rtf : guide.html;
      if (TAURI_ENV) {
        const path = await save({
          title: format === "rtf" ? "导出 Word 培训版" : "导出可打印培训版",
          defaultPath: fileName,
          filters: [{ name: format === "rtf" ? "Word 兼容文档" : "网页文件", extensions: [format] }],
        });
        if (!path) return;
        await invoke("export_to_file", { fileName: path, content });
        trainingExportMessage = `已导出到 ${path}`;
      } else {
        const blob = new Blob([content], { type: format === "rtf" ? "application/rtf;charset=utf-8" : "text/html;charset=utf-8" });
        const url = URL.createObjectURL(blob);
        const anchor = document.createElement("a");
        anchor.href = url;
        anchor.download = fileName;
        anchor.click();
        URL.revokeObjectURL(url);
        trainingExportMessage = "培训资料已开始下载";
      }
    } catch (reason: any) {
      trainingExportMessage = explainError(reason);
    } finally {
      trainingExporting = false;
    }
  }
  async function exportTrainingPack(): Promise<void> {
    if (!activeMaster) return;
    trainingExporting = true;
    trainingExportMessage = "";
    try {
      const pack = buildTrainingPack({
        baseline: activeMaster,
        candidates: [...approved, ...mergedCandidates],
        generatedAt: new Date().toLocaleString("zh-CN"),
      });
      const content = formatTrainingPackMarkdown(pack);
      const fileName = trainingPackFileName(pack);
      if (TAURI_ENV) {
        const path = await save({
          title: "导出母稿培养包",
          defaultPath: fileName,
          filters: [{ name: "Markdown 文档", extensions: ["md"] }],
        });
        if (!path) return;
        await invoke("export_to_file", { fileName: path, content });
        trainingExportMessage = `母稿培养包已导出到 ${path}`;
      } else {
        const blob = new Blob([content], { type: "text/markdown;charset=utf-8" });
        const url = URL.createObjectURL(blob);
        const anchor = document.createElement("a");
        anchor.href = url;
        anchor.download = fileName;
        anchor.click();
        URL.revokeObjectURL(url);
        trainingExportMessage = "母稿培养包已开始下载";
      }
      localStorage.setItem(
        "bsr:training-pack-exports",
        String(Number(localStorage.getItem("bsr:training-pack-exports") || 0) + 1),
      );
    } catch (reason: any) {
      trainingExportMessage = explainError(reason);
    } finally {
      trainingExporting = false;
    }
  }
  onMount(load);
</script>

<div class="backdrop"><section class="queue" role="dialog" aria-modal="true" aria-labelledby="queue-title">
  <header><div><h2 id="queue-title">候选话术审核</h2><p>达到入围条件的话术已与企业母稿比较，仍需人工决定是否用于下一版。</p></div><button class="icon" on:click={() => dispatch("close")} aria-label="关闭"><X size={20}/></button></header>
  <div class="body">
    {#if loading}<div class="empty"><span class="spin"><RotateCcw size={22}/></span>正在读取候选辅稿</div>
    {:else if error}<div class="error">{error}<button on:click={load}>重新读取</button></div>
    {:else}
    <section class="baseline" class:missing={!activeMaster}>
      <small>当前评分基准</small>
      <strong>{activeMaster ? `企业标准话术 · ${activeMaster.master.scriptKey} · V${activeMaster.master.version}` : "尚未设置企业标准话术"}</strong>
      <p>{activeMaster ? "后续录播分析会按这一版企业标准话术评分。" : "请先发布并启用一版企业标准话术，再审核候选话术。"}</p>
      {#if activeMaster}<div class="baseline-actions"><button disabled={trainingExporting} on:click={() => exportTrainingGuide("rtf")}><FileText size={14}/>导出 Word 培训版</button><button disabled={trainingExporting} on:click={() => exportTrainingGuide("html")}><Download size={14}/>导出打印版（可另存 PDF）</button><button disabled={trainingExporting} on:click={exportTrainingPack}><Download size={14}/>导出母稿培养包</button></div>{/if}
      {#if trainingExportMessage}<p class="export-message">{trainingExportMessage}</p>{/if}
    </section>
    {#if !candidates.length && !approved.length && !localMaterials.length && !staleCandidates.length && !reviewRequiredCandidates.length && !mergedCandidates.length}<div class="empty"><Check size={24}/>没有待审核、待合并或已保留的话术</div>{/if}
    {#each queueGroups as group}
      {@const groupedCandidates = candidates.filter((candidate) => supportCandidateUpgradeReview(candidate)?.comparisonDecision === group.decision)}
      {#if groupedCandidates.length}
        <section class="decision-group">
          <h3>{group.title} <span>{groupedCandidates.length}</span></h3>
          {#each groupedCandidates as candidate}
            {@const presentation = supportCandidatePresentation(candidate)}
            {@const upgradeReview = supportCandidateUpgradeReview(candidate)}
            <article>
              <div class="meta"><strong>可复用分 {candidate.totalScore}</strong><span class="decision-badge">{upgradeDecisionLabel(group.decision)}</span><span>{candidate.sourceKey}</span><span><Clock3 size={13}/>{time(candidate.sourceStartMs)} - {time(candidate.sourceEndMs)}</span></div>
              <div class="candidate-guide">
                <section><small>主播原话</small><blockquote>{candidate.hostText}</blockquote></section>
                <section><small>比企业母稿新增了什么</small><p>{upgradeReview?.newValue}</p></section>
                <section><small>为什么值得审核</small><ul>{#each upgradeReview?.whyBetter || presentation.reasons as reason}<li>{reason}</li>{/each}</ul></section>
                {#if upgradeReview?.duplicateContent.length}<section><small>与母稿重复的部分</small><ul>{#each upgradeReview.duplicateContent as item}<li>{item}</li>{/each}</ul></section>{/if}
                <section class="suggestion"><small>建议稿，不是主播原话</small><p>{upgradeReview?.trainingSuggestion}</p></section>
                <section><small>建议怎么处理</small><p>{upgradeReview?.recommendedAction || presentation.insertion}</p></section>
                <section class="checks"><small>使用前请确认</small><ul>{#each upgradeReview?.riskOrUncertainty || presentation.checks as check}<li>{check}</li>{/each}</ul></section>
              </div>
              <div class="actions">{#each actions as action}<button disabled={runningId === candidate.id} class:primary={action === "加入下一版企业话术"} on:click={() => decide(candidate, action)}>{action}</button>{/each}</div>
            </article>
          {/each}
        </section>
      {/if}
    {/each}
    {#if reviewRequiredCandidates.length}
      <section class="stale"><h3>需要按新规则复核 <span>{reviewRequiredCandidates.length}</span></h3><p>这些记录没有可执行的母稿比较结论，不能直接加入新版企业母稿。请回到对应录播重新分析。</p>{#each reviewRequiredCandidates as candidate}<article><small>暂不可用于母稿升级</small><blockquote>{candidate.hostText}</blockquote></article>{/each}</section>
    {/if}
    {#if approved.length}
      <section class="approved"><h3>已确认加入下一版企业话术 <span>{approved.length}</span></h3>
        {#if !upgrade}<button class="upgrade-button" on:click={previewUpgrade}>查看下一版企业话术草稿</button>
        {:else}
          {#each upgrade.sections as section}
            <MasterVersionDiff sectionKey={section.sectionKey} currentVersion={upgrade.currentVersion} nextVersion={upgrade.nextVersion} currentText={section.currentText} candidateText={section.nextText} showConfirmation={false}/>
          {/each}
          <label class="confirm"><input type="checkbox" bind:checked={diffConfirmed}/>我已逐条核对内容差异、主播原话和待确认事实</label>
          <button class="upgrade-button primary" disabled={!upgradePublishGate(diffConfirmed, approved.length).allowed || publishing} on:click={publishUpgrade}>发布母稿 V{upgrade.nextVersion}</button>
        {/if}
      </section>
    {/if}
    {#if localMaterials.length}
      <section class="held"><h3>已保留为局部素材 <span>{localMaterials.length}</span></h3><p>这些片段可用于新人训练，不会进入企业母稿。</p>{#each localMaterials as candidate}<blockquote>{candidate.hostText}</blockquote>{/each}</section>
    {/if}
    {#if staleCandidates.length}
      <section class="stale"><h3>需要按新版复核 <span>{staleCandidates.length}</span></h3><p>这些话术按旧版企业标准评分，不能直接加入当前企业话术。请回到对应录播，重新进行片段分析。</p>{#each staleCandidates as candidate}{@const versionStatus = supportCandidateVersionPresentation(candidate.masterScriptId, activeMaster?.master.id ?? 0)}<article><small>{versionStatus.label}</small><blockquote>{candidate.hostText}</blockquote><p>{versionStatus.detail}</p></article>{/each}</section>
    {/if}
    {#if mergedCandidates.length}
      <section class="history"><h3>已纳入企业话术记录 <span>{mergedCandidates.length}</span></h3><p>以下内容已在此前版本中纳入企业标准，可追溯到原录播来源。</p>{#each mergedCandidates as candidate}<details><summary>{candidate.sourceKey} · {time(candidate.sourceStartMs)} - {time(candidate.sourceEndMs)}</summary><blockquote>{candidate.hostText}</blockquote></details>{/each}</section>
    {/if}
    {/if}
  </div>
</section></div>

<style>
  .backdrop{position:fixed;inset:0;z-index:75;display:grid;place-items:center;padding:24px;background:rgba(15,23,42,.28);backdrop-filter:blur(12px)}.queue{display:flex;width:min(900px,96vw);max-height:88vh;flex-direction:column;overflow:hidden;border-radius:12px;background:#f7f7f9;box-shadow:0 24px 70px rgba(15,23,42,.22)}header{display:flex;align-items:center;padding:17px 20px;border-bottom:1px solid #e5e7eb;background:#fff}h2,p{margin:0}header p{margin-top:4px;color:#6b7280;font-size:12px}.icon{margin-left:auto;border:0;background:none;cursor:pointer}.body{display:grid;gap:10px;overflow:auto;padding:16px}article,.approved,.held,.stale,.history,.baseline,.decision-group{padding:14px;border:1px solid #e5e7eb;border-radius:8px;background:#fff}.decision-group{display:grid;gap:9px;background:#f8fafc}.decision-group>h3{margin:0;font-size:14px}.decision-group>h3 span{color:#1677ff}.decision-group>article{background:#fff}.baseline{border-color:#b8d9ff;background:#f4f9ff}.baseline.missing{border-color:#f3d7a4;background:#fffaf0}.baseline small{display:block;color:#667085;font-size:11px}.baseline strong{display:block;margin-top:4px;color:#1d4ed8;font-size:15px}.baseline.missing strong{color:#8a4b00}.baseline p{margin-top:4px;color:#475467;font-size:12px}.baseline-actions{display:flex;flex-wrap:wrap;gap:7px;margin-top:10px}.baseline-actions button{display:inline-flex;align-items:center;gap:5px;height:32px;padding:0 10px;border:1px solid #9ec5fe;border-radius:7px;background:#fff;color:#175cd3;cursor:pointer;font-size:12px}.baseline-actions button:disabled{cursor:wait;opacity:.55}.export-message{overflow-wrap:anywhere}.approved h3,.held h3,.stale h3,.history h3{margin:0 0 6px;font-size:14px}.approved h3 span{color:#007aff}.held{border-color:#e7d9ba;background:#fffdf7}.held h3 span{color:#a65d00}.held p{color:#76664b;font-size:12px}.held blockquote{margin-bottom:0;border-left-color:#f79009}.stale{border-color:#f1c6c2;background:#fff8f7}.stale h3 span{color:#c2410c}.stale>p,.stale article p{color:#9a3412;font-size:12px}.stale article{margin-top:8px;padding:10px;border-color:#f5d2cf}.stale article small{color:#c2410c}.history{background:#fcfcfd}.history h3 span{color:#667085}.history>p{color:#667085;font-size:12px}.history details{margin-top:8px}.history summary{cursor:pointer;color:#475467;font-size:12px}.candidate-guide{display:grid;gap:9px;margin:12px 0}.candidate-guide section{padding:9px 10px;border-radius:7px;background:#f8fafc}.candidate-guide section:first-child{padding:0;background:transparent}.candidate-guide small{display:block;color:#667085;font-size:11px}.candidate-guide p{margin-top:4px;color:#344054;font-size:13px;line-height:1.55}.candidate-guide ul{margin:4px 0 0;padding-left:18px;color:#344054;font-size:13px;line-height:1.55}.candidate-guide .checks{border-left:3px solid #f79009;background:#fffaf0}.candidate-guide .suggestion{border-left:3px solid #8b5cf6;background:#faf7ff}.confirm{display:flex;gap:8px;margin-top:12px;font-size:13px}.upgrade-button{height:36px;padding:0 13px;border:1px solid #007aff;border-radius:7px;background:#fff;color:#007aff;cursor:pointer}.upgrade-button.primary{margin-top:12px;background:#007aff;color:#fff}.meta{display:flex;align-items:center;gap:12px;color:#6b7280;font-size:12px}.meta strong{color:#007aff;font-size:16px}.meta span{display:flex;align-items:center;gap:4px}.meta .decision-badge{padding:3px 6px;border-radius:5px;background:#edf5ff;color:#175cd3}blockquote{margin:7px 0 0;padding-left:11px;border-left:3px solid #dbeafe;white-space:pre-wrap;color:#374151;line-height:1.6}.actions{display:flex;flex-wrap:wrap;gap:7px}.actions button,.error button{height:32px;padding:0 10px;border:1px solid #d1d5db;border-radius:7px;background:#fff;cursor:pointer}.actions .primary{border-color:#007aff;background:#007aff;color:#fff}.empty,.error{display:flex;min-height:160px;align-items:center;justify-content:center;gap:8px;color:#6b7280}.spin{animation:spin 1s linear infinite}@keyframes spin{to{transform:rotate(360deg)}}
</style>
