<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { X, Save, Trash2, Star, GitCompare, Database } from "lucide-svelte";
  import { invoke } from "../../invoker";
  import type { ReviewSample, ReviewSampleInput, VideoItem } from "../../interface";

  export let showDialog = false;
  export let reviewContent = "";

  const dispatch = createEventDispatcher();
  let samples: ReviewSample[] = [];
  let videos: VideoItem[] = [];
  let loading = false;
  let saving = false;
  let error = "";
  let wasOpen = false;

  const emptyForm = (): ReviewSampleInput => ({
    sample_no: "",
    product: "",
    category: "",
    deal_status: "片段内待确认",
    evidence_strength: "弱",
    transcript_path: "",
    data_screenshot_path: "",
    review_status: "已复盘",
    is_b_baseline: false,
    ops_score: null,
    host_score: null,
    control_score: null,
    main_issue: "",
    notes: "",
    clip_type: "无法判断",
    source_video_path: "",
    review_file_path: "",
    transcription_quality: "",
    agent_version: "V1.1",
    calibration_score: null,
    fact_accuracy_score: null,
    key_action_score: null,
    oral_usability_score: null,
    training_value_score: null,
    review_content: "",
    video_id: null,
  });

  let form = emptyForm();

  $: if (showDialog && !wasOpen) {
    wasOpen = true;
    loadData();
  } else if (!showDialog) {
    wasOpen = false;
  }

  function nextSampleNo(items: ReviewSample[]) {
    const max = items.reduce((current, item) => {
      const match = /^M1-(\d+)$/.exec(item.sample_no);
      return match ? Math.max(current, Number(match[1])) : current;
    }, 0);
    return `M1-${String(max + 1).padStart(3, "0")}`;
  }

  async function loadData() {
    loading = true;
    error = "";
    try {
      const [sampleRows, videoRows] = await Promise.all([
        invoke<ReviewSample[]>("get_review_samples"),
        invoke<VideoItem[]>("get_all_videos"),
      ]);
      samples = sampleRows || [];
      if (!samples.some((item) => item.sample_no === "M1-001") || !samples.some((item) => item.sample_no === "M1-002")) {
        try {
          samples = await invoke<ReviewSample[]>("seed_builtin_review_samples");
        } catch (seedError) {
          console.warn("当前运行模式无法写入内置M1样本", seedError);
        }
      }
      videos = videoRows || [];
      if (!form.id) {
        const latestVideo = videos[0];
        form = {
          ...emptyForm(),
          sample_no: nextSampleNo(samples),
          review_content: reviewContent,
          video_id: latestVideo?.id ?? null,
          source_video_path: latestVideo?.file ?? "",
        };
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  function editSample(sample: ReviewSample) {
    form = {
      ...sample,
      is_b_baseline: sample.is_b_baseline === 1,
      id: sample.id,
    };
  }

  function newSample() {
    const latestVideo = videos[0];
    form = {
      ...emptyForm(),
      sample_no: nextSampleNo(samples),
      review_content: reviewContent,
      video_id: latestVideo?.id ?? null,
      source_video_path: latestVideo?.file ?? "",
    };
  }

  function scoreValue(value: string): number | null {
    return value === "" ? null : Number(value);
  }

  async function saveSample() {
    if (!form.sample_no.trim() || !form.clip_type.trim()) {
      error = "请填写样本编号和片段类型";
      return;
    }
    saving = true;
    error = "";
    try {
      const saved = await invoke<ReviewSample>("save_review_sample", { sample: form });
      const index = samples.findIndex((item) => item.id === saved.id);
      samples = index >= 0
        ? samples.map((item) => item.id === saved.id ? saved : item)
        : [saved, ...samples];
      if (saved.is_b_baseline === 1) {
        samples = samples.map((item) =>
          item.category === saved.category && item.id !== saved.id
            ? { ...item, is_b_baseline: 0 }
            : item
        );
      }
      editSample(saved);
      dispatch("saved", saved);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }

  async function deleteSample(sample: ReviewSample) {
    if (!confirm(`确认删除样本 ${sample.sample_no}？`)) return;
    try {
      await invoke("delete_review_sample", { id: sample.id });
      samples = samples.filter((item) => item.id !== sample.id);
      if (form.id === sample.id) newSample();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
</script>

{#if showDialog}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
    <div class="flex max-h-[90vh] w-full max-w-6xl flex-col overflow-hidden rounded-2xl bg-white shadow-2xl dark:bg-gray-900">
      <div class="flex items-center justify-between border-b border-gray-200 px-6 py-4 dark:border-gray-800">
        <div class="flex items-center gap-3">
          <Database class="h-5 w-5 text-orange-500" />
          <div>
            <h2 class="font-semibold text-gray-900 dark:text-gray-100">M1 样本库与 B 基线</h2>
            <p class="text-xs text-gray-500">保存复盘结果、人工评分和同品类基线</p>
          </div>
        </div>
        <button on:click={() => showDialog = false} class="rounded-lg p-2 hover:bg-gray-100 dark:hover:bg-gray-800" aria-label="关闭">
          <X class="h-5 w-5" />
        </button>
      </div>

      <div class="grid min-h-0 flex-1 grid-cols-1 overflow-hidden lg:grid-cols-[360px_1fr]">
        <div class="overflow-y-auto border-r border-gray-200 p-4 dark:border-gray-800">
          <button on:click={newSample} class="mb-3 w-full rounded-lg border border-dashed border-orange-300 px-3 py-2 text-sm text-orange-600 hover:bg-orange-50 dark:hover:bg-orange-950/20">
            + 登记当前复盘
          </button>
          {#if loading}
            <p class="py-8 text-center text-sm text-gray-500">正在读取样本库…</p>
          {:else if samples.length === 0}
            <p class="py-8 text-center text-sm text-gray-500">还没有样本</p>
          {:else}
            <div class="space-y-2">
              {#each samples as sample}
                <div class="rounded-xl border p-3 {form.id === sample.id ? 'border-orange-400 bg-orange-50/60 dark:bg-orange-950/20' : 'border-gray-200 dark:border-gray-700'}">
                  <button class="w-full text-left" on:click={() => editSample(sample)}>
                    <div class="flex items-center justify-between gap-2">
                      <span class="text-sm font-semibold">{sample.sample_no}</span>
                      {#if sample.is_b_baseline === 1}<span class="inline-flex items-center gap-1 rounded bg-amber-100 px-1.5 py-0.5 text-[10px] text-amber-700"><Star class="h-3 w-3" />B基线</span>{/if}
                    </div>
                    <p class="mt-1 truncate text-xs text-gray-600 dark:text-gray-400">{sample.product || "未填写商品"} · {sample.clip_type}</p>
                  </button>
                  <div class="mt-2 flex justify-end gap-1">
                    <button on:click={() => dispatch('compare', sample)} class="rounded p-1.5 text-gray-500 hover:bg-gray-100" title="发起A/B对照"><GitCompare class="h-3.5 w-3.5" /></button>
                    <button on:click={() => deleteSample(sample)} class="rounded p-1.5 text-red-500 hover:bg-red-50" title="删除"><Trash2 class="h-3.5 w-3.5" /></button>
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>

        <div class="overflow-y-auto p-6">
          <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
            <label class="text-sm">样本编号<input bind:value={form.sample_no} class="mt-1 w-full rounded-lg border-gray-300 dark:bg-gray-800" /></label>
            <label class="text-sm">片段类型<select bind:value={form.clip_type} class="mt-1 w-full rounded-lg border-gray-300 dark:bg-gray-800"><option>成交片段</option><option>问价未成交</option><option>转品/上链接片段</option><option>讲得散片段</option><option>无法判断</option></select></label>
            <label class="text-sm">主商品<input bind:value={form.product} class="mt-1 w-full rounded-lg border-gray-300 dark:bg-gray-800" /></label>
            <label class="text-sm">品类<input bind:value={form.category} placeholder="如：二手镜头" class="mt-1 w-full rounded-lg border-gray-300 dark:bg-gray-800" /></label>
            <label class="text-sm">是否成交<select bind:value={form.deal_status} class="mt-1 w-full rounded-lg border-gray-300 dark:bg-gray-800"><option>已确认成交</option><option>片段内未确认成交</option><option>片段内待确认</option></select></label>
            <label class="text-sm">证据强度<select bind:value={form.evidence_strength} class="mt-1 w-full rounded-lg border-gray-300 dark:bg-gray-800"><option>强</option><option>中</option><option>弱</option></select></label>
            <label class="text-sm">转写质量<select bind:value={form.transcription_quality} class="mt-1 w-full rounded-lg border-gray-300 dark:bg-gray-800"><option value="">待填写</option><option>清晰</option><option>部分听不清</option><option>大量听不清</option><option>已人工校对</option></select></label>
            <label class="text-sm">复盘状态<select bind:value={form.review_status} class="mt-1 w-full rounded-lg border-gray-300 dark:bg-gray-800"><option>待复盘</option><option>已复盘</option><option>已校准</option></select></label>
            <label class="text-sm">源视频路径<input bind:value={form.source_video_path} class="mt-1 w-full rounded-lg border-gray-300 dark:bg-gray-800" /></label>
            <label class="text-sm">逐字稿路径<input bind:value={form.transcript_path} class="mt-1 w-full rounded-lg border-gray-300 dark:bg-gray-800" /></label>
            <label class="text-sm md:col-span-2">主要问题<input bind:value={form.main_issue} class="mt-1 w-full rounded-lg border-gray-300 dark:bg-gray-800" /></label>
            <div class="grid grid-cols-2 gap-3 md:col-span-2 md:grid-cols-4">
              <label class="text-sm">运营评分<input type="number" min="1" max="5" value={form.ops_score ?? ''} on:input={(e) => form.ops_score = scoreValue(e.currentTarget.value)} class="mt-1 w-full rounded-lg border-gray-300 dark:bg-gray-800" /></label>
              <label class="text-sm">主播评分<input type="number" min="1" max="5" value={form.host_score ?? ''} on:input={(e) => form.host_score = scoreValue(e.currentTarget.value)} class="mt-1 w-full rounded-lg border-gray-300 dark:bg-gray-800" /></label>
              <label class="text-sm">场控评分<input type="number" min="1" max="5" value={form.control_score ?? ''} on:input={(e) => form.control_score = scoreValue(e.currentTarget.value)} class="mt-1 w-full rounded-lg border-gray-300 dark:bg-gray-800" /></label>
              <label class="text-sm">综合校准<input type="number" min="1" max="5" value={form.calibration_score ?? ''} on:input={(e) => form.calibration_score = scoreValue(e.currentTarget.value)} class="mt-1 w-full rounded-lg border-gray-300 dark:bg-gray-800" /></label>
            </div>
            <div class="grid grid-cols-2 gap-3 rounded-xl bg-gray-50 p-3 md:col-span-2 md:grid-cols-4 dark:bg-gray-800/50">
              <label class="text-sm">事实准确<input type="number" min="1" max="5" value={form.fact_accuracy_score ?? ''} on:input={(e) => form.fact_accuracy_score = scoreValue(e.currentTarget.value)} class="mt-1 w-full rounded-lg border-gray-300 dark:bg-gray-800" /></label>
              <label class="text-sm">关键动作<input type="number" min="1" max="5" value={form.key_action_score ?? ''} on:input={(e) => form.key_action_score = scoreValue(e.currentTarget.value)} class="mt-1 w-full rounded-lg border-gray-300 dark:bg-gray-800" /></label>
              <label class="text-sm">口播可用<input type="number" min="1" max="5" value={form.oral_usability_score ?? ''} on:input={(e) => form.oral_usability_score = scoreValue(e.currentTarget.value)} class="mt-1 w-full rounded-lg border-gray-300 dark:bg-gray-800" /></label>
              <label class="text-sm">训练价值<input type="number" min="1" max="5" value={form.training_value_score ?? ''} on:input={(e) => form.training_value_score = scoreValue(e.currentTarget.value)} class="mt-1 w-full rounded-lg border-gray-300 dark:bg-gray-800" /></label>
            </div>
            <label class="flex items-center gap-2 text-sm md:col-span-2"><input type="checkbox" bind:checked={form.is_b_baseline} class="rounded border-gray-300 text-orange-500" /><span>设为该品类 B 基线（同品类仅保留一个）</span></label>
            <label class="text-sm md:col-span-2">备注<textarea bind:value={form.notes} rows="2" class="mt-1 w-full rounded-lg border-gray-300 dark:bg-gray-800"></textarea></label>
            <label class="text-sm md:col-span-2">复盘正文<textarea bind:value={form.review_content} rows="9" class="mt-1 w-full rounded-lg border-gray-300 font-mono text-xs dark:bg-gray-800"></textarea></label>
          </div>
          {#if error}<p class="mt-4 rounded-lg bg-red-50 p-3 text-sm text-red-600">{error}</p>{/if}
          <div class="mt-6 flex justify-end">
            <button on:click={saveSample} disabled={saving} class="inline-flex items-center gap-2 rounded-lg bg-orange-500 px-5 py-2.5 text-sm font-medium text-white hover:bg-orange-600 disabled:opacity-50"><Save class="h-4 w-4" />{saving ? "保存中…" : "保存样本"}</button>
          </div>
        </div>
      </div>
    </div>
  </div>
{/if}
