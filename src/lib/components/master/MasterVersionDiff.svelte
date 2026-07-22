<script lang="ts">
  export let currentVersion = "1.0.0";
  export let nextVersion = "1.0.1";
  export let currentText = "";
  export let candidateText = "";
  export let confirmed = false;
  export let showConfirmation = true;
</script>
<section class="diff"><header><strong>确认母稿差异</strong><span>V{currentVersion} → V{nextVersion}</span></header><div class="columns"><article><small>当前母稿</small><p>{currentText}</p></article><article class="added"><small>准备加入的辅稿</small><p>{candidateText}</p></article></div>{#if showConfirmation}<label><input type="checkbox" bind:checked={confirmed}/>我已核对主播原话、适用场景和待确认事实</label>{/if}</section>
<style>.diff{display:grid;gap:10px}.diff header{display:flex;justify-content:space-between}.columns{display:grid;grid-template-columns:1fr 1fr;gap:10px}.columns article{padding:12px;border:1px solid #e5e7eb;border-radius:8px;background:#fff}.columns .added{border-color:#b8e6c2;background:#f4fff6}.columns small{color:#6b7280}.columns p{white-space:pre-wrap;line-height:1.6}label{display:flex;gap:8px;font-size:13px}@media(max-width:700px){.columns{grid-template-columns:1fr}}</style>
