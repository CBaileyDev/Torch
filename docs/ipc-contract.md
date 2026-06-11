# Torch IPC contract — UI ⇄ Tauri shell

The React frontend talks to the Rust shell exclusively through this contract.
In the browser (no `window.__TAURI_INTERNALS__`), the UI runs a scripted
demo-mode bridge implementing the same interface.

## Events (Rust → UI)

One Tauri event channel: `engine-event`, payload:

```ts
{ runId: string, event: EngineEvent }
```

`EngineEvent` is a tagged union on `kind` (snake_case):

```ts
type EngineEvent =
  | { kind: "stage_started";  stage: string; model: string }
  | { kind: "stream";         stage: string; event: StreamEvent }
  | { kind: "stage_completed"; stage: string; result: RunResult }
  | { kind: "awaiting_intake_answers"; questions: string[] }
  | { kind: "awaiting_checkpoint";     next_stage: string }
  | { kind: "verify_finished"; iteration: number; green: boolean; summary: string }
  | { kind: "refine_escalated"; iteration: number; model: string }
  | { kind: "pipeline_finished"; green: boolean; refine_iterations: number };

type StreamEvent =
  | { kind: "init";  session_id: string; model: string }
  | { kind: "assistant_text"; session_id: string; text: string }
  | { kind: "assistant_tool_use"; session_id: string; tool_name: string; input: unknown }
  | { kind: "tool_result"; session_id: string }
  | { kind: "result"; /* RunResult fields inline */ }
  | { kind: "other"; event_type: string };

type RunResult = {
  subtype: string; is_error: boolean; session_id: string;
  num_turns: number; duration_ms: number; result: string | null;
  usage: { input_tokens: number; output_tokens: number;
           cache_read_input_tokens: number; cache_creation_input_tokens: number };
};
```

Stage names on the wire: `intake`, `plan`, `critic-a`, `critic-b`, `merge`,
`implement`, `refine`, plus `architect`, `planner`, `drafter`, `reviser`
for the Classic Linear preset.

A run that fails (stage error, checkpoint rejection, cancellation) emits a
shell-level event `run-failed` with payload `{ runId: string, error: string }`.

## Commands (UI → Rust, via `invoke`)

```ts
start_run(config: RunConfig): Promise<string /* runId */>
send_intake_answers(runId: string, answers: string[]): Promise<void>
checkpoint_decision(runId: string, approved: boolean): Promise<void>
cancel_run(runId: string): Promise<void>
list_runs(): Promise<RunSummary[]>
get_run_events(runId: string): Promise<{ runId: string, event: EngineEvent }[]>
get_settings(): Promise<Record<string, string>>
save_setting(key: string, value: string): Promise<void>
get_templates(): Promise<Record<string, string>>
save_template(name: string, content: string): Promise<void>
probe_models(): Promise<string[]>   // e.g. ["sonnet","opus","fable"]
pick_directory(): Promise<string | null>
```

```ts
type StageSetting = { model: string; effort: "low"|"medium"|"high"|"xhigh"|"max" };

type RunConfig = {
  goal: string;
  workdir: string;
  preset: "standard" | "classic_linear" | "fast";
  intake: StageSetting;        // default sonnet/low
  plan: StageSetting;          // default fable/max
  critic_a: StageSetting;      // default opus/high
  critic_b: StageSetting | null; // null = single-critic mode
  merge: StageSetting;         // default sonnet/medium
  implement: StageSetting;     // default sonnet/medium; Heavy Mode → opus
  refine: StageSetting;        // default sonnet/medium
  escalation_model: string;    // default "fable"
  max_refine_iterations: number; // 3 (Pro/5x) or 5 (20x)
  escalate_after: number;        // 2 (Pro/5x) or 1 (20x)
  checkpoint_before_implement: boolean; // default true
  verify_commands: string[];   // empty = intake decides
  // templates and binary are optional; omit from the UI for defaults
};

type RunSummary = {
  id: string; goal: string; workdir: string; preset: string;
  created_at: number;          // unix seconds
  status: "running" | "waiting" | "green" | "not_green" | "failed" | "cancelled";
  refine_iterations: number;
  total_turns: number; total_output_tokens: number;
};
```

## Plan-tier defaults (applied by the UI when the user picks a tier)

| Tier   | critics            | loop cap | escalate after | heavy mode hint |
|--------|--------------------|----------|----------------|-----------------|
| pro    | single (critic_b = null) | 3  | 2              | discouraged     |
| max5x  | single             | 3        | 2              | allowed         |
| max20x | ensemble           | 5        | 1              | allowed         |
