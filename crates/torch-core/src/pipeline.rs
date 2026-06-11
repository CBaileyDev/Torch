//! The engine pipeline: event vocabulary, the deterministic verifier, run
//! artifacts, and the Fast preset (Plan → Implement → Verify⇄Refine) used by
//! the headless CLI. The richer presets live in [`crate::orchestrator`] and
//! share the building blocks defined here.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Sender;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::claude::{run_invocation, CancelToken, ClaudeError, Invocation};
use crate::stream::{RunResult, StreamEvent};
use crate::templates::{render, Templates};

/// Engine → front-end events. Tagged on `kind` (snake_case) per
/// `docs/ipc-contract.md`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EngineEvent {
    StageStarted {
        stage: String,
        model: String,
    },
    Stream {
        stage: String,
        event: StreamEvent,
    },
    StageCompleted {
        stage: String,
        result: RunResult,
    },
    AwaitingIntakeAnswers {
        questions: Vec<String>,
    },
    AwaitingCheckpoint {
        next_stage: String,
    },
    VerifyFinished {
        iteration: u32,
        green: bool,
        summary: String,
    },
    RefineEscalated {
        iteration: u32,
        model: String,
    },
    PipelineFinished {
        green: bool,
        refine_iterations: u32,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("run cancelled")]
    Cancelled,
    #[error("checkpoint rejected")]
    CheckpointRejected,
    #[error("stage {stage} failed: {message}")]
    StageFailed { stage: String, message: String },
    #[error(transparent)]
    Claude(ClaudeError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<ClaudeError> for PipelineError {
    fn from(error: ClaudeError) -> Self {
        match error {
            ClaudeError::Cancelled => PipelineError::Cancelled,
            other => PipelineError::Claude(other),
        }
    }
}

/// Configuration for the headless Fast pipeline.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub goal: String,
    pub workdir: PathBuf,
    pub verify_commands: Vec<String>,
    pub max_refine_iterations: u32,
    pub escalate_after: u32,
    pub plan_model: String,
    pub implement_model: String,
    pub refine_model: String,
    pub escalation_model: String,
    pub binary: PathBuf,
}

/// What a finished (or given-up) pipeline hands back to the front-end.
#[derive(Debug)]
pub struct PipelineOutcome {
    pub green: bool,
    pub refine_iterations: u32,
    pub stage_results: Vec<(String, RunResult)>,
    /// `torch/run-<timestamp>` inside the working directory.
    pub run_dir: Option<PathBuf>,
    /// The last verify report when the loop could not go green.
    pub final_report: Option<VerifyReport>,
}

// ---------------------------------------------------------------------------
// Deterministic verifier — the orchestrator itself runs the commands; this
// costs zero tokens and is the only thing allowed to declare success.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct VerifyResult {
    pub command: String,
    pub exit_code: Option<i32>,
    pub green: bool,
    /// Combined stdout+stderr, tail-truncated.
    pub output: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyReport {
    pub iteration: u32,
    pub green: bool,
    pub results: Vec<VerifyResult>,
}

impl VerifyReport {
    /// One line per command, e.g. `cargo test: FAILED (exit 101)`.
    pub fn summary(&self) -> String {
        self.results
            .iter()
            .map(|r| {
                if r.green {
                    format!("{}: ok", r.command)
                } else {
                    match r.exit_code {
                        Some(code) => format!("{}: FAILED (exit {code})", r.command),
                        None => format!("{}: FAILED (terminated)", r.command),
                    }
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Failing commands with up to `max_lines` of output each — what the
    /// refiner gets fed.
    pub fn failure_summary(&self, max_lines: usize) -> String {
        self.results
            .iter()
            .filter(|r| !r.green)
            .map(|r| {
                let tail: Vec<&str> = r.output.lines().collect();
                let start = tail.len().saturating_sub(max_lines);
                format!(
                    "$ {}\nexit: {}\n{}",
                    r.command,
                    r.exit_code
                        .map_or_else(|| "terminated".to_string(), |c| c.to_string()),
                    tail[start..].join("\n")
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Stable identity of "the same failure" across iterations: which
    /// commands failed and how they exited.
    pub fn fingerprint(&self) -> String {
        self.results
            .iter()
            .filter(|r| !r.green)
            .map(|r| format!("{}#{:?}", r.command, r.exit_code))
            .collect::<Vec<_>>()
            .join("|")
    }
}

const VERIFY_OUTPUT_TAIL_BYTES: usize = 64 * 1024;

/// Run every verify command in `workdir`. An empty command list is green by
/// definition (the caller decides whether that is acceptable).
pub fn run_verify(commands: &[String], workdir: &Path, iteration: u32) -> VerifyReport {
    let results: Vec<VerifyResult> = commands
        .iter()
        .map(|command| {
            let output = shell_command(command).current_dir(workdir).output();
            match output {
                Ok(output) => {
                    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
                    combined.push_str(&String::from_utf8_lossy(&output.stderr));
                    if combined.len() > VERIFY_OUTPUT_TAIL_BYTES {
                        // keep the tail — failures print last; advance to a
                        // char boundary so multi-byte output can't panic
                        let mut cut = combined.len() - VERIFY_OUTPUT_TAIL_BYTES;
                        while !combined.is_char_boundary(cut) {
                            cut += 1;
                        }
                        combined = combined[cut..].to_string();
                    }
                    VerifyResult {
                        command: command.clone(),
                        exit_code: output.status.code(),
                        green: output.status.success(),
                        output: combined,
                    }
                }
                Err(error) => VerifyResult {
                    command: command.clone(),
                    exit_code: None,
                    green: false,
                    output: format!("failed to run: {error}"),
                },
            }
        })
        .collect();
    VerifyReport {
        iteration,
        green: results.iter().all(|r| r.green),
        results,
    }
}

fn shell_command(command: &str) -> Command {
    if cfg!(windows) {
        let mut cmd = Command::new("cmd");
        cmd.arg("/C").arg(command);
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command);
        cmd
    }
}

// ---------------------------------------------------------------------------
// Run artifacts — output survives independent of the app.
// ---------------------------------------------------------------------------

/// `<workdir>/torch/run-<timestamp>/` holding `artifact.md` and per-iteration
/// verify logs.
#[derive(Debug)]
pub(crate) struct RunArtifacts {
    pub dir: PathBuf,
}

impl RunArtifacts {
    pub fn create(workdir: &Path) -> std::io::Result<Self> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let base = workdir.join("torch");
        let mut dir = base.join(format!("run-{stamp}"));
        let mut n = 1;
        while dir.exists() {
            dir = base.join(format!("run-{stamp}-{n}"));
            n += 1;
        }
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    pub fn append_section(&self, heading: &str, body: &str) -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join("artifact.md"))?;
        writeln!(file, "## {heading}\n\n{}\n", body.trim_end())
    }

    pub fn write_verify_log(&self, report: &VerifyReport) -> std::io::Result<()> {
        let mut log = format!(
            "# verify iteration {}\n\n{}\n",
            report.iteration,
            report.summary()
        );
        for result in &report.results {
            log.push_str(&format!("\n$ {}\n{}\n", result.command, result.output));
        }
        fs::write(
            self.dir.join(format!("verify-{}.log", report.iteration)),
            log,
        )
    }
}

// ---------------------------------------------------------------------------
// Stage execution shared by both presets.
// ---------------------------------------------------------------------------

pub(crate) struct StageCtx<'a> {
    pub binary: &'a Path,
    pub workdir: &'a Path,
    pub tx: &'a Sender<EngineEvent>,
    pub cancel: &'a CancelToken,
}

pub(crate) fn emit(tx: &Sender<EngineEvent>, event: EngineEvent) {
    // A dropped receiver must not kill the run.
    let _ = tx.send(event);
}

/// One claude invocation, forwarding stream events tagged with `stage`.
/// Returns the terminal result and the accumulated assistant text.
pub(crate) fn stream_invocation(
    ctx: &StageCtx,
    stage: &str,
    model: &str,
    prompt: &str,
    resume_session: Option<&str>,
) -> Result<(RunResult, String), PipelineError> {
    let mut transcript = String::new();
    let result = run_invocation(
        &Invocation {
            binary: ctx.binary,
            workdir: ctx.workdir,
            model,
            prompt,
            resume_session,
            cancel: ctx.cancel,
        },
        |event| {
            if let StreamEvent::AssistantText { text, .. } = &event {
                transcript.push_str(text);
            }
            emit(
                ctx.tx,
                EngineEvent::Stream {
                    stage: stage.to_string(),
                    event,
                },
            );
        },
    )?;
    Ok((result, transcript))
}

/// A full stage: `stage_started`, the invocation, `stage_completed`.
pub(crate) fn run_stage(
    ctx: &StageCtx,
    stage: &str,
    model: &str,
    prompt: &str,
    resume_session: Option<&str>,
) -> Result<(RunResult, String), PipelineError> {
    emit(
        ctx.tx,
        EngineEvent::StageStarted {
            stage: stage.to_string(),
            model: model.to_string(),
        },
    );
    let (result, transcript) = stream_invocation(ctx, stage, model, prompt, resume_session)?;
    emit(
        ctx.tx,
        EngineEvent::StageCompleted {
            stage: stage.to_string(),
            result: result.clone(),
        },
    );
    if result.is_error {
        return Err(PipelineError::StageFailed {
            stage: stage.to_string(),
            message: result
                .result
                .clone()
                .unwrap_or_else(|| result.subtype.clone()),
        });
    }
    Ok((result, transcript))
}

// ---------------------------------------------------------------------------
// The Verify ⇄ Refine loop, shared by every preset.
// ---------------------------------------------------------------------------

pub(crate) struct LoopParams<'a> {
    pub goal: &'a str,
    pub verify_commands: &'a [String],
    pub max_refine_iterations: u32,
    pub escalate_after: u32,
    pub refine_model: &'a str,
    pub escalation_model: &'a str,
    pub refine_effort: &'a str,
    pub refine_template: &'a str,
    /// Session of the implementer, resumed by every refine pass.
    pub implement_session: String,
}

pub(crate) struct LoopOutcome {
    pub green: bool,
    pub refine_iterations: u32,
    pub final_report: Option<VerifyReport>,
    pub stage_results: Vec<(String, RunResult)>,
}

pub(crate) fn run_refine_loop(
    ctx: &StageCtx,
    params: &LoopParams,
    artifacts: &RunArtifacts,
) -> Result<LoopOutcome, PipelineError> {
    emit(
        ctx.tx,
        EngineEvent::StageStarted {
            stage: "refine".to_string(),
            model: params.refine_model.to_string(),
        },
    );

    let mut session = params.implement_session.clone();
    let mut refine_iterations: u32 = 0;
    let mut stage_results = Vec::new();
    let mut last_fingerprint: Option<String> = None;
    let mut consecutive_same: u32 = 0;
    let mut last_result: Option<RunResult> = None;
    let mut iteration: u32 = 0;

    loop {
        if ctx.cancel.is_cancelled() {
            return Err(PipelineError::Cancelled);
        }
        iteration += 1;
        let report = run_verify(params.verify_commands, ctx.workdir, iteration);
        let _ = artifacts.write_verify_log(&report);
        emit(
            ctx.tx,
            EngineEvent::VerifyFinished {
                iteration,
                green: report.green,
                summary: report.summary(),
            },
        );

        if report.green {
            finish_refine_stage(ctx, &mut stage_results, last_result.take(), &session);
            return Ok(LoopOutcome {
                green: true,
                refine_iterations,
                final_report: None,
                stage_results,
            });
        }

        if refine_iterations >= params.max_refine_iterations {
            finish_refine_stage(ctx, &mut stage_results, last_result.take(), &session);
            return Ok(LoopOutcome {
                green: false,
                refine_iterations,
                final_report: Some(report),
                stage_results,
            });
        }

        // The same failure surviving consecutive iterations escalates the
        // refiner to the configured frontier model.
        let fingerprint = report.fingerprint();
        if last_fingerprint.as_deref() == Some(fingerprint.as_str()) {
            consecutive_same += 1;
        } else {
            consecutive_same = 0;
        }
        last_fingerprint = Some(fingerprint);

        let model = if consecutive_same >= params.escalate_after {
            emit(
                ctx.tx,
                EngineEvent::RefineEscalated {
                    iteration,
                    model: params.escalation_model.to_string(),
                },
            );
            params.escalation_model
        } else {
            params.refine_model
        };

        refine_iterations += 1;
        let prompt = render(
            params.refine_template,
            &[
                ("goal", params.goal),
                ("failures", &report.failure_summary(40)),
                ("effort", params.refine_effort),
            ],
        );
        let (result, _) = stream_invocation(ctx, "refine", model, &prompt, Some(&session))?;
        if !result.session_id.is_empty() {
            session = result.session_id.clone();
        }
        last_result = Some(result);
    }
}

/// The loop reports one `refine` stage to the UI: completed with the last
/// refine result (or a synthetic zero-cost result if no refine pass ran).
fn finish_refine_stage(
    ctx: &StageCtx,
    stage_results: &mut Vec<(String, RunResult)>,
    last_result: Option<RunResult>,
    session: &str,
) {
    let result = last_result.unwrap_or_else(|| RunResult {
        subtype: "verified".to_string(),
        is_error: false,
        session_id: session.to_string(),
        num_turns: 0,
        duration_ms: 0,
        result: None,
        usage: Default::default(),
    });
    emit(
        ctx.tx,
        EngineEvent::StageCompleted {
            stage: "refine".to_string(),
            result: result.clone(),
        },
    );
    stage_results.push(("refine".to_string(), result));
}

// ---------------------------------------------------------------------------
// The Fast preset (headless CLI entry point).
// ---------------------------------------------------------------------------

/// Plan → Implement → Verify⇄Refine on one goal.
pub fn run_pipeline(
    config: &PipelineConfig,
    tx: Sender<EngineEvent>,
    cancel: CancelToken,
) -> Result<PipelineOutcome, PipelineError> {
    let templates = Templates::default();
    let artifacts = RunArtifacts::create(&config.workdir)?;
    let ctx = StageCtx {
        binary: &config.binary,
        workdir: &config.workdir,
        tx: &tx,
        cancel: &cancel,
    };
    let mut stage_results = Vec::new();

    artifacts.append_section("Goal", &config.goal)?;

    // Plan — fresh session.
    let plan_prompt = render(
        &templates.plan,
        &[
            ("goal", config.goal.as_str()),
            ("artifact", "(fast preset: no intake brief)"),
            ("effort", "max"),
        ],
    );
    let (plan_result, plan_text) = run_stage(&ctx, "plan", &config.plan_model, &plan_prompt, None)?;
    artifacts.append_section("Plan", &plan_text)?;
    stage_results.push(("plan".to_string(), plan_result));

    // Implement — fresh session, writes real files into the workdir.
    let implement_prompt = render(
        &templates.implement,
        &[
            ("goal", config.goal.as_str()),
            ("artifact", &plan_text),
            ("effort", "medium"),
        ],
    );
    let (implement_result, _) = run_stage(
        &ctx,
        "implement",
        &config.implement_model,
        &implement_prompt,
        None,
    )?;
    let implement_session = implement_result.session_id.clone();
    stage_results.push(("implement".to_string(), implement_result));

    // Verify ⇄ Refine.
    let loop_outcome = run_refine_loop(
        &ctx,
        &LoopParams {
            goal: &config.goal,
            verify_commands: &config.verify_commands,
            max_refine_iterations: config.max_refine_iterations,
            escalate_after: config.escalate_after,
            refine_model: &config.refine_model,
            escalation_model: &config.escalation_model,
            refine_effort: "medium",
            refine_template: &templates.refine,
            implement_session,
        },
        &artifacts,
    )?;
    stage_results.extend(loop_outcome.stage_results);

    emit(
        &tx,
        EngineEvent::PipelineFinished {
            green: loop_outcome.green,
            refine_iterations: loop_outcome.refine_iterations,
        },
    );

    Ok(PipelineOutcome {
        green: loop_outcome.green,
        refine_iterations: loop_outcome.refine_iterations,
        stage_results,
        run_dir: Some(artifacts.dir),
        final_report: loop_outcome.final_report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_reports_green_and_red() {
        let dir = tempfile::tempdir().unwrap();
        let report = run_verify(&["echo ok".to_string()], dir.path(), 1);
        assert!(report.green);
        assert!(report.summary().contains("echo ok: ok"));

        let report = run_verify(
            &["echo ok".to_string(), "exit 3".to_string()],
            dir.path(),
            2,
        );
        assert!(!report.green);
        assert!(report.summary().contains("exit 3: FAILED (exit 3)"));
        assert!(report.failure_summary(10).contains("$ exit 3"));
        assert!(!report.fingerprint().is_empty());
    }

    #[test]
    fn empty_verify_is_green() {
        let dir = tempfile::tempdir().unwrap();
        assert!(run_verify(&[], dir.path(), 1).green);
    }

    #[test]
    fn engine_event_serializes_to_contract_shape() {
        let json = serde_json::to_value(EngineEvent::VerifyFinished {
            iteration: 2,
            green: false,
            summary: "cargo test: FAILED (exit 101)".into(),
        })
        .unwrap();
        assert_eq!(json["kind"], "verify_finished");
        assert_eq!(json["iteration"], 2);

        let json = serde_json::to_value(EngineEvent::AwaitingCheckpoint {
            next_stage: "implement".into(),
        })
        .unwrap();
        assert_eq!(json["kind"], "awaiting_checkpoint");
        assert_eq!(json["next_stage"], "implement");
    }

    #[test]
    fn run_artifacts_write_sections_and_logs() {
        let dir = tempfile::tempdir().unwrap();
        let artifacts = RunArtifacts::create(dir.path()).unwrap();
        artifacts.append_section("Goal", "build a thing").unwrap();
        artifacts.append_section("Plan", "the plan").unwrap();
        let report = run_verify(&["echo ok".to_string()], dir.path(), 1);
        artifacts.write_verify_log(&report).unwrap();

        let artifact = fs::read_to_string(artifacts.dir.join("artifact.md")).unwrap();
        assert!(artifact.contains("## Goal"));
        assert!(artifact.contains("build a thing"));
        assert!(artifact.contains("## Plan"));
        assert!(artifacts.dir.join("verify-1.log").is_file());
        assert!(artifacts.dir.starts_with(dir.path().join("torch")));
    }
}
