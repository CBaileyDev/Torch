import { create } from 'zustand';
import type { RunSummary, EngineEvent, RunConfig, StageSetting } from '../bridge/types';

export type TorchState = 'unlit' | 'lit' | 'spent' | 'guttered';
export type Theme = 'theme-coal' | 'theme-pitch' | 'theme-iron' | 'theme-ember';
export type PlanTier = 'pro' | 'max5x' | 'max20x';
export type Preset = 'standard' | 'classic_linear' | 'fast';

export interface StageStatus {
  torchState: TorchState;
  statusText: string;
  elapsed: number; // ms
  startedAt: number | null;
  model: string;
  effort: StageSetting['effort'];
  transcript: string; // accumulated assistant text
  toolUseCount: number;
  turns: number;
}

export interface CriticStatus {
  a: StageStatus;
  b: StageStatus | null;
}

export interface LoopStatus {
  torchState: TorchState;
  iteration: number;
  maxIterations: number;
  escalateAfter: number;
  lastSummary: string;
  escalatedModel: string | null;
  statusText: string;
  elapsed: number;
  startedAt: number | null;
  model: string;
  effort: StageSetting['effort'];
  transcript: string;
}

export interface RunState {
  id: string;
  goal: string;
  workdir: string;
  preset: Preset;
  config: RunConfig;
  startedAt: number; // Date.now()
  status: RunSummary['status'];
  events: Array<{ runId: string; event: EngineEvent }>;
  // Per-stage
  intake: StageStatus;
  plan: StageStatus;
  critic: CriticStatus;
  implement: StageStatus;
  loop: LoopStatus;
  // Interaction state
  awaitingIntakeAnswers: boolean;
  intakeQuestions: string[];
  awaitingCheckpoint: boolean;
  checkpointNextStage: string;
  // Output / transcript routing
  activeOutputStage: string;
  splitOutput: boolean; // true while critics run in parallel
  // Usage
  totalTurns: number;
  totalOutputTokens: number;
  // Done
  pipelineGreen: boolean | null;
  failureError: string | null;
}

function defaultStage(model: string, effort: StageSetting['effort']): StageStatus {
  return {
    torchState: 'unlit',
    statusText: 'queued',
    elapsed: 0,
    startedAt: null,
    model,
    effort,
    transcript: '',
    toolUseCount: 0,
    turns: 0,
  };
}

function defaultLoop(model: string, effort: StageSetting['effort'], maxIter: number, escalateAfter: number): LoopStatus {
  return {
    torchState: 'unlit',
    iteration: 0,
    maxIterations: maxIter,
    escalateAfter,
    lastSummary: '',
    escalatedModel: null,
    statusText: 'queued',
    elapsed: 0,
    startedAt: null,
    model,
    effort,
    transcript: '',
  };
}

export interface AppSettings {
  theme: Theme;
  planTier: PlanTier;
  preset: Preset;
  heavyMode: boolean;
  ensembleCritic: boolean;
  maxRefineIterations: number;
  escalateAfter: number;
  verifyCommands: string;
  // Per-stage defaults
  stageModels: Record<string, string>;
  stageEfforts: Record<string, StageSetting['effort']>;
}

function defaultSettings(): AppSettings {
  return {
    theme: 'theme-coal',
    planTier: 'max20x',
    preset: 'standard',
    heavyMode: false,
    ensembleCritic: true,
    maxRefineIterations: 5,
    escalateAfter: 1,
    verifyCommands: '',
    stageModels: {
      intake: 'sonnet',
      plan: 'fable',
      critic_a: 'opus',
      critic_b: 'fable',
      merge: 'sonnet',
      implement: 'sonnet',
      refine: 'sonnet',
    },
    stageEfforts: {
      intake: 'low',
      plan: 'max',
      critic_a: 'high',
      critic_b: 'high',
      merge: 'medium',
      implement: 'medium',
      refine: 'medium',
    },
  };
}

export interface AppStore {
  // Runs
  runs: Record<string, RunState>;
  activeRunId: string | null;
  historySummaries: RunSummary[];

  // UI state
  theme: Theme;
  settings: AppSettings;
  availableModels: string[];
  settingsOpen: boolean;

  // Actions
  setTheme(theme: Theme): void;
  setSettings(patch: Partial<AppSettings>): void;
  setAvailableModels(models: string[]): void;
  setSettingsOpen(open: boolean): void;
  setHistorySummaries(summaries: RunSummary[]): void;
  setActiveRun(id: string | null): void;

  createRun(id: string, goal: string, workdir: string, config: RunConfig): void;
  applyEvent(envelope: { runId: string; event: EngineEvent }): void;
  failRun(runId: string, error: string): void;
  tickElapsed(): void;
}

export const useAppStore = create<AppStore>((set, get) => ({
  runs: {},
  activeRunId: null,
  historySummaries: [],
  theme: 'theme-coal',
  settings: defaultSettings(),
  availableModels: ['sonnet', 'opus', 'fable'],
  settingsOpen: false,

  setTheme: (theme) => set({ theme }),
  setSettings: (patch) => set((s) => ({ settings: { ...s.settings, ...patch } })),
  setAvailableModels: (models) => set({ availableModels: models }),
  setSettingsOpen: (open) => set({ settingsOpen: open }),
  setHistorySummaries: (summaries) => set({ historySummaries: summaries }),
  setActiveRun: (id) => set({ activeRunId: id }),

  createRun: (id, goal, workdir, config) => {
    const run: RunState = {
      id,
      goal,
      workdir,
      preset: config.preset,
      config,
      startedAt: Date.now(),
      status: 'running',
      events: [],
      intake: defaultStage(config.intake.model, config.intake.effort),
      plan: defaultStage(config.plan.model, config.plan.effort),
      critic: {
        a: defaultStage(config.critic_a.model, config.critic_a.effort),
        b: config.critic_b ? defaultStage(config.critic_b.model, config.critic_b.effort) : null,
      },
      implement: defaultStage(config.implement.model, config.implement.effort),
      loop: defaultLoop(
        config.refine.model,
        config.refine.effort,
        config.max_refine_iterations,
        config.escalate_after,
      ),
      awaitingIntakeAnswers: false,
      intakeQuestions: [],
      awaitingCheckpoint: false,
      checkpointNextStage: '',
      activeOutputStage: 'intake',
      splitOutput: false,
      totalTurns: 0,
      totalOutputTokens: 0,
      pipelineGreen: null,
      failureError: null,
    };
    set((s) => ({ runs: { ...s.runs, [id]: run }, activeRunId: id }));
  },

  applyEvent: (envelope) => {
    const { runId, event } = envelope;
    set((s) => {
      const run = s.runs[runId];
      if (!run) return s;

      const updated = applyEventToRun(run, event);
      return { runs: { ...s.runs, [runId]: updated } };
    });
    // Also update history summary status for runs list
    const { runs } = get();
    const run = runs[runId];
    if (!run) return;
    set((s) => ({
      historySummaries: s.historySummaries.map((h) =>
        h.id === runId ? { ...h, status: run.status, total_turns: run.totalTurns, total_output_tokens: run.totalOutputTokens } : h,
      ),
    }));
  },

  failRun: (runId, error) => {
    set((s) => {
      const run = s.runs[runId];
      if (!run) return s;
      const updated: RunState = {
        ...run,
        status: 'failed',
        failureError: error,
        awaitingIntakeAnswers: false,
        awaitingCheckpoint: false,
      };
      // Gutter the active stage
      const stages: Array<'intake' | 'plan' | 'implement'> = ['intake', 'plan', 'implement'];
      for (const k of stages) {
        if (updated[k].torchState === 'lit') {
          updated[k] = { ...updated[k], torchState: 'guttered', statusText: 'failed' };
        }
      }
      if (updated.critic.a.torchState === 'lit') {
        updated.critic = { ...updated.critic, a: { ...updated.critic.a, torchState: 'guttered', statusText: 'failed' } };
      }
      if (updated.critic.b?.torchState === 'lit') {
        updated.critic = { ...updated.critic, b: { ...updated.critic.b!, torchState: 'guttered', statusText: 'failed' } };
      }
      if (updated.loop.torchState === 'lit') {
        updated.loop = { ...updated.loop, torchState: 'guttered', statusText: 'failed' };
      }
      return { runs: { ...s.runs, [runId]: updated } };
    });
  },

  tickElapsed: () => {
    const now = Date.now();
    set((s) => {
      const runs = { ...s.runs };
      for (const runId in runs) {
        const run = runs[runId]!;
        if (run.status !== 'running' && run.status !== 'waiting') continue;
        const stages: Array<keyof RunState> = ['intake', 'plan', 'implement'];
        let changed = false;
        const updated = { ...run };
        for (const k of stages) {
          const stage = run[k] as StageStatus;
          if (stage.torchState === 'lit' && stage.startedAt) {
            (updated[k] as StageStatus) = { ...stage, elapsed: now - stage.startedAt };
            changed = true;
          }
        }
        if (run.critic.a.torchState === 'lit' && run.critic.a.startedAt) {
          updated.critic = {
            ...run.critic,
            a: { ...run.critic.a, elapsed: now - run.critic.a.startedAt },
          };
          changed = true;
        }
        if (run.critic.b?.torchState === 'lit' && run.critic.b.startedAt) {
          updated.critic = {
            ...updated.critic,
            b: { ...run.critic.b, elapsed: now - run.critic.b.startedAt },
          };
          changed = true;
        }
        if (run.loop.torchState === 'lit' && run.loop.startedAt) {
          updated.loop = { ...run.loop, elapsed: now - run.loop.startedAt };
          changed = true;
        }
        if (changed) runs[runId] = updated;
      }
      return { runs };
    });
  },
}));

function applyEventToRun(run: RunState, event: EngineEvent): RunState {
  const now = Date.now();
  switch (event.kind) {
    case 'stage_started': {
      const { stage, model } = event;
      const updated = { ...run };
      const startStage = (key: 'intake' | 'plan' | 'implement'): StageStatus => ({
        ...run[key],
        torchState: 'lit',
        statusText: 'running',
        startedAt: now,
        model,
      });

      if (stage === 'intake') {
        updated.intake = startStage('intake');
        updated.activeOutputStage = 'intake';
        updated.splitOutput = false;
      } else if (stage === 'plan') {
        updated.plan = startStage('plan');
        updated.activeOutputStage = 'plan';
        updated.splitOutput = false;
      } else if (stage === 'critic-a') {
        updated.critic = {
          ...run.critic,
          a: { ...run.critic.a, torchState: 'lit', statusText: 'running', startedAt: now, model },
        };
        updated.activeOutputStage = 'critic';
        updated.splitOutput = run.critic.b !== null;
      } else if (stage === 'critic-b') {
        updated.critic = {
          ...run.critic,
          b: run.critic.b
            ? { ...run.critic.b, torchState: 'lit', statusText: 'running', startedAt: now, model }
            : null,
        };
        updated.splitOutput = true;
      } else if (stage === 'merge') {
        // Merge runs in critic card context — mark critics done if not already
        updated.splitOutput = false;
        updated.activeOutputStage = 'merge';
      } else if (stage === 'implement') {
        updated.implement = startStage('implement');
        updated.activeOutputStage = 'implement';
        updated.splitOutput = false;
      } else if (stage === 'refine') {
        updated.loop = { ...run.loop, torchState: 'lit', statusText: 'running', startedAt: now, model };
        updated.activeOutputStage = 'loop';
        updated.splitOutput = false;
      }
      return updated;
    }

    case 'stream': {
      const { stage, event: streamEv } = event;
      const updated = { ...run };
      if (streamEv.kind === 'assistant_text') {
        const appendText = (s: StageStatus): StageStatus => ({ ...s, transcript: s.transcript + streamEv.text });
        if (stage === 'intake') updated.intake = appendText(run.intake);
        else if (stage === 'plan') updated.plan = appendText(run.plan);
        else if (stage === 'critic-a') updated.critic = { ...run.critic, a: appendText(run.critic.a) };
        else if (stage === 'critic-b' && run.critic.b) updated.critic = { ...run.critic, b: appendText(run.critic.b) };
        else if (stage === 'merge') updated.critic = { ...run.critic, a: appendText(run.critic.a) };
        else if (stage === 'implement') updated.implement = appendText(run.implement);
        else if (stage === 'refine') updated.loop = { ...run.loop, transcript: run.loop.transcript + streamEv.text };
      } else if (streamEv.kind === 'assistant_tool_use') {
        if (stage === 'implement') {
          updated.implement = { ...run.implement, toolUseCount: run.implement.toolUseCount + 1 };
        }
      }
      return updated;
    }

    case 'stage_completed': {
      const { stage, result } = event;
      const updated = { ...run };
      updated.totalTurns += result.num_turns;
      updated.totalOutputTokens += result.usage.output_tokens;

      const finishStage = (s: StageStatus): StageStatus => ({
        ...s,
        torchState: result.is_error ? 'guttered' : 'spent',
        statusText: result.is_error ? 'failed' : 'done',
        elapsed: s.startedAt ? now - s.startedAt : s.elapsed,
        turns: result.num_turns,
      });

      if (stage === 'intake') updated.intake = finishStage(run.intake);
      else if (stage === 'plan') updated.plan = finishStage(run.plan);
      else if (stage === 'critic-a') updated.critic = { ...run.critic, a: finishStage(run.critic.a) };
      else if (stage === 'critic-b' && run.critic.b) updated.critic = { ...run.critic, b: finishStage(run.critic.b) };
      else if (stage === 'merge') {
        // Mark critic a as done if not already (for single critic)
        if (run.critic.a.torchState === 'lit') {
          updated.critic = { ...run.critic, a: finishStage(run.critic.a) };
        }
      } else if (stage === 'implement') updated.implement = finishStage(run.implement);
      else if (stage === 'refine') updated.loop = { ...run.loop, ...finishStage(run.loop as unknown as StageStatus) } as LoopStatus;

      return updated;
    }

    case 'awaiting_intake_answers':
      return {
        ...run,
        awaitingIntakeAnswers: true,
        intakeQuestions: event.questions,
        status: 'waiting',
        intake: { ...run.intake, statusText: 'waiting for you' },
      };

    case 'awaiting_checkpoint':
      return {
        ...run,
        awaitingCheckpoint: true,
        checkpointNextStage: event.next_stage,
        status: 'waiting',
      };

    case 'verify_finished': {
      const updated = { ...run };
      updated.loop = {
        ...run.loop,
        iteration: event.iteration,
        lastSummary: event.summary,
        statusText: event.green ? 'done' : `iter ${event.iteration}`,
      };
      return updated;
    }

    case 'refine_escalated':
      return {
        ...run,
        loop: {
          ...run.loop,
          escalatedModel: event.model,
          statusText: `escalated → ${event.model}`,
        },
      };

    case 'pipeline_finished': {
      const greenStatus = event.green ? 'green' : 'not_green';
      const updated = { ...run, status: greenStatus as RunState['status'], pipelineGreen: event.green };
      // Finalize loop if still lit
      if (updated.loop.torchState === 'lit') {
        updated.loop = {
          ...updated.loop,
          torchState: event.green ? 'spent' : 'guttered',
          statusText: event.green ? 'done' : 'not green',
          elapsed: updated.loop.startedAt ? now - updated.loop.startedAt : updated.loop.elapsed,
        };
      }
      return updated;
    }

    default:
      return run;
  }
}
