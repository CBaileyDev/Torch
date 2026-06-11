//! Headless front-end for the Torch engine spike: runs the
//! Plan → Implement → Verify⇄Refine pipeline on one goal and streams
//! progress to stdout. Proves session threading, model switching, and the
//! refine loop end to end ahead of the Tauri shell.

use std::path::PathBuf;
use std::sync::mpsc;

use anyhow::{bail, Context, Result};
use torch_core::claude::CancelToken;
use torch_core::pipeline::{run_pipeline, EngineEvent, PipelineConfig};
use torch_core::stream::StreamEvent;

const USAGE: &str = "usage: torch --dir <workdir> --verify <command> [--verify <command>…] \
[--plan-model fable] [--implement-model sonnet] [--refine-model sonnet] \
[--escalation-model fable] [--max-iterations 3] [--escalate-after 2] <goal>";

fn parse_args() -> Result<PipelineConfig> {
    let mut args = std::env::args().skip(1);
    let mut goal = None;
    let mut workdir = None;
    let mut verify_commands = Vec::new();
    let mut plan_model = "fable".to_string();
    let mut implement_model = "sonnet".to_string();
    let mut refine_model = "sonnet".to_string();
    let mut escalation_model = "fable".to_string();
    let mut max_refine_iterations = 3;
    let mut escalate_after = 2;

    while let Some(arg) = args.next() {
        let mut value = |name: &str| {
            args.next()
                .with_context(|| format!("{name} requires a value\n{USAGE}"))
        };
        match arg.as_str() {
            "--dir" => workdir = Some(PathBuf::from(value("--dir")?)),
            "--verify" => verify_commands.push(value("--verify")?),
            "--plan-model" => plan_model = value("--plan-model")?,
            "--implement-model" => implement_model = value("--implement-model")?,
            "--refine-model" => refine_model = value("--refine-model")?,
            "--escalation-model" => escalation_model = value("--escalation-model")?,
            "--max-iterations" => max_refine_iterations = value("--max-iterations")?.parse()?,
            "--escalate-after" => escalate_after = value("--escalate-after")?.parse()?,
            "--help" | "-h" => bail!("{USAGE}"),
            _ if goal.is_none() && !arg.starts_with('-') => goal = Some(arg),
            other => bail!("unknown argument {other}\n{USAGE}"),
        }
    }

    let workdir: PathBuf = workdir.context(USAGE)?;
    if !workdir.is_dir() {
        bail!("working directory {} does not exist", workdir.display());
    }
    if verify_commands.is_empty() {
        bail!("at least one --verify command is required\n{USAGE}");
    }
    Ok(PipelineConfig {
        goal: goal.context(USAGE)?,
        workdir,
        verify_commands,
        max_refine_iterations,
        escalate_after,
        plan_model,
        implement_model,
        refine_model,
        escalation_model,
        binary: PathBuf::from("claude"),
    })
}

fn main() -> Result<()> {
    let config = parse_args()?;
    println!("torch · goal: {}", config.goal);
    println!("        dir: {}", config.workdir.display());
    println!("     verify: {}", config.verify_commands.join(" · "));

    let cancel = CancelToken::new();
    let cancel_for_signal = cancel.clone();
    ctrlc::set_handler(move || {
        eprintln!("\ncancelling run…");
        cancel_for_signal.cancel();
    })?;

    let (tx, rx) = mpsc::channel();
    let pipeline = std::thread::spawn(move || run_pipeline(&config, tx, cancel));

    for event in rx {
        match event {
            EngineEvent::StageStarted { stage, .. } => {
                println!("\n━━━ {stage} ━━━");
            }
            EngineEvent::Stream { event, .. } => match event {
                StreamEvent::Init { session_id, model } => {
                    println!("  session {session_id} · model {model}");
                }
                StreamEvent::AssistantText { text, .. } => {
                    for line in text.lines() {
                        println!("  {line}");
                    }
                }
                StreamEvent::AssistantToolUse {
                    tool_name, input, ..
                } => {
                    let detail = input
                        .get("command")
                        .or_else(|| input.get("file_path"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    println!("  ⚒ {tool_name} {detail}");
                }
                _ => {}
            },
            EngineEvent::StageCompleted { stage, result } => {
                println!(
                    "  ✓ {stage} done · {} turns · {:.1}s · {} out-tokens",
                    result.num_turns,
                    result.duration_ms as f64 / 1000.0,
                    result.usage.output_tokens
                );
            }
            EngineEvent::VerifyFinished {
                iteration,
                green,
                summary,
            } => {
                println!("\n━━━ verify (iteration {iteration}) ━━━");
                if green {
                    println!("  all checks green");
                } else {
                    for line in summary.lines() {
                        println!("  {line}");
                    }
                }
            }
            EngineEvent::RefineEscalated { iteration, model } => {
                println!("  ⚠ same failure survived — escalating iteration {iteration} to {model}");
            }
            EngineEvent::PipelineFinished {
                green,
                refine_iterations,
            } => {
                println!("\n━━━ run finished ━━━");
                println!(
                    "  {} after {refine_iterations} refine iteration(s)",
                    if green { "GREEN" } else { "NOT GREEN" }
                );
            }
            // interactive pauses only occur in the GUI's standard preset
            EngineEvent::AwaitingIntakeAnswers { .. } | EngineEvent::AwaitingCheckpoint { .. } => {}
        }
    }

    let outcome = pipeline
        .join()
        .expect("pipeline thread panicked")
        .context("pipeline failed")?;

    let (total_turns, total_out): (u64, u64) = outcome
        .stage_results
        .iter()
        .fold((0, 0), |(turns, out), (_, r)| {
            (turns + r.num_turns, out + r.usage.output_tokens)
        });
    println!("  totals: {total_turns} turns · {total_out} output tokens across all stages");
    println!("  (all stages share your Claude subscription rate limits)");
    if let Some(dir) = &outcome.run_dir {
        println!("  artifact + verify logs: {}", dir.display());
    }
    if !outcome.green {
        if let Some(report) = &outcome.final_report {
            eprintln!("\nremaining failures:\n{}", report.failure_summary(40));
        }
        std::process::exit(1);
    }
    Ok(())
}
