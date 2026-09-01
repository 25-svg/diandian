<script lang="ts">
  import LiveMetricCurveWorkbench from "../../src/lib/components/analysis/LiveMetricCurveWorkbench.svelte";
  import { COMPASS_CURVE_ACCEPTANCE } from "./compass-curve-acceptance";

  let seekSeconds: number | null = null;
</script>

<main>
  <h1>罗盘原始动态曲线验收</h1>
  <LiveMetricCurveWorkbench
    analysis={COMPASS_CURVE_ACCEPTANCE}
    sessionStartedAt="2026-08-12T10:00:00+08:00"
    on:seek={(event) => { seekSeconds = event.detail; }}
  />
  <output aria-live="polite">{seekSeconds == null ? "尚未跳转" : `已同步到 ${seekSeconds} 秒`}</output>
</main>

<style>
  :global(*) { box-sizing: border-box; }
  :global(html, body) { margin: 0; min-width: 0; background: #f5f7fa; font-family: Inter, "Microsoft YaHei", sans-serif; }
  main { width: min(100%, 1440px); min-width: 0; margin: 0 auto; padding: 16px; }
  h1 { margin: 0 0 12px; color: #101828; font-size: 18px; }
  output { display: block; margin-top: 10px; color: #344054; font-size: 12px; }
  @media (max-width: 420px) { main { padding: 8px; } }
</style>
