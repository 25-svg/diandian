<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import { Check, FileText, X } from "lucide-svelte";
  import {
    decideCompetitorReferenceCandidate,
    listCompetitorReferenceCandidates,
    publishCompetitorReference,
    type CompetitorReferenceCandidate,
  } from "../../masterScript";

  const dispatch = createEventDispatcher();
  let candidates: CompetitorReferenceCandidate[] = [];
  let loading = true;
  let error = "";
  let runningId = 0;
  const time = (ms: number) => `${Math.floor(ms / 60000)}:${String(Math.floor(ms / 1000) % 60).padStart(2, "0")}`;

  async function load(): Promise<void> {
    loading = true; error = "";
    try { candidates = await listCompetitorReferenceCandidates(); }
    catch (reason: any) { error = reason?.message || String(reason); }
    finally { loading = false; }
  }
  async function decide(candidate: CompetitorReferenceCandidate, status: "approved_reference" | "rejected"): Promise<void> {
    runningId = candidate.id; error = "";
    try { await decideCompetitorReferenceCandidate(candidate.id, status); await load(); }
    catch (reason: any) { error = reason?.message || String(reason); }
    finally { runningId = 0; }
  }
  async function publish(candidate: CompetitorReferenceCandidate): Promise<void> {
    runningId = candidate.id; error = "";
    try { await publishCompetitorReference(candidate.id); await load(); }
    catch (reason: any) { error = reason?.message || String(reason); }
    finally { runningId = 0; }
  }
  onMount(load);
</script>

<div class="backdrop"><section class="queue" role="dialog" aria-modal="true" aria-labelledby="competitor-queue-title">
  <header><div><h2 id="competitor-queue-title">竞品参考队列</h2><p>只保存对照结论。竞品原话不能直接升级企业母稿；批准后才可写入 03-直播案例。</p></div><button on:click={() => dispatch("close")} aria-label="关闭"><X size={20}/></button></header>
  <main>
    {#if loading}<p>正在读取竞品参考…</p>
    {:else if error}<p class="error">{error}</p>
    {:else if !candidates.length}<p class="empty"><Check size={20}/>暂无竞品参考。</p>
    {:else}{#each candidates as candidate}
      <article>
        <div class="meta"><strong>{candidate.competitorName || "未命名竞品"}</strong><span>{candidate.totalScore} 分</span><span>{candidate.migrationDecision}</span><span>{candidate.status}</span><span>{time(candidate.sourceStartMs)}–{time(candidate.sourceEndMs)}</span></div>
        <blockquote>{candidate.hostText}</blockquote>
        <small>来源：{candidate.sourceKey}。动态价格、库存、链接、型号、售后不能迁移为企业固定话术。</small>
        <div class="actions">
          {#if candidate.status === "pending"}<button disabled={runningId === candidate.id} on:click={() => decide(candidate, "approved_reference")}>人工批准参考</button><button disabled={runningId === candidate.id} on:click={() => decide(candidate, "rejected")}>拒绝</button>{/if}
          {#if candidate.status === "approved_reference"}<button class="primary" disabled={runningId === candidate.id} on:click={() => publish(candidate)}><FileText size={14}/>写入案例库</button>{/if}
        </div>
      </article>
    {/each}{/if}
  </main>
</section></div>

<style>
  .backdrop{position:fixed;inset:0;z-index:76;display:grid;place-items:center;padding:24px;background:rgba(15,23,42,.28);backdrop-filter:blur(12px)}.queue{width:min(850px,96vw);max-height:88vh;overflow:auto;border-radius:12px;background:#f8fafc;box-shadow:0 24px 70px rgba(15,23,42,.22)}header{display:flex;padding:17px 20px;border-bottom:1px solid #e5e7eb;background:#fff}h2,p{margin:0}header p{margin-top:4px;color:#667085;font-size:12px}header button{margin-left:auto;border:0;background:none;cursor:pointer}main{display:grid;gap:10px;padding:16px}article{padding:14px;border:1px solid #e5e7eb;border-radius:8px;background:#fff}.meta,.actions{display:flex;flex-wrap:wrap;align-items:center;gap:8px;font-size:12px;color:#667085}.meta strong{color:#1d4ed8;font-size:14px}.meta span{padding:2px 6px;border-radius:5px;background:#f2f4f7}blockquote{margin:10px 0;padding-left:11px;border-left:3px solid #c7d7fe;white-space:pre-wrap;line-height:1.6;color:#344054}article small{color:#667085}.actions{margin-top:11px}.actions button{display:inline-flex;align-items:center;gap:5px;height:32px;padding:0 10px;border:1px solid #d0d5dd;border-radius:7px;background:#fff;cursor:pointer}.actions .primary{border-color:#175cd3;background:#175cd3;color:#fff}.error{color:#b42318}.empty{display:flex;align-items:center;gap:8px;min-height:160px;justify-content:center;color:#667085}
</style>
