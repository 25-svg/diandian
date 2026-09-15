<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import {
    AlertTriangle,
    ArrowRight,
    BadgeCheck,
    BarChart3,
    CheckCircle2,
    ChevronRight,
    ClipboardCheck,
    Clock3,
    FileCheck2,
    Search,
    ShieldCheck,
    TrendingUp,
  } from "lucide-svelte";
  import {
    OPERATIONS_SECTIONS,
    filterOperationsAnchors,
    operationsDemoAnchors,
    operationsDemoAssets,
    operationsDemoTasks,
    operationsSummary,
    type OperationsAnchorRow,
    type OperationsSection,
  } from "../lib/operationsDashboard.js";

  export let section: OperationsSection = "运营总览";

  const dispatch = createEventDispatcher<{ navigate: OperationsSection }>();
  const summary = operationsSummary(operationsDemoAnchors, operationsDemoTasks);
  const teams = ["全部团队", ...Array.from(new Set(operationsDemoAnchors.map((item) => item.team)))];
  let selectedTeam = "全部团队";
  let query = "";
  let selectedAnchor: OperationsAnchorRow | null = null;
  let liveMessage = "";
  let assetActionMessage = "";

  $: visibleAnchors = filterOperationsAnchors(operationsDemoAnchors, selectedTeam, query);

  function navigate(next: OperationsSection): void {
    section = next;
    selectedAnchor = null;
    dispatch("navigate", next);
  }

  function inspectAnchor(anchor: OperationsAnchorRow): void {
    selectedAnchor = anchor;
    liveMessage = `已打开${anchor.name}的管理摘要。`;
  }

  function recordAssetAction(title: string, action: "退回" | "审核发布"): void {
    assetActionMessage = `原型操作：已选择“${action}”《${title}》，正式版会要求确认并写入审核日志。`;
    liveMessage = assetActionMessage;
  }
</script>

<div class="operations-page">
  <div class="prototype-notice" role="status">
    <ShieldCheck size={17} aria-hidden="true" />
    <span><strong>运营负责人端交互原型</strong> · 当前为示例数据；正式版按账号权限进入，不能自行切换角色。</span>
  </div>

  <header class="page-header">
    <div>
      <p class="eyebrow">典典 · 运营负责人端</p>
      <h1>{section}</h1>
      <p>从团队结果定位到主播、场次和视频证据，再形成可复核的改进任务。</p>
    </div>
    <div class="date-filter" aria-label="统计范围">
      <Clock3 size={16} aria-hidden="true" />
      最近 7 天 · 截至今天 12:00
    </div>
  </header>

  <div class="section-tabs" aria-label="运营负责人功能" role="tablist">
    {#each OPERATIONS_SECTIONS as item}
      <button type="button" role="tab" aria-selected={section === item} on:click={() => navigate(item)}>{item}</button>
    {/each}
  </div>

  {#if section === "运营总览"}
    <section class="kpi-grid" aria-label="核心经营指标">
      <article><span>团队成交额</span><strong>¥{summary.gmvYuan.toLocaleString("zh-CN")}</strong><small class="up"><TrendingUp size={14} /> 较前 7 天 +8.6%</small></article>
      <article><span>有效直播场次</span><strong>{summary.liveCount}</strong><small>4 位主播 · 2 个团队</small></article>
      <article><span>平均成交转化</span><strong>{summary.averageConversionRate.toFixed(1)}%</strong><small class="up"><TrendingUp size={14} /> 较前 7 天 +0.3%</small></article>
      <article class="attention"><span>需要负责人关注</span><strong>{summary.attentionAnchorCount} 人</strong><small><AlertTriangle size={14} /> {summary.openTaskCount} 项任务未闭环</small></article>
    </section>

    <div class="overview-grid">
      <section class="panel attention-panel" aria-labelledby="attention-title">
        <div class="panel-head"><div><p class="eyebrow">先处理这些</p><h2 id="attention-title">今日异常与行动</h2></div><button type="button" on:click={() => navigate("改进任务")}>全部任务 <ArrowRight size={15} /></button></div>
        <div class="attention-list">
          {#each operationsDemoTasks.filter((item) => item.status !== "已完成").slice(0, 3) as task}
            <article>
              <span class:urgent={task.status === "待领取"} class="status-dot" aria-hidden="true"></span>
              <div><strong>{task.anchorName} · {task.issue}</strong><p>{task.evidence} · {task.action}</p><small>{task.status} · {task.dueDate}</small></div>
              <button type="button" aria-label={`查看${task.anchorName}的任务`} on:click={() => navigate("改进任务")}><ChevronRight size={18} /></button>
            </article>
          {/each}
        </div>
      </section>

      <section class="panel ranking-panel" aria-labelledby="ranking-title">
        <div class="panel-head"><div><p class="eyebrow">团队横向比较</p><h2 id="ranking-title">主播表现</h2></div><button type="button" on:click={() => navigate("主播团队")}>查看全部 <ArrowRight size={15} /></button></div>
        <div class="ranking-list">
          {#each operationsDemoAnchors as anchor, index}
            <button type="button" on:click={() => { navigate("主播团队"); inspectAnchor(anchor); }}>
              <span class="rank">{index + 1}</span><span class="anchor-copy"><strong>{anchor.name}</strong><small>{anchor.team} · {anchor.focus}</small></span>
              <span class="score">{anchor.score}<small class:down={anchor.scoreDelta < 0}>{anchor.scoreDelta > 0 ? "+" : ""}{anchor.scoreDelta}</small></span>
            </button>
          {/each}
        </div>
      </section>
    </div>
  {:else if section === "主播团队"}
    <section class="panel team-panel" aria-labelledby="team-title">
      <div class="panel-head team-head"><div><p class="eyebrow">团队 → 主播 → 场次</p><h2 id="team-title">主播团队表现</h2></div><div class="filters"><label>团队<select bind:value={selectedTeam}>{#each teams as team}<option>{team}</option>{/each}</select></label><label class="search"><Search size={15} /><span class="sr-only">搜索主播</span><input bind:value={query} placeholder="搜索主播或问题" /></label></div></div>
      <div class="table-wrap" role="region" aria-label="主播表现表格">
        <table>
          <thead><tr><th>主播</th><th>场次</th><th>成交额</th><th>转化率</th><th>平均在线</th><th>综合分</th><th>风险</th><th>本周重点</th><th></th></tr></thead>
          <tbody>
            {#each visibleAnchors as anchor}
              <tr class:selected={selectedAnchor?.id === anchor.id}>
                <td><strong>{anchor.name}</strong><small>{anchor.team}</small></td><td>{anchor.liveCount}</td><td>¥{anchor.gmvYuan.toLocaleString("zh-CN")}</td><td>{anchor.conversionRate.toFixed(1)}%</td><td>{anchor.averageOnline}</td><td><strong>{anchor.score}</strong> <span class:down={anchor.scoreDelta < 0} class="delta">{anchor.scoreDelta > 0 ? "+" : ""}{anchor.scoreDelta}</span></td><td><span class:has-risk={anchor.riskCount > 0} class="risk-pill">{anchor.riskCount > 0 ? `${anchor.riskCount} 项` : "正常"}</span></td><td>{anchor.focus}</td><td><button type="button" class="inspect-button" on:click={() => inspectAnchor(anchor)}>查看</button></td>
              </tr>
            {:else}
              <tr><td colspan="9" class="empty">没有符合条件的主播。</td></tr>
            {/each}
          </tbody>
        </table>
      </div>
    </section>
    {#if selectedAnchor}
      <section class="anchor-detail" aria-labelledby="anchor-detail-title">
        <div><p class="eyebrow">主播管理摘要</p><h2 id="anchor-detail-title">{selectedAnchor.name}</h2><p>{selectedAnchor.team} · 本周重点：{selectedAnchor.focus}</p></div>
        <div class="detail-metrics"><span><small>综合分</small><strong>{selectedAnchor.score}</strong></span><span><small>转化率</small><strong>{selectedAnchor.conversionRate}%</strong></span><span><small>风险</small><strong>{selectedAnchor.riskCount}</strong></span></div>
        <button type="button" on:click={() => navigate("改进任务")}>查看相关任务 <ArrowRight size={15} /></button>
      </section>
    {/if}
  {:else if section === "改进任务"}
    <section class="panel" aria-labelledby="task-title">
      <div class="panel-head"><div><p class="eyebrow">问题 → 行动 → 复核</p><h2 id="task-title">改进任务闭环</h2></div><span class="count-pill">{summary.openTaskCount} 项未完成</span></div>
      <div class="task-board">
        {#each ["待领取", "进行中", "待复核", "已完成"] as status}
          <section aria-label={status}><header><strong>{status}</strong><span>{operationsDemoTasks.filter((item) => item.status === status).length}</span></header>
            {#each operationsDemoTasks.filter((item) => item.status === status) as task}
              <article><span>{task.anchorName}</span><h3>{task.issue}</h3><p>{task.action}</p><small>{task.evidence}</small><footer><Clock3 size={13} /> {task.dueDate}</footer></article>
            {/each}
          </section>
        {/each}
      </div>
    </section>
  {:else}
    <section class="panel" aria-labelledby="asset-title">
      <div class="panel-head"><div><p class="eyebrow">人工审核后才能发布</p><h2 id="asset-title">待审知识资产</h2></div><span class="count-pill">{operationsDemoAssets.length} 条待审核</span></div>
      <div class="asset-list">
        {#each operationsDemoAssets as asset}
          <article>
            <div class="asset-icon">{#if asset.category === "优秀切片"}<BarChart3 size={20} />{:else if asset.category === "培训案例"}<ClipboardCheck size={20} />{:else}<FileCheck2 size={20} />{/if}</div>
            <div><span>{asset.category} · {asset.anchorName}</span><h3>{asset.title}</h3><p>{asset.evidence}</p><small>提交于 {asset.submittedAt} · {asset.risk}</small></div>
            <div class="asset-actions"><button type="button" on:click={() => recordAssetAction(asset.title, "退回")}>退回</button><button type="button" class="primary" on:click={() => recordAssetAction(asset.title, "审核发布")}><BadgeCheck size={15} /> 审核发布</button></div>
          </article>
        {/each}
      </div>
      {#if assetActionMessage}<p class="asset-feedback" role="status">{assetActionMessage}</p>{/if}
      <div class="audit-rule"><CheckCircle2 size={17} /><span>发布动作将记录审核人、来源场次、证据版本和发布时间；商品事实仍以参数库为准。</span></div>
    </section>
  {/if}

  <p class="sr-only" aria-live="polite">{liveMessage}</p>
</div>

<style>
  .operations-page{height:100%;overflow:auto;padding:20px 22px 30px;box-sizing:border-box;background:linear-gradient(180deg,#f8fafc 0%,#eef2f7 100%);color:#101828}.prototype-notice{display:flex;align-items:center;gap:8px;min-height:42px;padding:0 14px;border:1px solid #b2ddff;border-radius:10px;background:#eff8ff;color:#1849a9;font-size:12px}.page-header{display:flex;align-items:flex-end;justify-content:space-between;gap:20px;margin:20px 0 16px}.page-header h1{margin:2px 0 4px;font-size:26px;letter-spacing:-.4px}.page-header p{margin:0;color:#667085;font-size:13px}.eyebrow{margin:0!important;color:#175cd3!important;font-size:11px!important;font-weight:750;text-transform:uppercase;letter-spacing:.08em}.date-filter{display:flex;align-items:center;gap:7px;padding:9px 12px;border:1px solid #d0d5dd;border-radius:9px;background:#fff;color:#475467;font-size:12px}.section-tabs{display:flex;gap:4px;margin-bottom:14px;padding:4px;border:1px solid #e4e7ec;border-radius:11px;background:#fff;width:max-content}.section-tabs button{min-height:44px;padding:0 15px;border:0;border-radius:8px;background:transparent;color:#475467;font-weight:650;cursor:pointer}.section-tabs button[aria-selected="true"]{background:#175cd3;color:#fff}.kpi-grid{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:12px}.kpi-grid article{display:flex;flex-direction:column;min-height:116px;padding:16px;border:1px solid #e4e7ec;border-radius:13px;background:#fff;box-shadow:0 1px 2px rgba(16,24,40,.04)}.kpi-grid article.attention{border-color:#fec84b;background:#fffaeb}.kpi-grid span{color:#667085;font-size:12px}.kpi-grid strong{margin:10px 0 7px;font-size:27px}.kpi-grid small{display:flex;align-items:center;gap:4px;color:#667085}.kpi-grid .up{color:#067647}.overview-grid{display:grid;grid-template-columns:minmax(0,1.3fr) minmax(320px,.7fr);gap:12px;margin-top:12px}.panel{border:1px solid #e4e7ec;border-radius:14px;background:#fff;box-shadow:0 1px 3px rgba(16,24,40,.05);overflow:hidden}.panel-head{display:flex;align-items:center;justify-content:space-between;gap:16px;padding:16px 18px;border-bottom:1px solid #eaecf0}.panel-head h2{margin:2px 0 0;font-size:17px}.panel-head>button,.anchor-detail>button{display:flex;align-items:center;gap:5px;min-height:44px;padding:0 10px;border:0;background:transparent;color:#175cd3;font-weight:650;cursor:pointer}.attention-list article{display:grid;grid-template-columns:auto minmax(0,1fr) auto;align-items:center;gap:12px;padding:14px 18px;border-bottom:1px solid #f2f4f7}.attention-list article:last-child{border-bottom:0}.attention-list strong{font-size:13px}.attention-list p{margin:4px 0;color:#475467;font-size:12px}.attention-list small{color:#b54708;font-size:11px}.attention-list button{width:44px;height:44px;border:0;background:transparent;color:#667085;cursor:pointer}.status-dot{width:9px;height:9px;border-radius:50%;background:#f79009;box-shadow:0 0 0 4px #fef0c7}.status-dot.urgent{background:#d92d20;box-shadow:0 0 0 4px #fee4e2}.ranking-list{padding:5px 0}.ranking-list>button{width:100%;min-height:62px;display:grid;grid-template-columns:30px minmax(0,1fr) auto;align-items:center;gap:9px;padding:8px 16px;border:0;background:transparent;text-align:left;cursor:pointer}.ranking-list>button:hover{background:#f9fafb}.rank{display:grid;place-items:center;width:26px;height:26px;border-radius:8px;background:#eff8ff;color:#175cd3;font-weight:750}.anchor-copy{display:flex;flex-direction:column;min-width:0}.anchor-copy small{margin-top:2px;color:#667085;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.score{font-size:18px;font-weight:750}.score small,.delta{margin-left:4px;color:#067647;font-size:11px}.score small.down,.delta.down{color:#b42318}.team-head{align-items:flex-end}.filters{display:flex;gap:8px}.filters label{display:flex;flex-direction:column;gap:4px;color:#667085;font-size:10px}.filters select,.filters input{height:36px;box-sizing:border-box;border:1px solid #d0d5dd;border-radius:8px;background:#fff;padding:0 10px;color:#344054}.search{position:relative;justify-content:flex-end}.search :global(svg){position:absolute;left:10px;bottom:10px;color:#98a2b3}.search input{padding-left:32px}.table-wrap{overflow:auto}.table-wrap table{width:100%;border-collapse:collapse;min-width:930px}.table-wrap th{padding:10px 12px;background:#f9fafb;color:#667085;font-size:11px;text-align:left;white-space:nowrap}.table-wrap td{padding:12px;border-top:1px solid #eaecf0;color:#344054;font-size:12px;white-space:nowrap}.table-wrap td:first-child{display:flex;flex-direction:column}.table-wrap td small{color:#98a2b3}.table-wrap tr.selected{background:#eff8ff}.risk-pill{display:inline-flex;padding:3px 7px;border-radius:99px;background:#ecfdf3;color:#067647}.risk-pill.has-risk{background:#fff4ed;color:#b93815}.inspect-button{min-width:52px;min-height:44px;border:1px solid #b2ddff;border-radius:8px;background:#eff8ff;color:#175cd3;cursor:pointer}.empty{text-align:center!important;color:#667085!important}.anchor-detail{display:flex;align-items:center;gap:24px;margin-top:12px;padding:16px 18px;border:1px solid #b2ddff;border-radius:13px;background:#eff8ff}.anchor-detail h2{margin:2px 0;font-size:17px}.anchor-detail p{margin:0;color:#475467;font-size:12px}.detail-metrics{display:flex;gap:8px;margin-left:auto}.detail-metrics span{min-width:76px;padding:8px 12px;border-radius:9px;background:#fff}.detail-metrics small{display:block;color:#667085}.detail-metrics strong{font-size:17px}.task-board{display:grid;grid-template-columns:repeat(4,minmax(220px,1fr));gap:10px;padding:14px;overflow:auto}.task-board>section{padding:10px;border-radius:11px;background:#f2f4f7}.task-board>section>header{display:flex;justify-content:space-between;padding:3px 3px 10px;color:#344054;font-size:12px}.task-board>section>header span,.count-pill{display:inline-flex;padding:3px 8px;border-radius:99px;background:#eef4ff;color:#3538cd;font-size:11px}.task-board article{margin-bottom:8px;padding:12px;border:1px solid #e4e7ec;border-radius:10px;background:#fff}.task-board article>span{color:#175cd3;font-size:11px;font-weight:700}.task-board h3{margin:6px 0;font-size:13px;line-height:1.45}.task-board p{margin:0 0 8px;color:#475467;font-size:12px;line-height:1.45}.task-board small{color:#667085}.task-board footer{display:flex;align-items:center;gap:4px;margin-top:10px;padding-top:8px;border-top:1px solid #f2f4f7;color:#b54708;font-size:11px}.asset-list article{display:grid;grid-template-columns:auto minmax(0,1fr) auto;align-items:center;gap:14px;padding:15px 18px;border-bottom:1px solid #eaecf0}.asset-icon{display:grid;place-items:center;width:42px;height:42px;border-radius:11px;background:#eff8ff;color:#175cd3}.asset-list span{color:#175cd3;font-size:11px;font-weight:700}.asset-list h3{margin:3px 0;font-size:14px}.asset-list p{margin:0;color:#475467;font-size:12px}.asset-list small{color:#667085}.asset-actions{display:flex;gap:7px}.asset-actions button{display:flex;align-items:center;gap:5px;min-height:44px;padding:0 12px;border:1px solid #d0d5dd;border-radius:8px;background:#fff;color:#344054;cursor:pointer}.asset-actions button.primary{border-color:#175cd3;background:#175cd3;color:#fff}.asset-feedback{margin:14px 18px 0;padding:10px 12px;border-radius:8px;background:#eff8ff;color:#1849a9;font-size:12px}.audit-rule{display:flex;align-items:center;gap:8px;margin:14px 18px;padding:12px;border-radius:9px;background:#ecfdf3;color:#067647;font-size:12px}.sr-only{position:absolute;width:1px;height:1px;padding:0;margin:-1px;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap;border:0}button:focus-visible,input:focus-visible,select:focus-visible{outline:3px solid rgba(23,92,211,.28);outline-offset:2px}
  @media(max-width:1100px){.kpi-grid{grid-template-columns:repeat(2,1fr)}.overview-grid{grid-template-columns:1fr}.task-board{grid-template-columns:repeat(2,minmax(240px,1fr))}}
  @media(max-width:760px){.operations-page{padding:12px}.page-header{align-items:flex-start;flex-direction:column}.section-tabs{width:100%;overflow:auto}.section-tabs button{min-width:max-content;min-height:44px}.kpi-grid{grid-template-columns:1fr}.filters{width:100%;flex-direction:column}.team-head{align-items:flex-start;flex-direction:column}.task-board{grid-template-columns:1fr}.anchor-detail{align-items:flex-start;flex-direction:column}.detail-metrics{margin-left:0}.asset-list article{grid-template-columns:auto 1fr}.asset-actions{grid-column:1/-1;justify-content:flex-end}}
</style>
