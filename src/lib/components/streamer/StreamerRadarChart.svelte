<script lang="ts">
  type RadarDimension = { key: string; label: string };
  type RadarSeries = {
    key: "current" | "previous";
    label: string;
    values: Record<string, number | null>;
  };

  export let dimensions: readonly RadarDimension[] = [];
  export let series: readonly RadarSeries[] = [];
  export let title = "主播八维能力雷达图";

  const centerX = 250;
  const centerY = 216;
  const radius = 142;
  const labelRadius = 184;
  const levels = [0.2, 0.4, 0.6, 0.8, 1];

  function point(index: number, scale: number, targetRadius = radius): { x: number; y: number } {
    const angle = -Math.PI / 2 + (Math.PI * 2 * index) / Math.max(dimensions.length, 1);
    return {
      x: centerX + Math.cos(angle) * targetRadius * scale,
      y: centerY + Math.sin(angle) * targetRadius * scale,
    };
  }

  function polygon(scale: number): string {
    return dimensions
      .map((_, index) => {
        const value = point(index, scale);
        return `${value.x.toFixed(2)},${value.y.toFixed(2)}`;
      })
      .join(" ");
  }

  function valuePoint(entry: RadarSeries, index: number): { x: number; y: number; score: number } | null {
    const score = entry.values[dimensions[index]?.key];
    if (typeof score !== "number" || !Number.isFinite(score)) return null;
    const safeScore = Math.max(0, Math.min(100, score));
    return { ...point(index, safeScore / 100), score: safeScore };
  }

  function completePoints(entry: RadarSeries): string | null {
    const points = dimensions.map((_, index) => valuePoint(entry, index));
    if (points.length !== dimensions.length || points.some((item) => item === null)) return null;
    return points.map((item) => `${item!.x.toFixed(2)},${item!.y.toFixed(2)}`).join(" ");
  }

  function availablePoints(entry: RadarSeries): string | null {
    const points = dimensions
      .map((_, index) => valuePoint(entry, index))
      .filter((item): item is { x: number; y: number; score: number } => item !== null);
    return points.length >= 2
      ? points.map((item) => `${item.x.toFixed(2)},${item.y.toFixed(2)}`).join(" ")
      : null;
  }

  function scoreLabelPoint(marker: { x: number; y: number }, index: number): { x: number; y: number } {
    const angle = -Math.PI / 2 + (Math.PI * 2 * index) / Math.max(dimensions.length, 1);
    return {
      x: marker.x + Math.cos(angle) * 16,
      y: marker.y + Math.sin(angle) * 16,
    };
  }

  function availableCount(entry: RadarSeries): number {
    return dimensions.filter((_, index) => valuePoint(entry, index) !== null).length;
  }

  function labelAnchor(index: number): "start" | "middle" | "end" {
    const label = point(index, 1, labelRadius);
    if (Math.abs(label.x - centerX) < 18) return "middle";
    return label.x < centerX ? "end" : "start";
  }

  $: visibleSeries = series.slice(0, 2);
  $: chartDescription = visibleSeries.length
    ? visibleSeries.map((entry) => `${entry.label}已有 ${availableCount(entry)}/${dimensions.length} 个维度达到样本门槛`).join("；")
    : "当前没有达到样本门槛的能力分数";
</script>

<figure class="radar-card" aria-labelledby="streamer-radar-title" aria-describedby="streamer-radar-description">
  <figcaption class="sr-only" id="streamer-radar-title">{title}</figcaption>
  <p class="sr-only" id="streamer-radar-description">{chartDescription}。精确分数与证据请查看旁边的能力分数表。</p>

  <svg viewBox="0 0 500 438" role="img" aria-labelledby="streamer-radar-title streamer-radar-description">
    {#each levels as level}
      <polygon class="grid-polygon" points={polygon(level)} />
    {/each}
    {#each dimensions as dimension, index (dimension.key)}
      {@const axisEnd = point(index, 1)}
      {@const label = point(index, 1, labelRadius)}
      <line class="axis-line" x1={centerX} y1={centerY} x2={axisEnd.x} y2={axisEnd.y} />
      <text
        class="axis-label"
        x={label.x}
        y={label.y}
        text-anchor={labelAnchor(index)}
        dominant-baseline="middle"
      >{dimension.label}</text>
    {/each}

    {#each visibleSeries as entry (entry.key)}
      {@const points = completePoints(entry)}
      {@const partialPoints = availablePoints(entry)}
      {#if points}
        <polygon
          class:current-polygon={entry.key === "current"}
          class:previous-polygon={entry.key === "previous"}
          points={points}
        />
      {:else if partialPoints}
        <polyline
          class:current-polygon={entry.key === "current"}
          class:previous-polygon={entry.key === "previous"}
          class="partial-series-line"
          points={partialPoints}
        />
      {/if}
      {#each dimensions as _, index}
        {@const marker = valuePoint(entry, index)}
        {#if marker}
          {#if entry.key === "current"}
            <circle class="current-marker" cx={marker.x} cy={marker.y} r="4.5" />
          {:else}
            <rect
              class="previous-marker"
              x={marker.x - 4}
              y={marker.y - 4}
              width="8"
              height="8"
              transform={`rotate(45 ${marker.x} ${marker.y})`}
            />
          {/if}
          {#if entry.key === "current"}
            {@const scoreLabel = scoreLabelPoint(marker, index)}
            <text
              class="score-label"
              x={scoreLabel.x}
              y={scoreLabel.y}
              text-anchor={labelAnchor(index)}
              dominant-baseline="middle"
              aria-hidden="true"
            >{Math.round(marker.score)}</text>
          {/if}
        {/if}
      {/each}
    {/each}
  </svg>

  <div class="legend" aria-hidden="true">
    {#each visibleSeries as entry (entry.key)}
      <span><i class:current-swatch={entry.key === "current"} class:previous-swatch={entry.key === "previous"}></i>{entry.label} · {availableCount(entry)}/{dimensions.length} 维</span>
    {:else}
      <span>等待人工审核证据</span>
    {/each}
  </div>
</figure>

<style>
  .radar-card { margin: 0; min-width: 0; }
  svg { display: block; width: 100%; min-height: 350px; overflow: visible; }
  .grid-polygon {
    fill: rgba(0, 113, 227, 0.035);
    stroke: color-mix(in srgb, var(--mac-secondary) 42%, transparent);
    stroke-width: 1.25;
  }
  .axis-line { stroke: color-mix(in srgb, var(--mac-secondary) 30%, transparent); stroke-width: 1.15; }
  .axis-label { fill: var(--mac-label); font-size: 12px; font-weight: 700; }
  .current-polygon {
    fill: rgba(0, 113, 227, 0.22);
    stroke: var(--mac-blue);
    stroke-width: 3.4;
    stroke-linejoin: round;
    stroke-linecap: round;
  }
  .previous-polygon {
    fill: rgba(191, 90, 242, 0.10);
    stroke: var(--mac-purple);
    stroke-width: 2.6;
    stroke-dasharray: 7 5;
    stroke-linejoin: round;
    stroke-linecap: round;
  }
  .partial-series-line { fill: none; }
  .current-marker { fill: var(--mac-blue); stroke: var(--mac-bg-card); stroke-width: 2.8; }
  .previous-marker { fill: var(--mac-bg-card); stroke: var(--mac-purple); stroke-width: 2.6; }
  .score-label {
    fill: var(--mac-blue);
    stroke: var(--mac-bg-card);
    stroke-width: 4px;
    paint-order: stroke;
    font-size: 11px;
    font-weight: 800;
  }
  .legend { display: flex; flex-wrap: wrap; justify-content: center; gap: 10px 18px; color: var(--mac-secondary); font-size: 12px; }
  .legend span { display: inline-flex; align-items: center; gap: 7px; }
  .legend i { display: inline-block; width: 24px; height: 0; border-top: 2px solid var(--mac-blue); }
  .legend .previous-swatch { border-top-style: dashed; border-top-color: var(--mac-purple); }
  @media (max-width: 720px) {
    svg { min-height: 300px; }
    .axis-label { font-size: 11px; }
  }
</style>
