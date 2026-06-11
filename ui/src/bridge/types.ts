// IPC contract types — verbatim from ipc-contract.md

export type StreamEvent =
  | { kind: 'init'; session_id: string; model: string }
  | { kind: 'assistant_text'; session_id: string; text: string }
  | { kind: 'assistant_tool_use'; session_id: string; tool_name: string; input: unknown }
  | { kind: 'tool_result'; session_id: string }
  | { kind: 'result' }
  | { kind: 'other'; event_type: string };

export type RunResult = {
  subtype: string;
  is_error: boolean;
  session_id: string;
  num_turns: number;
  duration_ms: number;
  result: string | null;
  usage: {
    input_tokens: number;
    output_tokens: number;
    cache_read_input_tokens: number;
    cache_creation_input_tokens: number;
  };
};

export type EngineEvent =
  | { kind: 'stage_started'; stage: string; model: string }
  | { kind: 'stream'; stage: string; event: StreamEvent }
  | { kind: 'stage_completed'; stage: string; result: RunResult }
  | { kind: 'awaiting_intake_answers'; questions: string[] }
  | { kind: 'awaiting_checkpoint'; next_stage: string }
  | { kind: 'verify_finished'; iteration: number; green: boolean; summary: string }
  | { kind: 'refine_escalated'; iteration: number; model: string }
  | { kind: 'pipeline_finished'; green: boolean; refine_iterations: number };

export type EngineEventEnvelope = { runId: string; event: EngineEvent };

export type StageSetting = {
  model: string;
  effort: 'low' | 'medium' | 'high' | 'xhigh' | 'max';
};

export type RunConfig = {
  goal: string;
  workdir: string;
  preset: 'standard' | 'classic_linear' | 'fast';
  intake: StageSetting;
  plan: StageSetting;
  critic_a: StageSetting;
  critic_b: StageSetting | null;
  merge: StageSetting;
  implement: StageSetting;
  refine: StageSetting;
  escalation_model: string;
  max_refine_iterations: number;
  escalate_after: number;
  checkpoint_before_implement: boolean;
  verify_commands: string[];
};

export type RunSummary = {
  id: string;
  goal: string;
  workdir: string;
  preset: string;
  created_at: number;
  status: 'running' | 'waiting' | 'green' | 'not_green' | 'failed' | 'cancelled';
  refine_iterations: number;
  total_turns: number;
  total_output_tokens: number;
};

export interface Bridge {
  startRun(config: RunConfig): Promise<string>;
  sendIntakeAnswers(runId: string, answers: string[]): Promise<void>;
  checkpointDecision(runId: string, approved: boolean): Promise<void>;
  cancelRun(runId: string): Promise<void>;
  listRuns(): Promise<RunSummary[]>;
  getRunEvents(runId: string): Promise<EngineEventEnvelope[]>;
  getSettings(): Promise<Record<string, string>>;
  saveSetting(key: string, value: string): Promise<void>;
  getTemplates(): Promise<Record<string, string>>;
  saveTemplate(name: string, content: string): Promise<void>;
  probeModels(): Promise<string[]>;
  pickDirectory(): Promise<string | null>;
  onEngineEvent(cb: (envelope: { runId: string; event: EngineEvent }) => void): () => void;
  onRunFailed(cb: (payload: { runId: string; error: string }) => void): () => void;
}
