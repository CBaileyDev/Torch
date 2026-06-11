//! The GUI-facing orchestrator: full presets (Standard, Classic Linear,
//! Fast), interactive intake Q&A, parallel critics, and the pre-implement
//! checkpoint. Front-ends send [`ControlMsg`]s to answer the engine's
//! `awaiting_*` events.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::claude::{CancelToken, Provider};
use crate::pipeline::{
    emit, run_refine_loop, run_stage, EngineEvent, LoopParams, PipelineError, PipelineOutcome,
    RunArtifacts, StageCtx,
};
use crate::templates::{render, Templates};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    Standard,
    ClassicLinear,
    Fast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl Effort {
    pub fn as_str(&self) -> &'static str {
        match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::Xhigh => "xhigh",
            Effort::Max => "max",
        }
    }
}

impl std::fmt::Display for Effort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageSetting {
    pub model: String,
    pub effort: Effort,
    /// Reserved for multi-provider configurations; `claude` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

/// A full run request from a front-end, matching `docs/ipc-contract.md`.
/// `binaries` and `templates` are engine-side details the shell fills in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunConfig {
    pub goal: String,
    pub workdir: PathBuf,
    pub preset: Preset,
    pub intake: StageSetting,
    pub plan: StageSetting,
    pub critic_a: StageSetting,
    #[serde(default)]
    pub critic_b: Option<StageSetting>,
    pub merge: StageSetting,
    pub implement: StageSetting,
    pub refine: StageSetting,
    pub escalation_model: String,
    pub max_refine_iterations: u32,
    pub escalate_after: u32,
    pub checkpoint_before_implement: bool,
    #[serde(default)]
    pub verify_commands: Vec<String>,
    #[serde(default)]
    pub binaries: HashMap<String, PathBuf>,
    #[serde(default)]
    pub templates: Templates,
}

impl RunConfig {
    fn binary(&self) -> PathBuf {
        self.binaries
            .get(Provider::Claude.id())
            .cloned()
            .or_else(|| Provider::Claude.resolve_binary())
            .unwrap_or_else(|| PathBuf::from(Provider::Claude.binary()))
    }
}

/// Front-end → engine messages answering `awaiting_*` events.
#[derive(Debug, Clone)]
pub enum ControlMsg {
    IntakeAnswers(Vec<String>),
    CheckpointDecision(bool),
}

/// Pull clarifying questions out of the intake transcript: numbered or
/// bulleted lines ending in `?`, capped at five.
pub fn parse_questions(transcript: &str) -> Vec<String> {
    transcript
        .lines()
        .map(|line| {
            line.trim()
                .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ')')
                .trim_start_matches(['-', '*', '•'])
                .trim()
        })
        .filter(|line| line.ends_with('?') && line.len() > 1)
        .map(str::to_string)
        .take(5)
        .collect()
}

/// Pull `VERIFY: <command>` lines out of the intake brief.
fn parse_verify_commands(brief: &str) -> Vec<String> {
    brief
        .lines()
        .filter_map(|line| line.trim().strip_prefix("VERIFY:"))
        .map(|command| command.trim().to_string())
        .filter(|command| !command.is_empty())
        .collect()
}

fn wait_for<T>(
    control_rx: &Receiver<ControlMsg>,
    cancel: &CancelToken,
    mut select: impl FnMut(ControlMsg) -> Option<T>,
) -> Result<T, PipelineError> {
    loop {
        if cancel.is_cancelled() {
            return Err(PipelineError::Cancelled);
        }
        match control_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(msg) => {
                if let Some(value) = select(msg) {
                    return Ok(value);
                }
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return Err(PipelineError::Cancelled),
        }
    }
}

/// Run a configured preset to completion. Blocks the calling thread; the
/// front-end consumes [`EngineEvent`]s from `event_tx`'s receiver and feeds
/// interactive pauses through `control_rx`.
pub fn run_orchestrated(
    config: &RunConfig,
    event_tx: Sender<EngineEvent>,
    control_rx: Receiver<ControlMsg>,
    cancel: CancelToken,
) -> Result<PipelineOutcome, PipelineError> {
    let binary = config.binary();
    let ctx = StageCtx {
        binary: &binary,
        workdir: &config.workdir,
        tx: &event_tx,
        cancel: &cancel,
    };
    let artifacts = RunArtifacts::create(&config.workdir)?;
    artifacts.append_section("Goal", &config.goal)?;

    let outcome = match config.preset {
        Preset::Standard => run_standard(config, &ctx, &control_rx, &artifacts)?,
        Preset::ClassicLinear => run_classic_linear(config, &ctx, &control_rx, &artifacts)?,
        Preset::Fast => run_fast(config, &ctx, &artifacts)?,
    };

    emit(
        ctx.tx,
        EngineEvent::PipelineFinished {
            green: outcome.green,
            refine_iterations: outcome.refine_iterations,
        },
    );

    Ok(PipelineOutcome {
        run_dir: Some(artifacts.dir.clone()),
        ..outcome
    })
}

struct PresetOutcome {
    green: bool,
    refine_iterations: u32,
    stage_results: Vec<(String, crate::stream::RunResult)>,
    final_report: Option<crate::pipeline::VerifyReport>,
}

impl From<PresetOutcome> for PipelineOutcome {
    fn from(outcome: PresetOutcome) -> Self {
        PipelineOutcome {
            green: outcome.green,
            refine_iterations: outcome.refine_iterations,
            stage_results: outcome.stage_results,
            run_dir: None,
            final_report: outcome.final_report,
        }
    }
}

fn run_standard(
    config: &RunConfig,
    ctx: &StageCtx,
    control_rx: &Receiver<ControlMsg>,
    artifacts: &RunArtifacts,
) -> Result<PipelineOutcome, PipelineError> {
    let templates = &config.templates;
    let mut stage_results = Vec::new();

    // ── 1 · Intake: questions, then the brief. One stage, two invocations;
    // its session becomes the main session every later "resumes main" stage
    // threads through.
    emit(
        ctx.tx,
        EngineEvent::StageStarted {
            stage: "intake".to_string(),
            model: config.intake.model.clone(),
        },
    );
    let questions_prompt = render(
        &templates.intake_questions,
        &[
            ("goal", config.goal.as_str()),
            ("effort", config.intake.effort.as_str()),
        ],
    );
    let (questions_result, questions_text) = crate::pipeline::stream_invocation(
        ctx,
        "intake",
        &config.intake.model,
        &questions_prompt,
        None,
    )?;
    let mut main_session = questions_result.session_id.clone();

    let questions = parse_questions(&questions_text);
    let answers = if questions.is_empty() {
        Vec::new()
    } else {
        emit(
            ctx.tx,
            EngineEvent::AwaitingIntakeAnswers {
                questions: questions.clone(),
            },
        );
        wait_for(control_rx, ctx.cancel, |msg| match msg {
            ControlMsg::IntakeAnswers(answers) => Some(answers),
            _ => None,
        })?
    };

    let answers_block = questions
        .iter()
        .zip(answers.iter())
        .map(|(q, a)| format!("Q: {q}\nA: {a}"))
        .collect::<Vec<_>>()
        .join("\n");
    let brief_prompt = render(
        &templates.intake_brief,
        &[
            ("goal", config.goal.as_str()),
            ("answers", &answers_block),
            ("effort", config.intake.effort.as_str()),
        ],
    );
    let (brief_result, brief) = crate::pipeline::stream_invocation(
        ctx,
        "intake",
        &config.intake.model,
        &brief_prompt,
        Some(&main_session),
    )?;
    if !brief_result.session_id.is_empty() {
        main_session = brief_result.session_id.clone();
    }
    emit(
        ctx.tx,
        EngineEvent::StageCompleted {
            stage: "intake".to_string(),
            result: brief_result.clone(),
        },
    );
    if brief_result.is_error {
        return Err(PipelineError::StageFailed {
            stage: "intake".to_string(),
            message: brief_result.subtype.clone(),
        });
    }
    artifacts.append_section("Brief", &brief)?;
    stage_results.push(("intake".to_string(), brief_result));

    let verify_commands = if config.verify_commands.is_empty() {
        parse_verify_commands(&brief)
    } else {
        config.verify_commands.clone()
    };

    // ── 2 · Plan — resumes main.
    let plan_prompt = render(
        &templates.plan,
        &[
            ("goal", config.goal.as_str()),
            ("artifact", &brief),
            ("effort", config.plan.effort.as_str()),
        ],
    );
    let (plan_result, plan) = run_stage(
        ctx,
        "plan",
        &config.plan.model,
        &plan_prompt,
        Some(&main_session),
    )?;
    if !plan_result.session_id.is_empty() {
        main_session = plan_result.session_id.clone();
    }
    artifacts.append_section("Plan", &plan)?;
    stage_results.push(("plan".to_string(), plan_result));

    // ── 3 · Critics — brand-new sessions so they can't rationalize the
    // planner's mistakes. Ensemble runs in parallel.
    let critic_prompt_a = render(
        &templates.critic,
        &[
            ("goal", config.goal.as_str()),
            ("artifact", &plan),
            ("effort", config.critic_a.effort.as_str()),
        ],
    );
    let critique_b_handle = config.critic_b.as_ref().map(|critic_b| {
        let prompt = render(
            &templates.critic,
            &[
                ("goal", config.goal.as_str()),
                ("artifact", &plan),
                ("effort", critic_b.effort.as_str()),
            ],
        );
        let binary = ctx.binary.to_path_buf();
        let workdir = ctx.workdir.to_path_buf();
        let tx = ctx.tx.clone();
        let cancel = ctx.cancel.clone();
        let model = critic_b.model.clone();
        std::thread::spawn(move || {
            let ctx = StageCtx {
                binary: &binary,
                workdir: &workdir,
                tx: &tx,
                cancel: &cancel,
            };
            run_stage(&ctx, "critic-b", &model, &prompt, None)
        })
    });

    let (critic_a_result, critique_a) = run_stage(
        ctx,
        "critic-a",
        &config.critic_a.model,
        &critic_prompt_a,
        None,
    )?;
    stage_results.push(("critic-a".to_string(), critic_a_result));
    let mut critiques = format!("### critic-a\n{critique_a}");

    if let Some(handle) = critique_b_handle {
        let (critic_b_result, critique_b) =
            handle.join().map_err(|_| PipelineError::StageFailed {
                stage: "critic-b".to_string(),
                message: "critic thread panicked".to_string(),
            })??;
        stage_results.push(("critic-b".to_string(), critic_b_result));
        critiques.push_str(&format!("\n\n### critic-b\n{critique_b}"));
    }
    artifacts.append_section("Critiques", &critiques)?;

    // ── 3b · Merge — resumes main.
    let merge_prompt = render(
        &templates.merge,
        &[
            ("goal", config.goal.as_str()),
            ("artifact", &plan),
            ("critiques", &critiques),
            ("effort", config.merge.effort.as_str()),
        ],
    );
    let (merge_result, spec) = run_stage(
        ctx,
        "merge",
        &config.merge.model,
        &merge_prompt,
        Some(&main_session),
    )?;
    artifacts.append_section("Final spec", &spec)?;
    stage_results.push(("merge".to_string(), merge_result));

    // ── Checkpoint — the user reviews the artifact before any file is
    // written.
    if config.checkpoint_before_implement {
        emit(
            ctx.tx,
            EngineEvent::AwaitingCheckpoint {
                next_stage: "implement".to_string(),
            },
        );
        let approved = wait_for(control_rx, ctx.cancel, |msg| match msg {
            ControlMsg::CheckpointDecision(approved) => Some(approved),
            _ => None,
        })?;
        if !approved {
            return Err(PipelineError::CheckpointRejected);
        }
    }

    // ── 4 · Implement — fresh session, in the workdir.
    let implement_prompt = render(
        &templates.implement,
        &[
            ("goal", config.goal.as_str()),
            ("artifact", &spec),
            ("effort", config.implement.effort.as_str()),
        ],
    );
    let (implement_result, _) = run_stage(
        ctx,
        "implement",
        &config.implement.model,
        &implement_prompt,
        None,
    )?;
    let implement_session = implement_result.session_id.clone();
    stage_results.push(("implement".to_string(), implement_result));

    // ── 5 · Verify ⇄ Refine.
    let loop_outcome = run_refine_loop(
        ctx,
        &LoopParams {
            goal: &config.goal,
            verify_commands: &verify_commands,
            max_refine_iterations: config.max_refine_iterations,
            escalate_after: config.escalate_after,
            refine_model: &config.refine.model,
            escalation_model: &config.escalation_model,
            refine_effort: config.refine.effort.as_str(),
            refine_template: &templates.refine,
            implement_session,
        },
        artifacts,
    )?;
    stage_results.extend(loop_outcome.stage_results);

    Ok(PresetOutcome {
        green: loop_outcome.green,
        refine_iterations: loop_outcome.refine_iterations,
        stage_results,
        final_report: loop_outcome.final_report,
    }
    .into())
}

/// The original 6-stage telephone pipeline, kept so it can be A/B'd against
/// the loop on the same goal.
fn run_classic_linear(
    config: &RunConfig,
    ctx: &StageCtx,
    _control_rx: &Receiver<ControlMsg>,
    artifacts: &RunArtifacts,
) -> Result<PipelineOutcome, PipelineError> {
    let templates = &config.templates;
    let mut stage_results = Vec::new();
    let mut artifact = String::new();

    let stages: [(&str, &StageSetting, &str); 4] = [
        ("architect", &config.plan, &templates.architect),
        ("planner", &config.plan, &templates.plan),
        ("drafter", &config.implement, &templates.drafter),
        ("reviser", &config.refine, &templates.reviser),
    ];
    for (stage, setting, template) in stages {
        let prompt = render(
            template,
            &[
                ("goal", config.goal.as_str()),
                ("artifact", &artifact),
                ("effort", setting.effort.as_str()),
            ],
        );
        let (result, text) = run_stage(ctx, stage, &setting.model, &prompt, None)?;
        artifacts.append_section(stage, &text)?;
        stage_results.push((stage.to_string(), result));
        artifact = text;
    }

    let implement_prompt = render(
        &templates.implement,
        &[
            ("goal", config.goal.as_str()),
            ("artifact", &artifact),
            ("effort", config.implement.effort.as_str()),
        ],
    );
    let (implement_result, _) = run_stage(
        ctx,
        "implement",
        &config.implement.model,
        &implement_prompt,
        None,
    )?;
    let implement_session = implement_result.session_id.clone();
    stage_results.push(("implement".to_string(), implement_result));

    let loop_outcome = run_refine_loop(
        ctx,
        &LoopParams {
            goal: &config.goal,
            verify_commands: &config.verify_commands,
            max_refine_iterations: config.max_refine_iterations,
            escalate_after: config.escalate_after,
            refine_model: &config.refine.model,
            escalation_model: &config.escalation_model,
            refine_effort: config.refine.effort.as_str(),
            refine_template: &templates.refine,
            implement_session,
        },
        artifacts,
    )?;
    stage_results.extend(loop_outcome.stage_results);

    Ok(PresetOutcome {
        green: loop_outcome.green,
        refine_iterations: loop_outcome.refine_iterations,
        stage_results,
        final_report: loop_outcome.final_report,
    }
    .into())
}

/// Plan → Implement → Verify⇄Refine with the orchestrator's config.
fn run_fast(
    config: &RunConfig,
    ctx: &StageCtx,
    artifacts: &RunArtifacts,
) -> Result<PipelineOutcome, PipelineError> {
    let templates = &config.templates;
    let mut stage_results = Vec::new();

    let plan_prompt = render(
        &templates.plan,
        &[
            ("goal", config.goal.as_str()),
            ("artifact", "(fast preset: no intake brief)"),
            ("effort", config.plan.effort.as_str()),
        ],
    );
    let (plan_result, plan) = run_stage(ctx, "plan", &config.plan.model, &plan_prompt, None)?;
    artifacts.append_section("Plan", &plan)?;
    stage_results.push(("plan".to_string(), plan_result));

    let implement_prompt = render(
        &templates.implement,
        &[
            ("goal", config.goal.as_str()),
            ("artifact", &plan),
            ("effort", config.implement.effort.as_str()),
        ],
    );
    let (implement_result, _) = run_stage(
        ctx,
        "implement",
        &config.implement.model,
        &implement_prompt,
        None,
    )?;
    let implement_session = implement_result.session_id.clone();
    stage_results.push(("implement".to_string(), implement_result));

    let loop_outcome = run_refine_loop(
        ctx,
        &LoopParams {
            goal: &config.goal,
            verify_commands: &config.verify_commands,
            max_refine_iterations: config.max_refine_iterations,
            escalate_after: config.escalate_after,
            refine_model: &config.refine.model,
            escalation_model: &config.escalation_model,
            refine_effort: config.refine.effort.as_str(),
            refine_template: &templates.refine,
            implement_session,
        },
        artifacts,
    )?;
    stage_results.extend(loop_outcome.stage_results);

    Ok(PresetOutcome {
        green: loop_outcome.green,
        refine_iterations: loop_outcome.refine_iterations,
        stage_results,
        final_report: loop_outcome.final_report,
    }
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_numbered_and_bulleted_questions() {
        let text = "Here you go:\n1. What platform?\n2) Are tests required?\n- Which runtime?\nNot a question.";
        assert_eq!(
            parse_questions(text),
            vec![
                "What platform?".to_string(),
                "Are tests required?".to_string(),
                "Which runtime?".to_string(),
            ]
        );
    }

    #[test]
    fn caps_questions_at_five() {
        let text = (1..=8)
            .map(|n| format!("{n}. Question {n}?"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(parse_questions(&text).len(), 5);
    }

    #[test]
    fn parses_verify_lines() {
        let brief =
            "Brief text.\nVERIFY: cargo test\n  VERIFY: cargo clippy -- -D warnings\nVERIFY:\n";
        assert_eq!(
            parse_verify_commands(brief),
            vec![
                "cargo test".to_string(),
                "cargo clippy -- -D warnings".to_string()
            ]
        );
    }

    #[test]
    fn run_config_deserializes_ipc_contract_json() {
        let json = r#"{
            "goal": "Build me a thing",
            "workdir": "/tmp/work",
            "preset": "standard",
            "intake":    { "model": "sonnet", "effort": "low" },
            "plan":      { "model": "fable",  "effort": "max" },
            "critic_a":  { "model": "opus",   "effort": "high" },
            "critic_b":  { "model": "fable",  "effort": "high" },
            "merge":     { "model": "sonnet", "effort": "medium" },
            "implement": { "model": "sonnet", "effort": "medium" },
            "refine":    { "model": "sonnet", "effort": "medium" },
            "escalation_model": "fable",
            "max_refine_iterations": 5,
            "escalate_after": 1,
            "checkpoint_before_implement": true,
            "verify_commands": ["cargo test"]
        }"#;
        let config: RunConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.preset, Preset::Standard);
        assert_eq!(config.plan.effort, Effort::Max);
        assert!(config.critic_b.is_some());
        assert!(config.binaries.is_empty());
        assert_eq!(
            serde_json::to_value(config.preset).unwrap().as_str(),
            Some("standard")
        );
    }

    #[test]
    fn run_config_accepts_null_critic_b() {
        let json = r#"{
            "goal": "g", "workdir": "/tmp", "preset": "fast",
            "intake": { "model": "sonnet", "effort": "low" },
            "plan": { "model": "fable", "effort": "max" },
            "critic_a": { "model": "opus", "effort": "high" },
            "critic_b": null,
            "merge": { "model": "sonnet", "effort": "medium" },
            "implement": { "model": "sonnet", "effort": "medium" },
            "refine": { "model": "sonnet", "effort": "medium" },
            "escalation_model": "fable",
            "max_refine_iterations": 3,
            "escalate_after": 2,
            "checkpoint_before_implement": false,
            "verify_commands": []
        }"#;
        let config: RunConfig = serde_json::from_str(json).unwrap();
        assert!(config.critic_b.is_none());
    }
}
