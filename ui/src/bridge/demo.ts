import type {
  Bridge,
  RunConfig,
  RunSummary,
  EngineEvent,
} from './types';

// Scripted fake engine for browser preview
// Simulates a believable standard-preset run over ~25 seconds

type EngineEventCb = (envelope: { runId: string; event: EngineEvent }) => void;
type RunFailedCb = (payload: { runId: string; error: string }) => void;

const engineListeners: EngineEventCb[] = [];
const runFailedListeners: RunFailedCb[] = [];

function emit(runId: string, event: EngineEvent) {
  engineListeners.forEach((cb) => cb({ runId, event }));
}

let resolveIntakeAnswers: (() => void) | null = null;
let resolveCheckpoint: (() => void) | null = null;
let activeRunId: string | null = null;

const history: RunSummary[] = [
  {
    id: 'demo-run-001',
    goal: 'ProjectPacker XML schema v2',
    workdir: '~/dev/projectpacker',
    preset: 'standard',
    created_at: Math.floor(Date.now() / 1000) - 172800,
    status: 'green',
    refine_iterations: 4,
    total_turns: 38,
    total_output_tokens: 142000,
  },
  {
    id: 'demo-run-002',
    goal: 'Obsidian vault sync CLI',
    workdir: '~/dev/obsidian-sync',
    preset: 'standard',
    created_at: Math.floor(Date.now() / 1000) - 345600,
    status: 'green',
    refine_iterations: 2,
    total_turns: 22,
    total_output_tokens: 89000,
  },
  {
    id: 'demo-run-003',
    goal: 'SQLite blackboard migration',
    workdir: '~/dev/blackboard',
    preset: 'classic_linear',
    created_at: Math.floor(Date.now() / 1000) - 604800,
    status: 'failed',
    refine_iterations: 0,
    total_turns: 11,
    total_output_tokens: 41000,
  },
];

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

async function streamText(runId: string, stage: string, text: string, chunkMs = 60) {
  const sessionId = `${stage}-session-${runId}`;
  emit(runId, { kind: 'stream', stage, event: { kind: 'init', session_id: sessionId, model: 'sonnet' } });
  const words = text.split(' ');
  let buf = '';
  for (const word of words) {
    buf += (buf ? ' ' : '') + word;
    if (buf.length > 12 || word === words[words.length - 1]) {
      await sleep(chunkMs);
      emit(runId, {
        kind: 'stream',
        stage,
        // trailing space: the store concatenates chunks verbatim
        event: { kind: 'assistant_text', session_id: sessionId, text: buf + ' ' },
      });
      buf = '';
    }
  }
}

async function runScriptedDemo(runId: string, config: RunConfig) {
  activeRunId = runId;

  // --- INTAKE ---
  emit(runId, { kind: 'stage_started', stage: 'intake', model: config.intake.model });
  await sleep(800);
  emit(runId, {
    kind: 'awaiting_intake_answers',
    questions: [
      'What is the primary target platform? (web, native desktop, CLI, mobile)',
      'Should the implementation include automated tests?',
      'Are there any existing dependencies or frameworks the project must use?',
      'What is the preferred language version or runtime constraint?',
    ],
  });

  // Wait for user to submit answers
  await new Promise<void>((resolve) => {
    resolveIntakeAnswers = resolve;
  });
  resolveIntakeAnswers = null;

  await streamText(
    runId,
    'intake',
    'Confirmed: native desktop application targeting macOS and Linux. Tests required. No hard framework constraints. Latest stable toolchain preferred.',
    55,
  );
  await sleep(400);
  emit(runId, {
    kind: 'stage_completed',
    stage: 'intake',
    result: {
      subtype: 'end_turn',
      is_error: false,
      session_id: `intake-session-${runId}`,
      num_turns: 2,
      duration_ms: 3200,
      result: 'intake complete',
      usage: { input_tokens: 800, output_tokens: 240, cache_read_input_tokens: 0, cache_creation_input_tokens: 0 },
    },
  });

  // --- PLANNER ---
  await sleep(300);
  emit(runId, { kind: 'stage_started', stage: 'plan', model: config.plan.model });
  const planText =
    '# Implementation Plan\n\n## 1. Project structure\nModular crate layout with a workspace Cargo.toml. Core logic separated from UI bindings.\n\n## 2. Core modules\n- `core/` — domain types and pure business logic\n- `cli/` — command-line interface layer\n- `app/` — Tauri application shell\n\n## 3. Key milestones\n1. Scaffold workspace and CI pipeline\n2. Implement core data model with serde\n3. CLI commands for import/export\n4. Integration tests against fixtures\n5. Package for macOS and Linux release';
  await streamText(runId, 'plan', planText, 40);
  await sleep(300);
  emit(runId, {
    kind: 'stage_completed',
    stage: 'plan',
    result: {
      subtype: 'end_turn',
      is_error: false,
      session_id: `plan-session-${runId}`,
      num_turns: 3,
      duration_ms: 7100,
      result: 'plan complete',
      usage: { input_tokens: 1200, output_tokens: 680, cache_read_input_tokens: 0, cache_creation_input_tokens: 0 },
    },
  });

  // --- CRITICS (parallel) ---
  await sleep(300);
  emit(runId, { kind: 'stage_started', stage: 'critic-a', model: config.critic_a.model });
  if (config.critic_b) {
    emit(runId, { kind: 'stage_started', stage: 'critic-b', model: config.critic_b.model });
  }

  const criticAText =
    'F1. The plan should move core data-model implementation before CLI scaffolding — dependent work blocked otherwise.\nF2. Missing error-propagation strategy across crate boundaries; recommend thiserror + anyhow hierarchy.\nF3. CI matrix should pin toolchain version to avoid nightly regressions.';
  const criticBText =
    'F4. Integration test fixtures should be generated, not committed as binary blobs.\nF5. The macOS packaging step omits codesigning — will fail notarization.\nF6. No mention of structured logging; recommend tracing crate from the start.';

  await Promise.all([
    streamText(runId, 'critic-a', criticAText, 50),
    config.critic_b ? streamText(runId, 'critic-b', criticBText, 60) : Promise.resolve(),
  ]);

  await sleep(300);
  emit(runId, {
    kind: 'stage_completed',
    stage: 'critic-a',
    result: {
      subtype: 'end_turn', is_error: false, session_id: `critic-a-session-${runId}`,
      num_turns: 2, duration_ms: 5400, result: null,
      usage: { input_tokens: 900, output_tokens: 310, cache_read_input_tokens: 0, cache_creation_input_tokens: 0 },
    },
  });
  if (config.critic_b) {
    emit(runId, {
      kind: 'stage_completed',
      stage: 'critic-b',
      result: {
        subtype: 'end_turn', is_error: false, session_id: `critic-b-session-${runId}`,
        num_turns: 2, duration_ms: 5800, result: null,
        usage: { input_tokens: 920, output_tokens: 295, cache_read_input_tokens: 0, cache_creation_input_tokens: 0 },
      },
    });
  }

  // --- MERGE ---
  emit(runId, { kind: 'stage_started', stage: 'merge', model: config.merge.model });
  await streamText(runId, 'merge', 'Merging 6 findings into revised spec. Reordering milestones: data model first, then CLI, then packaging with codesigning. Adding tracing crate to dependencies. Fixture generation added to test plan.', 45);
  await sleep(300);
  emit(runId, {
    kind: 'stage_completed',
    stage: 'merge',
    result: {
      subtype: 'end_turn', is_error: false, session_id: `merge-session-${runId}`,
      num_turns: 1, duration_ms: 2200, result: 'merge complete',
      usage: { input_tokens: 1100, output_tokens: 180, cache_read_input_tokens: 0, cache_creation_input_tokens: 0 },
    },
  });

  // --- CHECKPOINT ---
  if (config.checkpoint_before_implement) {
    emit(runId, { kind: 'awaiting_checkpoint', next_stage: 'implement' });
    await new Promise<void>((resolve) => {
      resolveCheckpoint = resolve;
    });
    resolveCheckpoint = null;
  }

  // --- IMPLEMENT ---
  await sleep(300);
  emit(runId, { kind: 'stage_started', stage: 'implement', model: config.implement.model });
  const implSessionId = `implement-session-${runId}`;

  const toolUseItems = [
    { tool: 'write_file', path: 'Cargo.toml' },
    { tool: 'write_file', path: 'crates/core/src/lib.rs' },
    { tool: 'write_file', path: 'crates/core/src/model.rs' },
    { tool: 'write_file', path: 'crates/cli/src/main.rs' },
    { tool: 'write_file', path: 'crates/cli/src/commands.rs' },
    { tool: 'write_file', path: 'tests/integration_test.rs' },
    { tool: 'run_command', path: 'cargo fmt --all' },
  ];

  for (const item of toolUseItems) {
    await sleep(600);
    emit(runId, {
      kind: 'stream',
      stage: 'implement',
      event: {
        kind: 'assistant_tool_use',
        session_id: implSessionId,
        tool_name: item.tool,
        input: { path: item.path },
      },
    });
    await sleep(400);
    emit(runId, {
      kind: 'stream',
      stage: 'implement',
      event: { kind: 'tool_result', session_id: implSessionId },
    });
  }

  await sleep(300);
  emit(runId, {
    kind: 'stage_completed',
    stage: 'implement',
    result: {
      subtype: 'end_turn', is_error: false, session_id: implSessionId,
      num_turns: 8, duration_ms: 12400, result: 'implement complete',
      usage: { input_tokens: 2800, output_tokens: 4200, cache_read_input_tokens: 0, cache_creation_input_tokens: 0 },
    },
  });

  // --- VERIFY / REFINE loop ---
  emit(runId, { kind: 'stage_started', stage: 'refine', model: config.refine.model });

  // Iteration 1: two failures
  await sleep(1200);
  emit(runId, {
    kind: 'verify_finished',
    iteration: 1,
    green: false,
    summary: 'cargo test: 2 failed · 47 passed\ncargo clippy: 1 warning\ncargo build: ok',
  });

  // Refine pass 1
  await streamText(runId, 'refine',
    'Fixing boundary condition in chunk_mesh.rs and updating test fixture generator to produce deterministic output.', 50);
  await sleep(800);

  // Iteration 2: one failure, triggers escalation
  emit(runId, { kind: 'verify_finished', iteration: 2, green: false, summary: 'cargo test: 1 failed · 48 passed\ncargo clippy: 0 warnings' });
  emit(runId, { kind: 'refine_escalated', iteration: 2, model: config.escalation_model });
  await streamText(runId, 'refine', 'Escalating to fable for final fix on integration test fixture path resolution on Linux.', 50);
  await sleep(800);

  // Iteration 3: green
  emit(runId, { kind: 'verify_finished', iteration: 3, green: true, summary: 'cargo test: 0 failed · 49 passed\ncargo clippy: 0 warnings\ncargo build: ok' });
  await sleep(300);
  emit(runId, {
    kind: 'stage_completed',
    stage: 'refine',
    result: {
      subtype: 'end_turn', is_error: false, session_id: `refine-session-${runId}`,
      num_turns: 6, duration_ms: 9100, result: 'all checks green',
      usage: { input_tokens: 1800, output_tokens: 1100, cache_read_input_tokens: 0, cache_creation_input_tokens: 0 },
    },
  });

  // --- DONE ---
  await sleep(300);
  emit(runId, { kind: 'pipeline_finished', green: true, refine_iterations: 2 });
  activeRunId = null;
}

const savedSettings: Record<string, string> = {
  theme: 'theme-pitch',
  plan_tier: 'max20x',
  default_preset: 'standard',
  heavy_mode: 'false',
  ensemble_critic: 'true',
  max_refine_iterations: '5',
  escalate_after: '1',
};

const savedTemplates: Record<string, string> = {
  'rust-cli': '# {{goal}}\n\nBuild a robust Rust CLI with clap, thiserror, and tracing.\n',
  'web-api': '# {{goal}}\n\nBuild a REST API using axum with JSON payloads and structured error handling.\n',
};

export const demoBridge: Bridge = {
  async startRun(config: RunConfig): Promise<string> {
    const runId = `run-${Date.now()}`;
    // Start async without blocking
    void runScriptedDemo(runId, config);
    return runId;
  },

  async sendIntakeAnswers(_runId: string, _answers: string[]): Promise<void> {
    if (resolveIntakeAnswers) resolveIntakeAnswers();
  },

  async checkpointDecision(_runId: string, approved: boolean): Promise<void> {
    if (!approved && activeRunId) {
      runFailedListeners.forEach((cb) =>
        cb({ runId: activeRunId!, error: 'Checkpoint rejected by user' }),
      );
      activeRunId = null;
      return;
    }
    if (resolveCheckpoint) resolveCheckpoint();
  },

  async cancelRun(runId: string): Promise<void> {
    runFailedListeners.forEach((cb) => cb({ runId, error: 'Cancelled by user' }));
    activeRunId = null;
    resolveIntakeAnswers = null;
    resolveCheckpoint = null;
  },

  async listRuns(): Promise<RunSummary[]> {
    return [...history];
  },

  async getRunEvents(_runId: string): Promise<{ runId: string; event: EngineEvent }[]> {
    // Return an empty history for demo — the real app stores events in the store
    return [];
  },

  async getSettings(): Promise<Record<string, string>> {
    return { ...savedSettings };
  },

  async saveSetting(key: string, value: string): Promise<void> {
    savedSettings[key] = value;
  },

  async getTemplates(): Promise<Record<string, string>> {
    return { ...savedTemplates };
  },

  async saveTemplate(name: string, content: string): Promise<void> {
    savedTemplates[name] = content;
  },

  async probeModels(): Promise<string[]> {
    return ['sonnet', 'opus', 'fable'];
  },

  async pickDirectory(): Promise<string | null> {
    // In browser mode return a plausible path
    return '~/dev/my-project';
  },

  onEngineEvent(cb: (envelope: { runId: string; event: EngineEvent }) => void): () => void {
    engineListeners.push(cb);
    return () => {
      const idx = engineListeners.indexOf(cb);
      if (idx !== -1) engineListeners.splice(idx, 1);
    };
  },

  onRunFailed(cb: (payload: { runId: string; error: string }) => void): () => void {
    runFailedListeners.push(cb);
    return () => {
      const idx = runFailedListeners.indexOf(cb);
      if (idx !== -1) runFailedListeners.splice(idx, 1);
    };
  },
};
