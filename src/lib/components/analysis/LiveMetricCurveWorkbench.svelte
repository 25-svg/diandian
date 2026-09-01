<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { AlertTriangle, Play } from "lucide-svelte";
  import {
    analyzeCompassPoint,
    analyzeCompassMetricWindow,
    compassEvidenceExplanation,
    compassMetricStyle,
    compassPointOffsetSeconds,
    formatCompassOffset,
    normalizeCompassMetricValues,
    type CompassCaptureAnalysis,
    type CompassDeclineEvent,
    type CompassMetricAnalysis,
    type CompassMetricPoint,
    type CompassMetricWindowAnalysis,
    type CompassPointInsight,
  } from "../../compassAnalysis";

  export let analysis: CompassCaptureAnalysis;
  export let sessionStartedAt = "";

  const dispatch = createEventDispatcher<{ seek: number }>();

  type CurveEventRow = {
    id: string;
    metricLabel: string;
    unit: string;
    kind: "异常区间" | "明显变化点";
    time: string;
    sortValue: number;
    value: number;
    detail: string;
    evidence: CompassDeclineEvent | null;
  };

  type CompassModule = "data" | "product" | "audience" | "qianchuan";
  type SelectedPoint = {
    key: string;
    metric: CompassMetricAnalysis;
    point: CompassMetricPoint;
    insight: CompassPointInsight;
    offsetSeconds: number | null;
    evidence: CompassDeclineEvent | null;
  };

  let initializedCaptureId = "";
  let selectedMetricLabels: string[] = [];
  let fullRangeStart = 0;
  let fullRangeEnd = 0;
  let rangeStart = 0;
  let rangeEnd = 0;
  let rangeStep = 1;
  let selectedMetrics: CompassMetricAnalysis[] = [];
  let windowAnalyses: CompassMetricWindowAnalysis[] = [];
  let normalizedComparison = false;
  let chartMinimum = 0;
  let chartMaximum = 1;
  let curveEvents: CurveEventRow[] = [];
  let activeModule: CompassModule = "data";
  let selectedPoint: SelectedPoint | null = null;

  $: if (analysis.captureId !== initializedCaptureId) {
    initializedCaptureId = analysis.captureId;
    initializeControls();
  }
  $: selectedMetrics = analysis.metrics.filter((metric) => selectedMetricLabels.includes(metric.label));
  $: windowAnalyses = selectedMetrics.map((metric) => analyzeCompassMetricWindow(metric, {
    start: rangeStart,
    end: rangeEnd,
  }));
  $: normalizedComparison = new Set(windowAnalyses.map((item) => item.dimension)).size > 1;
  $: {
    const values = normalizedComparison
      ? [0, 100]
      : selectedMetrics.flatMap((metric) => metric.points
          .filter((point) => point.sortValue >= rangeStart && point.sortValue <= rangeEnd && pointValid(point))
          .map((point) => point.value));
    chartMinimum = values.length ? Math.min(...values) : 0;
    chartMaximum = values.length ? Math.max(...values) : 1;
  }
  $: curveEvents = windowAnalyses.flatMap((metricAnalysis) => [
    ...metricAnalysis.anomalies.map((event): CurveEventRow => ({
      id: event.id,
      metricLabel: metricAnalysis.label,
      unit: metricAnalysis.unit,
      kind: "异常区间",
      time: `${event.startTime}–${event.endTime}`,
      sortValue: event.startSortValue,
      value: event.value,
      detail: `${event.direction === "high" ? "高于" : "低于"}${metricAnalysis.baselineSource === "explicit" ? "目标基线" : "统计基线"}${event.deviationPercent == null ? "" : ` ${signedPercent(event.deviationPercent)}`}`,
      evidence: evidenceFor(metricAnalysis.label, event.startSortValue, event.endSortValue),
    })),
    ...metricAnalysis.changes.map((event, index): CurveEventRow => ({
      id: `${metricAnalysis.metricId}-change-${index + 1}`,
      metricLabel: metricAnalysis.label,
      unit: metricAnalysis.unit,
      kind: "明显变化点",
      time: event.timeLabel,
      sortValue: event.sortValue,
      value: event.value,
      detail: `${formatNumber(event.previousValue, metricAnalysis.unit)} → ${formatNumber(event.value, metricAnalysis.unit)}（${signedPercent(event.changePercent)}）`,
      evidence: evidenceFor(metricAnalysis.label, event.sortValue, event.sortValue),
    })),
  ]).sort((left, right) => left.sortValue - right.sortValue);

  function initializeControls(): void {
    const points = analysis.metrics.flatMap((metric) => metric.points);
    fullRangeStart = points.length ? Math.min(...points.map((point) => point.sortValue)) : 0;
    fullRangeEnd = points.length ? Math.max(...points.map((point) => point.sortValue)) : 0;
    rangeStart = fullRangeStart;
    rangeEnd = fullRangeEnd;
    const uniqueTimes = [...new Set(points.map((point) => point.sortValue))].sort((left, right) => left - right);
    const gaps = uniqueTimes.slice(1).map((value, index) => value - uniqueTimes[index]).filter((value) => value > 0);
    rangeStep = gaps.length ? Math.max(1, Math.min(...gaps)) : 1;
    const initial = analysis.declineEvents[0]?.metricLabel || analysis.metrics[0]?.label || "";
    selectedMetricLabels = initial ? [initial] : [];
    activeModule = "data";
    selectedPoint = null;
  }

  function toggleMetric(label: string): void {
    if (selectedMetricLabels.includes(label)) {
      if (selectedMetricLabels.length > 1) {
        selectedMetricLabels = selectedMetricLabels.filter((item) => item !== label);
      }
      return;
    }
    selectedMetricLabels = [...selectedMetricLabels.slice(-3), label];
  }

  function resetRange(): void {
    rangeStart = fullRangeStart;
    rangeEnd = fullRangeEnd;
  }

  function pointValid(point: CompassMetricPoint): boolean {
    return point.valid !== false && Number.isFinite(point.value);
  }

  function rangeLabel(sortValue: number): string {
    const points = analysis.metrics.flatMap((metric) => metric.points);
    return points.reduce<CompassMetricPoint | null>((nearest, point) => (
      !nearest || Math.abs(point.sortValue - sortValue) < Math.abs(nearest.sortValue - sortValue)
        ? point
        : nearest
    ), null)?.timeLabel || "—";
  }

  function trendLabel(item: CompassMetricWindowAnalysis): string {
    if (item.trendDirection === "not_applicable") return "无法计算";
    const direction = item.trendDirection === "rising" ? "上升" : item.trendDirection === "falling" ? "下降" : "稳定";
    return `${direction} ${item.trendPercent == null ? "" : signedPercent(item.trendPercent)}`.trim();
  }

  function statusLabel(item: CompassMetricWindowAnalysis): string {
    if (item.status === "no_data") return "无数据";
    if (item.status === "insufficient") return `样本不足 · ${item.validPointCount}点`;
    if (item.status === "flat") return `平坦曲线 · ${item.validPointCount}点`;
    return `${item.validPointCount}个有效点`;
  }

  function formatNumber(value: number | null, unit = ""): string {
    if (value == null || !Number.isFinite(value)) return "—";
    return `${value.toLocaleString("zh-CN", { maximumFractionDigits: 2 })}${unit}`;
  }

  function signedPercent(value: number): string {
    return `${value > 0 ? "+" : ""}${value.toFixed(1)}%`;
  }

  function chartX(sortValue: number): number {
    return 34 + ((sortValue - rangeStart) / Math.max(rangeEnd - rangeStart, 1)) * 790;
  }

  function metricDisplayValue(metric: CompassMetricAnalysis, point: CompassMetricPoint): number | null {
    if (!pointValid(point)) return null;
    if (!normalizedComparison) return point.value;
    const points = metric.points.filter((item) => item.sortValue >= rangeStart && item.sortValue <= rangeEnd);
    return normalizeCompassMetricValues(points)[points.indexOf(point)] ?? null;
  }

  function chartY(metric: CompassMetricAnalysis, point: CompassMetricPoint): number {
    const value = metricDisplayValue(metric, point) ?? chartMinimum;
    return 22 + (1 - (value - chartMinimum) / Math.max(chartMaximum - chartMinimum, 1)) * 158;
  }

  function chartPath(metric: CompassMetricAnalysis): string {
    const points = metric.points.filter((point) => point.sortValue >= rangeStart && point.sortValue <= rangeEnd);
    let penDown = false;
    return points.map((point) => {
      if (!pointValid(point)) {
        penDown = false;
        return "";
      }
      const command = penDown ? "L" : "M";
      penDown = true;
      return `${command}${chartX(point.sortValue).toFixed(1)},${chartY(metric, point).toFixed(1)}`;
    }).filter(Boolean).join(" ");
  }

  function evidenceFor(metricLabel: string, start: number, end: number): CompassDeclineEvent | null {
    return analysis.declineEvents.find((event) =>
      event.metricLabel === metricLabel
      && event.endSortValue >= start
      && event.startSortValue <= end
    ) || null;
  }

  function seekEvidence(event: CompassDeclineEvent): void {
    dispatch("seek", event.seekSeconds ?? event.transcriptStartSeconds ?? 0);
  }

  function pointKey(metric: CompassMetricAnalysis, point: CompassMetricPoint): string {
    return `${metric.label}|${point.sortValue}|${point.timeLabel}`;
  }

  function selectPoint(metric: CompassMetricAnalysis, point: CompassMetricPoint): void {
    selectedPoint = {
      key: pointKey(metric, point),
      metric,
      point,
      insight: analyzeCompassPoint(metric, point),
      offsetSeconds: compassPointOffsetSeconds(point, sessionStartedAt),
      evidence: evidenceFor(metric.label, point.sortValue, point.sortValue),
    };
  }

  function pointKeydown(event: KeyboardEvent, metric: CompassMetricAnalysis, point: CompassMetricPoint): void {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    selectPoint(metric, point);
  }

  function seekSelectedPoint(): void {
    if (selectedPoint?.offsetSeconds != null) dispatch("seek", selectedPoint.offsetSeconds);
  }

  function timeOffset(time: string): number | null {
    if (!time) return null;
    return compassPointOffsetSeconds({ timeLabel: time, sortValue: Number.NaN }, sessionStartedAt);
  }

  function audienceDimensions(): string[] {
    return [...new Set((analysis.audience ?? []).map((item) => item.dimension))];
  }

  function coverageMessage(key: string, fallback: string): string {
    const item = (analysis.coverage ?? []).find((entry) => entry.key === key);
    if (!item) return fallback;
    if (item.status === "permission_denied") return `${item.label}：当前账号无权限`;
    if (item.status === "unavailable") return `${item.label}：当前场次未提供`;
    if (item.status === "untriggered") return `${item.label}：页面已触发但未捕获到响应`;
    return item.message || fallback;
  }
</script>

<section class="curve-workbench" aria-label="按指标和时间范围分析罗盘原始曲线">
  <nav class="module-tabs" aria-label="罗盘大屏模块">
    <button type="button" class:active={activeModule === "data"} aria-pressed={activeModule === "data"} on:click={() => activeModule = "data"}>数据 <span>{analysis.metrics.length}</span></button>
    <button type="button" class:active={activeModule === "product"} aria-pressed={activeModule === "product"} on:click={() => activeModule = "product"}>商品 <span>{(analysis.products ?? []).length}</span></button>
    <button type="button" class:active={activeModule === "audience"} aria-pressed={activeModule === "audience"} on:click={() => activeModule = "audience"}>人群 <span>{(analysis.audience ?? []).length}</span></button>
    <button type="button" class:active={activeModule === "qianchuan"} aria-pressed={activeModule === "qianchuan"} on:click={() => activeModule = "qianchuan"}>千川 <span>{(analysis.qianchuan ?? []).length}</span></button>
  </nav>

  {#if activeModule === "data"}
  {#if analysis.aiSummary}
    <aside class="ai-summary" aria-label="AI整场综合分析"><strong>AI整场综合分析</strong><p>{analysis.aiSummary}</p><small>结论只使用已采集模块；相关性不代表因果。</small></aside>
  {/if}
  <div class="curve-controls">
    <fieldset class="metric-selector">
      <legend>分析指标（最多4项）</legend>
      <div aria-label="切换或组合曲线指标">
        {#each analysis.metrics as metric (metric.label)}
          <button
            type="button"
            aria-pressed={selectedMetricLabels.includes(metric.label)}
            class:active={selectedMetricLabels.includes(metric.label)}
            on:click={() => toggleMetric(metric.label)}
          >{metric.label}</button>
        {/each}
      </div>
    </fieldset>
    <fieldset class="range-selector">
      <legend>当前分析时间范围</legend>
      <div class="range-labels"><strong>{rangeLabel(rangeStart)}</strong><span>至</span><strong>{rangeLabel(rangeEnd)}</strong></div>
      <label><span>开始时间</span><input aria-label="曲线分析开始时间" type="range" min={fullRangeStart} max={rangeEnd} step={rangeStep} bind:value={rangeStart} /></label>
      <label><span>结束时间</span><input aria-label="曲线分析结束时间" type="range" min={rangeStart} max={fullRangeEnd} step={rangeStep} bind:value={rangeEnd} /></label>
      <button type="button" class="secondary-btn" on:click={resetRange}>恢复整场</button>
    </fieldset>
  </div>

  {#if normalizedComparison}
    <p class="normalization-note" role="status"><AlertTriangle size={14} aria-hidden="true" />不同量纲按当前窗口各自归一化为0–100显示；统计和事件仍保留原始数值，不能直接比较绝对值。</p>
  {/if}

  <div class="analysis-grid" aria-live="polite">
    {#each windowAnalyses as item (item.metricId)}
      <article>
        <header><strong>{item.label}</strong><span>{statusLabel(item)}</span></header>
        <dl>
          <div><dt>整体趋势</dt><dd class:negative={item.trendDirection === "falling"}>{trendLabel(item)}</dd></div>
          <div><dt>峰值</dt><dd>{formatNumber(item.maximum, item.unit)}<small>{item.maximumTime || "—"}</small></dd></div>
          <div><dt>低谷</dt><dd>{formatNumber(item.minimum, item.unit)}<small>{item.minimumTime || "—"}</small></dd></div>
          <div><dt>变化 / 异常</dt><dd>{item.changes.length} / {item.anomalies.length}<small>{item.missingPointCount ? `缺失${item.missingPointCount}点` : "数据连续"}</small></dd></div>
        </dl>
      </article>
    {/each}
  </div>

  <article class="chart-card">
    <div class="chart-meta">
      <strong>罗盘原始分钟曲线</strong>
      <span>{rangeLabel(rangeStart)}–{rangeLabel(rangeEnd)}</span>
    </div>
    <div class="chart-legend" aria-label="曲线图例">
      {#each selectedMetrics as metric, index (metric.label)}
        {@const style = compassMetricStyle(index)}
        <span><svg viewBox="0 0 42 16" aria-hidden="true"><line x1="1" y1="8" x2="41" y2="8" stroke={style.color} stroke-width="2.5" stroke-dasharray={style.dash} />{#if style.shape === "square"}<rect x="18" y="5" width="6" height="6" fill={style.color} />{:else if style.shape === "triangle"}<path d="M21 4 L17 12 L25 12 Z" fill={style.color} />{:else if style.shape === "diamond"}<path d="M21 4 L25 8 L21 12 L17 8 Z" fill={style.color} />{:else}<circle cx="21" cy="8" r="3" fill={style.color} />{/if}</svg>{metric.label}</span>
      {/each}
    </div>
    <!-- svelte-ignore a11y-no-noninteractive-tabindex -->
    <div class="chart-scroll" role="region" tabindex="0" aria-label="可横向滚动的多指标曲线图">
      <svg viewBox="0 0 860 214" role="img" aria-label={`${selectedMetricLabels.join("、")}在${rangeLabel(rangeStart)}至${rangeLabel(rangeEnd)}的曲线、变化点和异常区间`}>
        <line x1="34" y1="180" x2="824" y2="180" class="axis" />
        <line x1="34" y1="22" x2="34" y2="180" class="axis" />
        {#each windowAnalyses as item}
          {#each item.anomalies as event (event.id)}
            <rect x={chartX(event.startSortValue)} y="22" width={Math.max(5, chartX(event.endSortValue) - chartX(event.startSortValue))} height="158" class="anomaly-band"><title>{item.label} {event.startTime}–{event.endTime}异常</title></rect>
            <text x={Math.min(788, chartX(event.startSortValue) + 4)} y="36" class="anomaly-label">异常</text>
          {/each}
        {/each}
        {#each selectedMetrics as metric, index (metric.label)}
          {@const style = compassMetricStyle(index)}
          <path d={chartPath(metric)} class="metric-line" stroke={style.color} stroke-dasharray={style.dash} />
          {#each metric.points.filter((point) => point.sortValue >= rangeStart && point.sortValue <= rangeEnd && pointValid(point)) as point}
            <!-- svelte-ignore a11y-no-static-element-interactions -->
            <g
              class="point-target"
              class:selected={selectedPoint?.key === pointKey(metric, point)}
              role="button"
              tabindex="0"
              aria-label={`${metric.label}，${point.timeLabel}，${formatNumber(point.value, metric.unit)}；选择后可同步录播和文稿`}
              on:click={() => selectPoint(metric, point)}
              on:keydown={(event) => pointKeydown(event, metric, point)}
            >
              <circle class="point-hit" cx={chartX(point.sortValue)} cy={chartY(metric, point)} r="9" />
              {#if style.shape === "square"}
                <rect x={chartX(point.sortValue)-2.7} y={chartY(metric, point)-2.7} width="5.4" height="5.4" fill={style.color} />
              {:else if style.shape === "triangle"}
                <path d={`M${chartX(point.sortValue)},${chartY(metric, point)-3.5} L${chartX(point.sortValue)-3.5},${chartY(metric, point)+3} L${chartX(point.sortValue)+3.5},${chartY(metric, point)+3} Z`} fill={style.color} />
              {:else if style.shape === "diamond"}
                <path d={`M${chartX(point.sortValue)},${chartY(metric, point)-3.5} L${chartX(point.sortValue)+3.5},${chartY(metric, point)} L${chartX(point.sortValue)},${chartY(metric, point)+3.5} L${chartX(point.sortValue)-3.5},${chartY(metric, point)} Z`} fill={style.color} />
              {:else}
                <circle cx={chartX(point.sortValue)} cy={chartY(metric, point)} r="2.7" fill={style.color} />
              {/if}
              <title>{metric.label} {point.timeLabel}：{formatNumber(point.value, metric.unit)}</title>
            </g>
          {/each}
          {@const lastPoint = [...metric.points].reverse().find((point) => point.sortValue >= rangeStart && point.sortValue <= rangeEnd && pointValid(point))}
          {#if lastPoint}<text x={Math.min(786, chartX(lastPoint.sortValue)+6)} y={Math.max(16, chartY(metric, lastPoint)-6)} fill={style.color} class="series-label">{metric.label}</text>{/if}
        {/each}
      </svg>
    </div>
    {#if selectedPoint}
      <section class="point-insight" aria-live="polite" aria-label="选中曲线点分析">
        <div><strong>{selectedPoint.metric.label} · {selectedPoint.point.timeLabel}</strong><b>{formatNumber(selectedPoint.point.value, selectedPoint.metric.unit)}</b></div>
        <dl>
          <div><dt>相对上一点</dt><dd>{selectedPoint.insight.previousValue == null ? "首个有效点" : `${formatNumber(selectedPoint.insight.previousValue, selectedPoint.metric.unit)} → ${formatNumber(selectedPoint.point.value, selectedPoint.metric.unit)}`}</dd></div>
          <div><dt>变化幅度</dt><dd>{selectedPoint.insight.changePercent == null ? "无法计算" : signedPercent(selectedPoint.insight.changePercent)}</dd></div>
          <div><dt>录播位置</dt><dd>{formatCompassOffset(selectedPoint.offsetSeconds)}</dd></div>
          <div><dt>证据状态</dt><dd>{selectedPoint.evidence?.transcriptExcerpt ? "已有逐字稿证据" : "待核实"}</dd></div>
        </dl>
        <p>{selectedPoint.evidence?.transcriptExcerpt || "待核实：请同步到该时间点，结合主播原话和节奏地图判断；同期变化不等于因果。"}</p>
        <button type="button" disabled={selectedPoint.offsetSeconds == null} on:click={seekSelectedPoint}><Play size={14} aria-hidden="true" />同步录播/文稿</button>
      </section>
    {/if}
    <details class="raw-table"><summary>查看当前窗口原始数据点</summary><!-- svelte-ignore a11y-no-noninteractive-tabindex --><div role="region" tabindex="0" aria-label="当前窗口原始数据表"><table><thead><tr><th>指标</th><th>时间</th><th>数值</th><th>状态</th></tr></thead><tbody>{#each selectedMetrics as metric}{#each metric.points.filter((point) => point.sortValue >= rangeStart && point.sortValue <= rangeEnd) as point}<tr><td>{metric.label}</td><td>{point.timeLabel}</td><td>{pointValid(point) ? formatNumber(point.value, metric.unit) : "—"}</td><td>{pointValid(point) ? "有效" : "缺失"}</td></tr>{/each}{/each}</tbody></table></div></details>
  </article>

  <section class="event-section" aria-label="当前窗口异常和明显变化事件">
    <header><div><AlertTriangle size={17} aria-hidden="true" /><div><strong>当前窗口事件</strong><span>证据不足只写待核实；同期变化不等于因果。</span></div></div><b>{curveEvents.length}项</b></header>
    {#if curveEvents.length}
      <!-- svelte-ignore a11y-no-noninteractive-tabindex -->
      <div class="event-table" role="region" tabindex="0" aria-label="当前窗口异常和变化事件表"><table><thead><tr><th>指标</th><th>类型</th><th>时间</th><th>数值</th><th>变化</th><th>证据</th><th>操作</th></tr></thead><tbody>{#each curveEvents as event (event.id)}<tr><td>{event.metricLabel}</td><td>{event.kind}</td><td>{event.time}</td><td>{formatNumber(event.value, event.unit)}</td><td>{event.detail}</td><td>{compassEvidenceExplanation(event.evidence)}</td><td><button type="button" disabled={!event.evidence} on:click={() => event.evidence && seekEvidence(event.evidence)}><Play size={13} aria-hidden="true" />同步录播/文稿</button></td></tr>{/each}</tbody></table></div>
    {:else}
      <div class="empty-events" role="status">当前指标和时间范围没有达到阈值的变化点或异常区间。</div>
    {/if}
  </section>
  {:else if activeModule === "product"}
    <section class="module-panel" aria-label="罗盘商品数据">
      <header><div><strong>商品</strong><span>来自官方罗盘响应；有讲解时间时可直接回看。</span></div><b>{(analysis.products ?? []).length}项</b></header>
      {#if (analysis.products ?? []).length}
        <!-- svelte-ignore a11y-no-noninteractive-tabindex -->
        <div class="module-table" role="region" tabindex="0" aria-label="商品数据表"><table><thead><tr><th>商品</th><th>讲解次数</th><th>点击</th><th>成交金额</th><th>成交件数</th><th>讲解时间</th><th>操作</th></tr></thead><tbody>
          {#each analysis.products ?? [] as product (product.productId || product.productName)}
            {@const offset = timeOffset(product.explainStartTime || "")}
            <tr><td>{product.productName}</td><td>{product.explainCount}</td><td>{product.clickCount}</td><td>{formatNumber(product.paymentAmount, "元")}</td><td>{product.soldCount}</td><td>{product.explainStartTime || "未返回"}{product.explainEndTime ? `–${product.explainEndTime}` : ""}</td><td><button type="button" disabled={offset == null} on:click={() => offset != null && dispatch("seek", offset)}><Play size={13} aria-hidden="true" />回看讲解</button></td></tr>
          {/each}
        </tbody></table></div>
      {:else}<div class="module-empty">{coverageMessage("module.product", "当前采集未返回商品明细，不按 0 处理。")}</div>{/if}
    </section>
  {:else if activeModule === "audience"}
    <section class="module-panel" aria-label="罗盘人群数据">
      <header><div><strong>人群</strong><span>性别、年龄、地域等官方画像响应。</span></div><b>{(analysis.audience ?? []).length}项</b></header>
      {#if (analysis.audience ?? []).length}
        <div class="audience-grid">
          {#each audienceDimensions() as dimension}
            <article><h3>{dimension}</h3>{#each (analysis.audience ?? []).filter((item) => item.dimension === dimension) as item}<div class="audience-row"><div><span>{item.label}</span><b>{formatNumber(item.value, item.unit)}</b></div><div class="audience-bar"><i style={`width:${Math.min(100, Math.max(0, item.value <= 1 ? item.value * 100 : item.value))}%`}></i></div></div>{/each}</article>
          {/each}
        </div>
      {:else}<div class="module-empty">{coverageMessage("module.audience", "当前采集未返回人群画像，不按 0 处理。")}</div>{/if}
    </section>
  {:else}
    <section class="module-panel" aria-label="罗盘千川数据">
      <header><div><strong>千川</strong><span>仅展示官方响应明确返回的投放指标。</span></div><b>{(analysis.qianchuan ?? []).length}项</b></header>
      {#if (analysis.qianchuan ?? []).length}
        <div class="qianchuan-grid">{#each analysis.qianchuan ?? [] as item (item.key)}<article><span>{item.label}</span><strong>{formatNumber(item.value, item.unit)}</strong><small title={item.sourceEndpoint}>响应字段：{item.key}</small></article>{/each}</div>
      {:else}<div class="module-empty">{coverageMessage("module.qianchuan", "当前采集未返回千川投放指标，不按 0 处理。")}</div>{/if}
    </section>
  {/if}
</section>

<style>
  .curve-workbench { min-width: 0; display: grid; gap: 10px; }
  .module-tabs { display: flex; gap: 6px; padding: 4px; border: 1px solid #e4e7ec; border-radius: 10px; background: #f9fafb; overflow-x: auto; }
  .module-tabs button { min-width: 92px; min-height: 44px; display: inline-flex; align-items: center; justify-content: center; gap: 7px; padding: 0 14px; border: 1px solid transparent; border-radius: 8px; background: transparent; color: #475467; font-size: 11px; cursor: pointer; }
  .module-tabs button.active { border-color: #b2ccff; background: #fff; color: #175cd3; font-weight: 700; box-shadow: 0 1px 2px rgba(16, 24, 40, .06); }
  .module-tabs span { min-width: 20px; padding: 2px 5px; border-radius: 999px; background: #eaecf0; color: #667085; font-size: 9px; }
  .ai-summary { display: grid; gap: 5px; padding: 10px 11px; border: 1px solid #b2ccff; border-radius: 10px; background: #f5f8ff; }
  .ai-summary strong { color: #175cd3; font-size: 12px; }
  .ai-summary p { margin: 0; color: #344054; font-size: 11px; line-height: 1.6; white-space: pre-wrap; }
  .ai-summary small { color: #667085; font-size: 9px; }
  .curve-controls { display: grid; grid-template-columns: minmax(0, 1.35fr) minmax(260px, .65fr); gap: 10px; }
  fieldset { min-width: 0; margin: 0; padding: 9px 10px 10px; border: 1px solid #e4e7ec; border-radius: 10px; }
  legend { padding: 0 5px; color: #667085; font-size: 10px; font-weight: 650; }
  .metric-selector > div { display: flex; flex-wrap: wrap; gap: 7px; }
  .metric-selector button, .secondary-btn, .event-table button { min-height: 44px; border: 1px solid #d0d5dd; border-radius: 9px; background: #fff; color: #475467; cursor: pointer; }
  .metric-selector button { padding: 0 12px; font-size: 11px; }
  .metric-selector button.active { border-color: #84adff; background: #eff4ff; color: #175cd3; font-weight: 650; }
  button:focus-visible, input:focus-visible, summary:focus-visible, [tabindex="0"]:focus-visible { outline: 3px solid rgba(23, 105, 210, .35); outline-offset: 2px; }
  .range-selector { display: grid; gap: 5px; }
  .range-selector label { display: grid; grid-template-columns: 56px 1fr; align-items: center; gap: 8px; color: #667085; font-size: 10px; }
  .range-selector input { width: 100%; }
  .range-labels { display: flex; justify-content: space-between; gap: 8px; color: #344054; font-size: 10px; font-variant-numeric: tabular-nums; }
  .range-labels span { color: #98a2b3; }
  .secondary-btn { justify-self: start; min-height: 36px; padding: 0 11px; font-size: 10px; }
  .normalization-note { display: flex; align-items: center; gap: 6px; margin: 0; padding: 8px 9px; border: 1px solid #fedf89; border-radius: 8px; background: #fffaeb; color: #93370d; font-size: 10px; line-height: 1.45; }
  .analysis-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; }
  .analysis-grid article { min-width: 0; padding: 9px 10px; border: 1px solid #e4e7ec; border-radius: 9px; background: #fcfcfd; }
  .analysis-grid header, .event-section > header, .event-section > header > div { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .analysis-grid header strong { color: #101828; font-size: 12px; }
  .analysis-grid header span { color: #667085; font-size: 9px; }
  .analysis-grid dl { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 7px; margin: 8px 0 0; }
  .analysis-grid dl div { min-width: 0; padding-top: 6px; border-top: 1px solid #eaecf0; }
  dt { color: #98a2b3; font-size: 9px; }
  dd { display: grid; gap: 2px; margin: 3px 0 0; color: #101828; font-size: 11px; font-weight: 650; font-variant-numeric: tabular-nums; }
  dd.negative { color: #b42318; }
  dd small { overflow: hidden; color: #667085; font-size: 8px; font-weight: 400; text-overflow: ellipsis; white-space: nowrap; }
  .chart-card, .event-section { min-width: 0; border: 1px solid #e4e7ec; border-radius: 10px; background: #fff; overflow: hidden; }
  .chart-meta { display: flex; justify-content: space-between; gap: 10px; padding: 9px 10px 5px; color: #667085; font-size: 9px; }
  .chart-meta strong { color: #067647; }
  .chart-legend { display: flex; flex-wrap: wrap; gap: 6px 12px; padding: 0 10px 6px; color: #475467; font-size: 10px; }
  .chart-legend span { display: inline-flex; align-items: center; gap: 4px; }
  .chart-legend svg { width: 34px; height: 14px; }
  .chart-scroll { min-width: 0; overflow-x: auto; }
  .chart-scroll > svg { display: block; width: max(100%, 720px); height: auto; min-height: 190px; }
  .axis { stroke: #d0d5dd; stroke-width: 1; }
  .metric-line { fill: none; stroke-width: 2.4; stroke-linecap: round; stroke-linejoin: round; vector-effect: non-scaling-stroke; }
  .anomaly-band { fill: rgba(217, 45, 32, .1); stroke: #d92d20; stroke-width: .7; stroke-dasharray: 4 3; }
  .anomaly-label { fill: #b42318; font-size: 8px; font-weight: 700; }
  .series-label { font-size: 8px; font-weight: 700; paint-order: stroke; stroke: #fff; stroke-width: 3px; }
  .point-target { cursor: pointer; }
  .point-hit { fill: transparent; stroke: transparent; }
  .point-target:hover .point-hit, .point-target:focus-visible .point-hit, .point-target.selected .point-hit { fill: rgba(23, 105, 210, .12); stroke: #175cd3; stroke-width: 1.5; }
  .point-target:focus { outline: none; }
  .point-insight { display: grid; gap: 8px; padding: 10px; border-top: 1px solid #d6e4ff; background: #f5f8ff; }
  .point-insight > div { display: flex; align-items: center; justify-content: space-between; gap: 8px; color: #101828; font-size: 11px; }
  .point-insight > div b { color: #175cd3; font-variant-numeric: tabular-nums; }
  .point-insight dl { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 8px; margin: 0; }
  .point-insight dl div { min-width: 0; padding: 7px; border: 1px solid #d6e4ff; border-radius: 7px; background: #fff; }
  .point-insight p { margin: 0; color: #475467; font-size: 10px; line-height: 1.5; white-space: pre-wrap; }
  .point-insight button, .module-panel button { min-height: 44px; display: inline-flex; align-items: center; justify-content: center; gap: 5px; justify-self: start; padding: 0 12px; border: 1px solid #84adff; border-radius: 8px; background: #fff; color: #175cd3; font-size: 10px; cursor: pointer; }
  .point-insight button:disabled, .module-panel button:disabled { opacity: .45; cursor: not-allowed; }
  .raw-table summary { min-height: 36px; display: flex; align-items: center; padding: 0 10px; border-top: 1px solid #eaecf0; color: #475467; font-size: 10px; cursor: pointer; }
  .raw-table > div, .event-table { overflow: auto; }
  table { width: 100%; border-collapse: collapse; font-size: 10px; }
  th, td { padding: 7px 9px; border-top: 1px solid #eaecf0; color: #475467; text-align: left; vertical-align: top; white-space: nowrap; }
  th { color: #667085; background: #f9fafb; }
  .event-section > header { padding: 9px 10px; }
  .event-section > header > div { justify-content: flex-start; color: #b42318; }
  .event-section > header > div > div { display: grid; gap: 2px; }
  .event-section header strong { color: #101828; font-size: 12px; }
  .event-section header span { color: #667085; font-size: 9px; }
  .event-section header b { color: #475467; font-size: 10px; }
  .event-table td:nth-child(6) { min-width: 240px; white-space: normal; line-height: 1.45; }
  .event-table button { min-width: 120px; display: inline-flex; align-items: center; justify-content: center; gap: 5px; padding: 0 9px; color: #175cd3; font-size: 10px; }
  .event-table button:disabled { opacity: .45; cursor: not-allowed; }
  .empty-events { padding: 18px; border-top: 1px solid #eaecf0; color: #667085; font-size: 10px; text-align: center; }
  .module-panel { min-width: 0; border: 1px solid #e4e7ec; border-radius: 10px; background: #fff; overflow: hidden; }
  .module-panel > header { min-height: 54px; display: flex; align-items: center; justify-content: space-between; gap: 10px; padding: 9px 11px; border-bottom: 1px solid #eaecf0; }
  .module-panel > header div { display: grid; gap: 2px; }
  .module-panel > header strong { color: #101828; font-size: 13px; }
  .module-panel > header span { color: #667085; font-size: 10px; }
  .module-panel > header b { color: #475467; font-size: 10px; }
  .module-table { overflow: auto; }
  .module-empty { min-height: 190px; display: grid; place-items: center; padding: 24px; color: #667085; font-size: 11px; text-align: center; }
  .audience-grid, .qianchuan-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 9px; padding: 10px; }
  .audience-grid article, .qianchuan-grid article { min-width: 0; padding: 10px; border: 1px solid #e4e7ec; border-radius: 9px; background: #fcfcfd; }
  .audience-grid h3 { margin: 0 0 9px; color: #101828; font-size: 12px; }
  .audience-row { display: grid; gap: 4px; margin-top: 8px; }
  .audience-row > div:first-child { display: flex; align-items: center; justify-content: space-between; gap: 8px; color: #475467; font-size: 10px; }
  .audience-bar { height: 6px; border-radius: 999px; background: #eaecf0; overflow: hidden; }
  .audience-bar i { display: block; height: 100%; border-radius: inherit; background: #175cd3; }
  .qianchuan-grid { grid-template-columns: repeat(3, minmax(0, 1fr)); }
  .qianchuan-grid article { display: grid; gap: 5px; }
  .qianchuan-grid span, .qianchuan-grid small { overflow: hidden; color: #667085; font-size: 9px; text-overflow: ellipsis; white-space: nowrap; }
  .qianchuan-grid strong { color: #101828; font-size: 18px; font-variant-numeric: tabular-nums; }
  @media (prefers-reduced-motion: reduce) { * { scroll-behavior: auto !important; transition: none !important; } }
  @media (max-width: 900px) { .curve-controls { grid-template-columns: 1fr; } .analysis-grid { grid-template-columns: 1fr; } }
  @media (max-width: 700px) { .analysis-grid dl, .point-insight dl, .audience-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } .qianchuan-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } .chart-scroll > svg { width: 720px; } }
  @media (max-width: 420px) { .metric-selector button { width: 100%; } .range-selector label { grid-template-columns: 1fr; } .analysis-grid dl, .point-insight dl, .audience-grid, .qianchuan-grid { grid-template-columns: 1fr; } }
</style>
