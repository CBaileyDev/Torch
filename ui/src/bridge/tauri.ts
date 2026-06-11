import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  Bridge,
  RunConfig,
  RunSummary,
  EngineEvent,
  EngineEventEnvelope,
} from './types';

export const tauriBridge: Bridge = {
  startRun(config: RunConfig): Promise<string> {
    return invoke<string>('start_run', { config });
  },

  sendIntakeAnswers(runId: string, answers: string[]): Promise<void> {
    return invoke<void>('send_intake_answers', { runId, answers });
  },

  checkpointDecision(runId: string, approved: boolean): Promise<void> {
    return invoke<void>('checkpoint_decision', { runId, approved });
  },

  cancelRun(runId: string): Promise<void> {
    return invoke<void>('cancel_run', { runId });
  },

  listRuns(): Promise<RunSummary[]> {
    return invoke<RunSummary[]>('list_runs');
  },

  getRunEvents(runId: string): Promise<EngineEventEnvelope[]> {
    return invoke<EngineEventEnvelope[]>('get_run_events', { runId });
  },

  getSettings(): Promise<Record<string, string>> {
    return invoke<Record<string, string>>('get_settings');
  },

  saveSetting(key: string, value: string): Promise<void> {
    return invoke<void>('save_setting', { key, value });
  },

  getTemplates(): Promise<Record<string, string>> {
    return invoke<Record<string, string>>('get_templates');
  },

  saveTemplate(name: string, content: string): Promise<void> {
    return invoke<void>('save_template', { name, content });
  },

  probeModels(): Promise<string[]> {
    return invoke<string[]>('probe_models');
  },

  pickDirectory(): Promise<string | null> {
    return invoke<string | null>('pick_directory');
  },

  onEngineEvent(cb: (envelope: { runId: string; event: EngineEvent }) => void): () => void {
    let unlisten: (() => void) | null = null;
    listen<{ runId: string; event: EngineEvent }>('engine-event', (e) => {
      cb(e.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      if (unlisten) unlisten();
    };
  },

  onRunFailed(cb: (payload: { runId: string; error: string }) => void): () => void {
    let unlisten: (() => void) | null = null;
    listen<{ runId: string; error: string }>('run-failed', (e) => {
      cb(e.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      if (unlisten) unlisten();
    };
  },
};
