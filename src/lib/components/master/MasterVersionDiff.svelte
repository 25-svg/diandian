<script lang="ts">
  export let currentVersion = "1.0.0";
  export let nextVersion = "1.0.1";
  export let sectionKey = "";
  export let currentText = "";
  export let candidateText = "";
  export let confirmed = false;
  export let showConfirmation = true;
</script>
<section class="diff"><header><div><strong>下一版企业话术草稿</strong>{#if sectionKey}<small>对应章节：{sectionKey}</small>{/if}</div><span>V{currentVersion} → V{nextVersion}</span></header><div class="columns"><article><small>当前企业话术</small><p>{currentText}</p></article><article class="added"><small>下一版拟议内容</small><p>{candidateText}</p></article></div><p class="notice">发布前必须人工核对；发布后生成新版本，当前版本仍可追溯。</p>{#if showConfirmation}<label><input type="checkbox" bind:checked={confirmed}/>我已核对主播原话、建议稿、适用场景和待确认事实</label>{/if}</section>
<style>.diff{display:grid;gap:10px}.diff header{display:flex;justify-content:space-between;gap:10px}.diff header div{display:grid;gap:2px}.diff header small{color:#667085}.diff header>span{color:#007aff;font-size:13px}.columns{display:grid;grid-template-columns:1fr 1fr;gap:10px}.columns article{padding:12px;border:1px solid #e5e7eb;border-radius:8px;background:#fff}.columns .added{border-color:#b8e6c2;background:#f4fff6}.columns small{color:#6b7280}.columns p{white-space:pre-wrap;line-height:1.6}.notice{margin:0;color:#667085;font-size:12px}label{display:flex;gap:8px;font-size:13px}@media(max-width:700px){.columns{grid-template-columns:1fr}}</style>
