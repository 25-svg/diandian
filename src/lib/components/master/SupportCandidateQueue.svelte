<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import { Check, Clock3, RotateCcw, X } from "lucide-svelte";
  import {
    candidateDecisionStatus,
    decideSupportCandidate,
    listSupportCandidates,
    previewMasterUpgrade,
    publishMasterUpgrade,
    upgradePublishGate,
    type MasterUpgradePreview,
    type CandidateDecisionLabel,
    type SupportCandidate,
  } from "../../masterScript";
  import MasterVersionDiff from "./MasterVersionDiff.svelte";
  const dispatch = createEventDispatcher();
  let candidates: SupportCandidate[] = [];
  let loading = true;
  let error = "";
  let runningId = 0;
  let approved: SupportCandidate[] = [];
  let upgrade: MasterUpgradePreview | null = null;
  let diffConfirmed = false;
  let publishing = false;
  const actions: CandidateDecisionLabel[] = ["通过并加入新版本", "保留候选", "退回修改", "不采用"];
  const time = (ms: number) => { const s = Math.floor(ms / 1000); return `${Math.floor(s / 60)}:${String(s % 60).padStart(2,"0")}`; };
  async function load(): Promise<void> {
    loading = true; error = "";
    try { [candidates, approved] = await Promise.all([listSupportCandidates("pending_review"), listSupportCandidates("approved")]); }
    catch (reason: any) { error = reason?.message || String(reason); }
    finally { loading = false; }
  }
  async function decide(candidate: SupportCandidate, label: CandidateDecisionLabel): Promise<void> {
    runningId = candidate.id; error = "";
    try { const updated = await decideSupportCandidate(candidate.id, candidateDecisionStatus(label)); candidates = candidates.filter((item) => item.id !== candidate.id); if (updated.status === "approved") approved = [...approved, updated]; }
    catch (reason: any) { error = reason?.message || String(reason); }
    finally { runningId = 0; }
  }
  function activeScriptKey(): string {
    try { return JSON.parse(localStorage.getItem("bsr:active-master") || "{}").scriptKey || ""; } catch { return ""; }
  }
  async function previewUpgrade(): Promise<void> {
    error = "";
    try { upgrade = await previewMasterUpgrade(activeScriptKey(), approved.map((item) => item.id)); diffConfirmed = false; }
    catch (reason: any) { error = reason?.message || String(reason); }
  }
  async function publishUpgrade(): Promise<void> {
    if (!upgradePublishGate(diffConfirmed, approved.length).allowed || !upgrade) return;
    publishing = true; error = "";
    try {
      const master = await publishMasterUpgrade(upgrade.scriptKey, approved.map((item) => item.id), diffConfirmed);
      localStorage.setItem("bsr:active-master", JSON.stringify({ scriptKey: master.scriptKey, masterScriptId: master.id }));
      upgrade = null; approved = [];
    } catch (reason: any) { error = reason?.message || String(reason); }
    finally { publishing = false; }
  }
  onMount(load);
</script>

<div class="backdrop"><section class="queue" role="dialog" aria-modal="true" aria-labelledby="queue-title">
  <header><div><h2 id="queue-title">候选辅稿审核</h2><p>85 分及以上只进入这里，仍需人工决定是否加入母稿新版本。</p></div><button class="icon" on:click={() => dispatch("close")} aria-label="关闭"><X size={20}/></button></header>
  <div class="body">
    {#if loading}<div class="empty"><RotateCcw class="spin" size={22}/>正在读取候选辅稿</div>
    {:else if error}<div class="error">{error}<button on:click={load}>重新读取</button></div>
    {:else if !candidates.length && !approved.length}<div class="empty"><Check size={24}/>没有待审核或已通过的候选辅稿</div>
    {:else}{#each candidates as candidate}
      <article>
        <div class="meta"><strong>{candidate.totalScore} 分</strong><span>{candidate.sourceKey}</span><span><Clock3 size={13}/>{time(candidate.sourceStartMs)} - {time(candidate.sourceEndMs)}</span></div>
        <blockquote>{candidate.hostText}</blockquote>
        <div class="actions">{#each actions as action}<button disabled={runningId === candidate.id} class:primary={action === "通过并加入新版本"} on:click={() => decide(candidate, action)}>{action}</button>{/each}</div>
      </article>
    {/each}
    {#if approved.length}
      <section class="approved"><h3>已通过，等待加入新版本 <span>{approved.length}</span></h3>
        {#if !upgrade}<button class="upgrade-button" on:click={previewUpgrade}>预览母稿新版本</button>
        {:else}
          {#each upgrade.sections as section}
            <MasterVersionDiff currentVersion={upgrade.currentVersion} nextVersion={upgrade.nextVersion} currentText={section.currentText} candidateText={section.nextText.slice(section.currentText.length).trim()} showConfirmation={false}/>
          {/each}
          <label class="confirm"><input type="checkbox" bind:checked={diffConfirmed}/>我已逐条核对上述差异、主播原话和待确认事实</label>
          <button class="upgrade-button primary" disabled={!upgradePublishGate(diffConfirmed, approved.length).allowed || publishing} on:click={publishUpgrade}>发布母稿 V{upgrade.nextVersion}</button>
        {/if}
      </section>
    {/if}{/if}
  </div>
</section></div>

<style>
  .backdrop{position:fixed;inset:0;z-index:75;display:grid;place-items:center;padding:24px;background:rgba(15,23,42,.28);backdrop-filter:blur(12px)}.queue{display:flex;width:min(900px,96vw);max-height:88vh;flex-direction:column;overflow:hidden;border-radius:12px;background:#f7f7f9;box-shadow:0 24px 70px rgba(15,23,42,.22)}header{display:flex;align-items:center;padding:17px 20px;border-bottom:1px solid #e5e7eb;background:#fff}h2,p{margin:0}header p{margin-top:4px;color:#6b7280;font-size:12px}.icon{margin-left:auto;border:0;background:none;cursor:pointer}.body{display:grid;gap:10px;overflow:auto;padding:16px}article,.approved{padding:14px;border:1px solid #e5e7eb;border-radius:8px;background:#fff}.approved h3{margin:0 0 10px;font-size:14px}.approved h3 span{color:#007aff}.confirm{display:flex;gap:8px;margin-top:12px;font-size:13px}.upgrade-button{height:36px;padding:0 13px;border:1px solid #007aff;border-radius:7px;background:#fff;color:#007aff;cursor:pointer}.upgrade-button.primary{margin-top:12px;background:#007aff;color:#fff}.meta{display:flex;align-items:center;gap:12px;color:#6b7280;font-size:12px}.meta strong{color:#007aff;font-size:16px}.meta span{display:flex;align-items:center;gap:4px}blockquote{margin:12px 0;padding-left:11px;border-left:3px solid #dbeafe;white-space:pre-wrap;color:#374151;line-height:1.6}.actions{display:flex;flex-wrap:wrap;gap:7px}.actions button,.error button{height:32px;padding:0 10px;border:1px solid #d1d5db;border-radius:7px;background:#fff;cursor:pointer}.actions .primary{border-color:#007aff;background:#007aff;color:#fff}.empty,.error{display:flex;min-height:160px;align-items:center;justify-content:center;gap:8px;color:#6b7280}.spin{animation:spin 1s linear infinite}@keyframes spin{to{transform:rotate(360deg)}}
</style>
