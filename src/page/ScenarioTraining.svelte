<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import { ArrowLeft, FlaskConical, ShieldCheck } from "lucide-svelte";
  import PageShell from "../lib/components/PageShell.svelte";
  import TrainingFeedbackPanel from "../lib/components/training/TrainingFeedback.svelte";
  import TrainingQuestionPanel from "../lib/components/training/TrainingQuestionPanel.svelte";
  import TrainingRolePicker from "../lib/components/training/TrainingRolePicker.svelte";
  import TrainingSetup from "../lib/components/training/TrainingSetup.svelte";
  import TrainingSummaryPanel from "../lib/components/training/TrainingSummary.svelte";
  import TrainingExperiencePicker from "../lib/components/training/TrainingExperiencePicker.svelte";
  import HumanMachineSetup from "../lib/components/training/HumanMachineSetup.svelte";
  import HumanMachineTraining from "../lib/components/training/HumanMachineTraining.svelte";
  import {
    createEmptyTrainingState,
    type TrainingFeedback,
    type TrainingMode,
    type TrainingModuleKey,
    type TrainingQuestion,
    type TrainingRole,
    type TrainingSession,
    type TrainingStep,
    type TrainingSummary,
    type HumanMachineScenario,
    type HumanMachineTurnCount,
  } from "../lib/scenarioTraining";
  import {
    abandonTrainingSession,
    activateTrainingFixtureForTesting,
    canActivateTrainingFixture,
    completeTrainingSession,
    isExplicitTrainingFixtureActive,
    listTrainingRoles,
    nextTrainingQuestion,
    startTrainingSession,
    submitTrainingAnswer,
    startHumanMachineScenario,
    submitHumanMachineTurn,
    abandonHumanMachineScenario,
    getActiveHumanMachineScenario,
  } from "../lib/trainingApi";

  type PageStep = TrainingStep | "human_setup" | "human_active" | "human_result";

  let roles: TrainingRole[] = [];
  let loadingRoles = true;
  let busy = false;
  let error = "";
  let liveMessage = "";
  let step: PageStep = "roles";
  let selectedRole: TrainingRole | null = null;
  let selectedRoleId: string | null = null;
  let mode: TrainingMode = "comprehensive";
  let selectedModule: TrainingModuleKey | null = null;
  let session: TrainingSession | null = null;
  let question: TrainingQuestion | null = null;
  let answer = "";
  let feedback: TrainingFeedback | null = null;
  let summary: TrainingSummary | null = null;
  let transitioningRole: TrainingRole | null = null;
  let roleTransitionTimer: ReturnType<typeof setTimeout> | null = null;
  let humanScenario: HumanMachineScenario | null = null;
  let humanAnswer = "";
  let humanTotalTurns: HumanMachineTurnCount = 3;
  let requestGeneration = 0;
  let pageRoot: HTMLElement;
  let trainingFixtureActive = isExplicitTrainingFixtureActive();

  $: isLastQuestion = Boolean(
    session && session.currentIndex + 1 >= session.totalQuestions,
  );
  $: hasTrainableRole = roles.some((role) => role.availableCaseCount > 0);
  $: showTrainingFixturePrompt = canActivateTrainingFixture()
    && !trainingFixtureActive
    && !loadingRoles
    && !hasTrainableRole;

  function messageForError(value: unknown): string {
    if (value && typeof value === "object" && "message" in value) {
      return String((value as { message?: unknown }).message || "训练服务暂不可用。");
    }
    const raw = String(value || "训练服务暂不可用。");
    return raw.replace(/^Error:\s*/i, "");
  }

  function prefersReducedMotion(): boolean {
    return typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
  }

  async function focusStep(testId: string): Promise<void> {
    await tick();
    const target = pageRoot?.querySelector<HTMLElement>(`[data-testid="${testId}"] h2, [data-testid="${testId}"][tabindex="-1"]`);
    target?.focus({ preventScroll: false });
  }

  async function loadRoles(): Promise<void> {
    loadingRoles = true;
    error = "";
    try {
      roles = await listTrainingRoles();
      trainingFixtureActive = isExplicitTrainingFixtureActive();
      liveMessage = roles.length > 0 ? `已读取 ${roles.length} 位训练角色。` : "暂无训练角色。";
    } catch (value) {
      roles = [];
      error = isExplicitTrainingFixtureActive()
        ? messageForError(value)
        : `${messageForError(value)} 未启用测试夹具，系统不会静默使用虚假证据。`;
      liveMessage = error;
    } finally {
      loadingRoles = false;
    }
  }

  async function loadTrainingFixture(): Promise<void> {
    if (loadingRoles) return;
    error = "";
    if (!activateTrainingFixtureForTesting()) {
      error = "当前构建不允许加载自动测试夹具。";
      liveMessage = error;
      return;
    }
    trainingFixtureActive = true;
    liveMessage = "正在加载自动测试夹具；内容只用于流程测试，不是主播真实话术。";
    await loadRoles();
  }

  function selectRole(role: TrainingRole): void {
    requestGeneration += 1;
    if (roleTransitionTimer) clearTimeout(roleTransitionTimer);
    selectedRole = role;
    selectedRoleId = role.id;
    mode = "comprehensive";
    selectedModule = null;
    error = "";
    // Selection semantics update immediately. The timer below only removes a
    // non-interactive visual echo and never controls business state.
    step = role.availableCaseCount > 0 ? "experience" : "setup";
    liveMessage = role.availableCaseCount > 0
      ? `当前训练主播：${role.displayName}。请选择证据答题或人机情景训练。`
      : `当前训练主播：${role.displayName}。暂无经审核训练证据。`;
    void focusStep("current-training-role");
    if (!prefersReducedMotion()) {
      transitioningRole = role;
      roleTransitionTimer = setTimeout(() => {
        transitioningRole = null;
        roleTransitionTimer = null;
      }, 240);
    } else {
      transitioningRole = null;
    }
  }

  async function selectExperience(experience: "evidence_quiz" | "human_machine"): Promise<void> {
    if (!selectedRole || busy) return;
    error = "";
    if (experience === "evidence_quiz") {
      step = "setup";
      liveMessage = "已选择证据答题训练，请选择综合训练或专项训练。";
      return;
    }
    const generation = requestGeneration;
    const roleId = selectedRole.id;
    busy = true;
    try {
      const active = await getActiveHumanMachineScenario({ roleId });
      if (generation !== requestGeneration || selectedRoleId !== roleId) return;
      if (active) {
        humanScenario = active;
        humanTotalTurns = active.totalTurns;
        selectedModule = active.module;
        humanAnswer = "";
        step = "human_active";
        liveMessage = `已恢复 ${active.displayName} 的第 ${active.currentTurn} 轮人机训练。`;
        void focusStep("human-machine-training");
      } else {
        selectedModule = selectedRole.availableModules[0] || null;
        step = "human_setup";
        liveMessage = "已选择人机情景训练试点，请选择一个有真实案例的板块。";
        void focusStep("human-machine-setup");
      }
    } catch (value) {
      if (generation === requestGeneration) {
        error = messageForError(value);
        liveMessage = error;
      }
    } finally {
      if (generation === requestGeneration) busy = false;
    }
  }

  async function startHumanMachine(): Promise<void> {
    if (!selectedRole || !selectedModule || busy) return;
    const generation = requestGeneration;
    const roleId = selectedRole.id;
    busy = true;
    error = "";
    try {
      const result = await startHumanMachineScenario({ roleId, selectedModule, seed: 20260831, totalTurns: humanTotalTurns });
      if (generation !== requestGeneration || selectedRoleId !== roleId) {
        await abandonHumanMachineScenario({ runId: result.runId }).catch(() => undefined);
        return;
      }
      humanScenario = result;
      humanAnswer = "";
      step = "human_active";
      liveMessage = "第1轮真实评论已加载。";
      void focusStep("human-machine-training");
    } catch (value) {
      if (generation === requestGeneration) {
        error = messageForError(value);
        liveMessage = error;
      }
    } finally {
      if (generation === requestGeneration) busy = false;
    }
  }

  async function submitHumanTurn(): Promise<void> {
    if (!humanScenario || !humanAnswer.trim() || busy) return;
    const generation = requestGeneration;
    const runId = humanScenario.runId;
    busy = true;
    error = "";
    try {
      const result = await submitHumanMachineTurn({ runId, traineeAnswer: humanAnswer.trim() });
      if (generation !== requestGeneration || humanScenario?.runId !== runId) return;
      humanScenario = result;
      humanAnswer = "";
      step = result.status === "completed" ? "human_result" : "human_active";
      liveMessage = result.status === "completed"
        ? `${result.totalTurns}轮人机训练已完成，结果和真实证据已解锁。`
        : `第 ${result.currentTurn} 轮 AI 模拟追问已加载。`;
      void focusStep(result.status === "completed" ? "human-machine-result" : "human-machine-training");
    } catch (value) {
      if (generation === requestGeneration) {
        error = messageForError(value);
        liveMessage = error;
      }
    } finally {
      if (generation === requestGeneration) busy = false;
    }
  }

  function selectMode(nextMode: TrainingMode): void {
    if (busy) return;
    mode = nextMode;
    if (nextMode === "comprehensive") selectedModule = null;
    error = "";
  }

  function selectModule(module: TrainingModuleKey): void {
    if (busy) return;
    selectedModule = module;
    error = "";
  }

  async function start(): Promise<void> {
    if (!selectedRole || selectedRole.availableCaseCount <= 0 || busy) return;
    if (mode === "specialized" && !selectedModule) {
      error = "请先选择一个专项训练板块。";
      return;
    }
    const generation = requestGeneration;
    const roleId = selectedRole.id;
    const roleDisplayName = selectedRole.displayName;
    const requestedMode = mode;
    const requestedModule = selectedModule;
    busy = true;
    error = "";
    try {
      const result = await startTrainingSession({
        roleId,
        mode: requestedMode,
        selectedModule: requestedMode === "specialized" ? requestedModule || undefined : undefined,
        seed: 20260828,
      });
      if (generation !== requestGeneration || selectedRoleId !== roleId) {
        if (result.sessionId) {
          try {
            await abandonTrainingSession({ sessionId: result.sessionId });
          } catch {
            // A stale session must never restore UI, even if cleanup is already complete remotely.
          }
        }
        return;
      }
      session = {
        sessionId: result.sessionId,
        roleId: result.roleId,
        displayName: result.displayName || roleDisplayName,
        mode: result.mode,
        selectedModule: result.selectedModule,
        currentIndex: result.currentIndex,
        totalQuestions: result.totalQuestions,
      };
      question = result.question;
      answer = "";
      feedback = null;
      summary = null;
      step = "question";
      liveMessage = `第 ${result.currentIndex + 1} 题已加载。`;
      void focusStep("training-question");
    } catch (value) {
      if (generation === requestGeneration && selectedRoleId === roleId) {
        error = messageForError(value);
        liveMessage = error;
      }
    } finally {
      if (generation === requestGeneration) busy = false;
    }
  }

  async function submitAnswer(): Promise<void> {
    if (!session || !question || !answer.trim() || busy) return;
    const generation = requestGeneration;
    const sessionId = session.sessionId;
    const roleId = session.roleId;
    busy = true;
    error = "";
    try {
      const result = await submitTrainingAnswer({
        sessionId,
        traineeAnswer: answer.trim(),
      });
      if (
        generation !== requestGeneration ||
        selectedRoleId !== roleId ||
        session?.sessionId !== sessionId
      ) {
        return;
      }
      feedback = result;
      step = "feedback";
      liveMessage = feedback.scoresValid
        ? "本题七维评分和真实证据已解锁。"
        : "本题评分待复核，真实证据已解锁。";
      void focusStep("training-feedback");
    } catch (value) {
      if (generation === requestGeneration && selectedRoleId === roleId) {
        error = messageForError(value);
        liveMessage = error;
      }
    } finally {
      if (generation === requestGeneration) busy = false;
    }
  }

  async function finishSession(generation = requestGeneration, sessionId = session?.sessionId): Promise<void> {
    if (!sessionId) return;
    const roleId = session?.roleId || selectedRoleId;
    const result = await completeTrainingSession({ sessionId });
    if (
      generation !== requestGeneration ||
      selectedRoleId !== roleId ||
      session?.sessionId !== sessionId
    ) {
      return;
    }
    summary = {
      ...result,
      displayName: result.displayName || selectedRole?.displayName || "待确认主播",
    };
    step = "summary";
    liveMessage = "本轮训练已完成，训练总结已生成。";
    void focusStep("training-summary");
  }

  async function continueTraining(): Promise<void> {
    if (!session || busy) return;
    const generation = requestGeneration;
    const sessionId = session.sessionId;
    const roleId = session.roleId;
    busy = true;
    error = "";
    try {
      if (isLastQuestion) {
        await finishSession(generation, sessionId);
        return;
      }
      const result = await nextTrainingQuestion({ sessionId });
      if (
        generation !== requestGeneration ||
        selectedRoleId !== roleId ||
        session?.sessionId !== sessionId
      ) {
        return;
      }
      if (result.isComplete || !result.question) {
        await finishSession(generation, sessionId);
        return;
      }
      session = {
        sessionId: result.sessionId || session.sessionId,
        roleId: result.roleId || session.roleId,
        displayName: result.displayName || session.displayName,
        mode: result.mode,
        selectedModule: result.selectedModule,
        currentIndex: result.currentIndex,
        totalQuestions: result.totalQuestions,
      };
      question = result.question;
      answer = "";
      feedback = null;
      step = "question";
      liveMessage = `第 ${result.currentIndex + 1} 题已加载。`;
      void focusStep("training-question");
    } catch (value) {
      if (generation === requestGeneration && selectedRoleId === roleId) {
        error = messageForError(value);
        liveMessage = error;
      }
    } finally {
      if (generation === requestGeneration) busy = false;
    }
  }

  async function switchRole(): Promise<void> {
    requestGeneration += 1;
    if (roleTransitionTimer) clearTimeout(roleTransitionTimer);
    transitioningRole = null;
    roleTransitionTimer = null;
    const activeSessionId = session?.sessionId;
    const activeHumanRunId = humanScenario?.status === "active" ? humanScenario.runId : null;
    const shouldAbandonActiveSession = Boolean(activeSessionId && step !== "summary");
    const empty = createEmptyTrainingState();
    step = empty.step;
    selectedRole = null;
    selectedRoleId = empty.selectedRoleId;
    mode = empty.mode;
    selectedModule = empty.selectedModule;
    session = null;
    question = null;
    answer = empty.answer;
    feedback = null;
    summary = null;
    humanScenario = null;
    humanAnswer = "";
    humanTotalTurns = 3;
    busy = false;
    error = "";
    liveMessage = "已清空上一位主播的题目、回答、得分、板块和证据。请重新选择主播。";
    await tick();
    pageRoot?.focus({ preventScroll: false });
    if (activeSessionId && shouldAbandonActiveSession) {
      try {
        await abandonTrainingSession({ sessionId: activeSessionId });
      } catch {
        // Reset the local view even when an already-expired session cannot be abandoned.
      }
    }
    if (activeHumanRunId) {
      await abandonHumanMachineScenario({ runId: activeHumanRunId }).catch(() => undefined);
    }
  }

  function restartSameRole(): void {
    requestGeneration += 1;
    session = null;
    question = null;
    answer = "";
    feedback = null;
    summary = null;
    busy = false;
    error = "";
    mode = "comprehensive";
    selectedModule = null;
    humanScenario = null;
    humanAnswer = "";
    humanTotalTurns = 3;
    step = "experience";
    liveMessage = `请为 ${selectedRole?.displayName || "当前主播"} 选择下一轮训练形式。`;
  }

  onMount(loadRoles);
  onDestroy(() => {
    requestGeneration += 1;
    if (roleTransitionTimer) clearTimeout(roleTransitionTimer);
    if (session?.sessionId && step !== "summary") {
      void abandonTrainingSession({ sessionId: session.sessionId }).catch(() => undefined);
    }
    if (humanScenario?.runId && humanScenario.status === "active") {
      void abandonHumanMachineScenario({ runId: humanScenario.runId }).catch(() => undefined);
    }
  });
</script>

<PageShell
  title="头牌主播情景训练"
  subtitle="从真实评论、主播真实回答和可播放片段中学习现场应答。"
  paddedBottom={true}
>
  <div slot="actions">
    {#if selectedRole}
      <button type="button" class="mac-btn switch-button" data-testid="switch-role" on:click={switchRole}>
        <ArrowLeft size={15} aria-hidden="true" />切换主播
      </button>
    {/if}
  </div>

  <main
    class="scenario-training"
    data-testid="scenario-training"
    bind:this={pageRoot}
    tabindex="-1"
    aria-busy={busy || loadingRoles}
  >
    <div class="safety-note" class:test-mode={trainingFixtureActive} data-testid="training-data-mode">
      {#if trainingFixtureActive}
        <FlaskConical size={16} aria-hidden="true" />
        <span><strong>测试数据模式：</strong>题目、回答和片段只用于验证训练流程，不是主播真实话术。</span>
      {:else}
        <ShieldCheck size={16} aria-hidden="true" />
        <span>只使用已审核证据；AI建议与主播真实原话分开展示；资料不足时不补造商品事实。</span>
      {/if}
    </div>

    {#if step === "roles"}
      {#if showTrainingFixturePrompt}
        <section class="fixture-callout" data-testid="training-empty-test-action" aria-labelledby="fixture-callout-title">
          <FlaskConical size={22} aria-hidden="true" />
          <div>
            <strong id="fixture-callout-title">正式证据库暂时为空</strong>
            <p>先加载明确标注的自动测试场景，验证角色选择、答题、评分、证据和总结闭环。测试内容不会写入正式知识库。</p>
          </div>
          <button type="button" class="mac-btn primary" on:click={loadTrainingFixture}>
            加载测试场景
          </button>
        </section>
      {/if}
      <TrainingRolePicker
        {roles}
        {selectedRoleId}
        loading={loadingRoles}
        on:select={(event) => selectRole(event.detail)}
        on:retry={loadRoles}
      />
      {#if error}<p class="page-error" role="alert">{error}</p>{/if}
    {:else if step === "experience" && selectedRole}
      <TrainingExperiencePicker role={selectedRole} on:select={(event) => selectExperience(event.detail)} />
      {#if error}<p class="page-error" role="alert">{error}</p>{/if}
    {:else if step === "setup" && selectedRole}
      <TrainingSetup
        role={selectedRole}
        {mode}
        {selectedModule}
        {busy}
        {error}
        on:mode={(event) => selectMode(event.detail)}
        on:module={(event) => selectModule(event.detail)}
        on:start={start}
      />
    {:else if step === "human_setup" && selectedRole}
      <HumanMachineSetup
        role={selectedRole}
        {selectedModule}
        totalTurns={humanTotalTurns}
        {busy}
        {error}
        on:module={(event) => selectModule(event.detail)}
        on:turns={(event) => humanTotalTurns = event.detail}
        on:start={startHumanMachine}
      />
    {:else if (step === "human_active" || step === "human_result") && humanScenario}
      <HumanMachineTraining
        scenario={humanScenario}
        bind:answer={humanAnswer}
        {busy}
        {error}
        on:submit={submitHumanTurn}
        on:restart={restartSameRole}
      />
    {:else if step === "question" && session && question}
      <TrainingQuestionPanel
        {session}
        {question}
        bind:answer
        {busy}
        {error}
        on:submit={submitAnswer}
      />
    {:else if step === "feedback" && feedback && question}
      <TrainingFeedbackPanel
        {feedback}
        {question}
        {busy}
        {error}
        isLast={isLastQuestion}
        on:next={continueTraining}
      />
    {:else if step === "summary" && summary}
      <TrainingSummaryPanel {summary} on:restart={restartSameRole} />
    {/if}

    {#if transitioningRole}
      <div class="role-transition-layer" aria-hidden="true" inert>
        <TrainingRolePicker
          {roles}
          selectedRoleId={transitioningRole.id}
          loading={false}
        />
      </div>
    {/if}

    <p class="sr-only" aria-live="polite" aria-atomic="true">{liveMessage}</p>
  </main>
</PageShell>

<style>
  .scenario-training { position: relative; display: grid; gap: 16px; outline: none; }
  .scenario-training:focus-visible { box-shadow: 0 0 0 3px var(--mac-blue-soft); border-radius: var(--mac-radius-sm); }
  .safety-note { display: flex; align-items: center; gap: 7px; min-height: 36px; padding: 7px 11px; border: 1px solid color-mix(in srgb, var(--mac-green) 25%, var(--mac-separator)); border-radius: 10px; background: color-mix(in srgb, var(--mac-green) 6%, var(--mac-bg-card)); color: var(--mac-green-solid); }
  .safety-note.test-mode { border-color: color-mix(in srgb, var(--mac-orange) 34%, var(--mac-separator)); background: color-mix(in srgb, var(--mac-orange) 8%, var(--mac-bg-card)); color: var(--mac-orange); }
  .safety-note span { color: var(--mac-secondary); font-size: 11px; line-height: 1.4; }
  .safety-note strong { color: var(--mac-label); }
  .fixture-callout {
    display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: 12px;
    min-height: 88px; padding: 16px; border: 1px solid color-mix(in srgb, var(--mac-orange) 36%, var(--mac-separator));
    border-radius: var(--mac-radius-lg); background: color-mix(in srgb, var(--mac-orange) 7%, var(--mac-bg-card)); color: var(--mac-orange);
  }
  .fixture-callout strong { display: block; color: var(--mac-label); font-size: 14px; }
  .fixture-callout p { margin: 4px 0 0; color: var(--mac-secondary); font-size: 12px; line-height: 1.5; }
  .fixture-callout .mac-btn { min-height: 44px; white-space: nowrap; }
  .switch-button { min-height: 40px; }
  .page-error { margin: 0; color: var(--mac-red); font-size: 12px; }
  .role-transition-layer {
    position: absolute;
    inset: 52px 0 auto;
    z-index: 4;
    min-height: calc(100% - 52px);
    padding-bottom: 16px;
    pointer-events: none;
    background: color-mix(in srgb, var(--mac-bg-card) 94%, var(--mac-bg));
    transform-origin: 50% 10%;
    animation: role-selection-settle 240ms cubic-bezier(.22, .8, .3, 1) forwards;
  }
  @keyframes role-selection-settle {
    from { opacity: 1; transform: translateY(0) scale(1); }
    to { opacity: 0; transform: translateY(-10px) scale(.99); }
  }
  .sr-only { position: absolute; width: 1px; height: 1px; padding: 0; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0; }
  @media (max-width: 720px) {
    .fixture-callout { grid-template-columns: auto minmax(0, 1fr); }
    .fixture-callout .mac-btn { grid-column: 1 / -1; width: 100%; }
  }
  @media (prefers-reduced-motion: reduce) {
    .role-transition-layer { display: none; animation: none; }
    .scenario-training, .scenario-training :global(*) { scroll-behavior: auto !important; animation-duration: .001ms !important; animation-iteration-count: 1 !important; transition-duration: .001ms !important; }
  }
</style>
