//! End-to-end engine tests against the stub claude binary. No login, no
//! network: the stub replays canned stream-json. Each test copies the stub
//! into its own temp dir with a `torch-stub.config` sidecar pointing at its
//! replies, so tests stay isolated under the parallel test runner.

use std::path::PathBuf;
use std::sync::mpsc;

use torch_core::claude::CancelToken;
use torch_core::orchestrator::{
    run_orchestrated, ControlMsg, Effort, Preset, RunConfig, StageSetting,
};
use torch_core::pipeline::{run_pipeline, EngineEvent, PipelineConfig};
use torch_core::templates::Templates;

/// Compose one canned claude session: init, a text turn, the result event.
fn reply(session: &str, text: &str) -> String {
    let text_json = serde_json::to_string(text).unwrap();
    format!(
        concat!(
            "{{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"{s}\",\"model\":\"stub\"}}\n",
            "{{\"type\":\"assistant\",\"session_id\":\"{s}\",\"message\":{{\"content\":[{{\"type\":\"text\",\"text\":{t}}}]}}}}\n",
            "{{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"session_id\":\"{s}\",",
            "\"num_turns\":2,\"duration_ms\":50,\"result\":\"done\",",
            "\"usage\":{{\"input_tokens\":100,\"output_tokens\":40,",
            "\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0}}}}\n",
        ),
        s = session,
        t = text_json,
    )
}

/// An isolated stub installation: its own copy of the executable plus the
/// scripted replies it will play back in invocation order.
struct StubScript {
    dir: tempfile::TempDir,
}

impl StubScript {
    fn new(replies: &[String]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        for (i, content) in replies.iter().enumerate() {
            std::fs::write(dir.path().join(format!("reply-{}.jsonl", i + 1)), content).unwrap();
        }
        let exe_name = if cfg!(windows) {
            "claude.exe"
        } else {
            "claude"
        };
        std::fs::copy(
            env!("CARGO_BIN_EXE_torch-stub-claude"),
            dir.path().join(exe_name),
        )
        .unwrap();
        std::fs::write(
            dir.path().join("torch-stub.config"),
            dir.path().display().to_string(),
        )
        .unwrap();
        Self { dir }
    }

    fn binary(&self) -> PathBuf {
        let exe_name = if cfg!(windows) {
            "claude.exe"
        } else {
            "claude"
        };
        self.dir.path().join(exe_name)
    }
}

fn setting(model: &str, effort: Effort) -> StageSetting {
    StageSetting {
        model: model.to_string(),
        effort,
        provider: None,
    }
}

fn kinds(events: &[EngineEvent]) -> Vec<String> {
    events
        .iter()
        .map(|e| {
            serde_json::to_value(e).unwrap()["kind"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect()
}

#[test]
fn fast_pipeline_goes_green_and_writes_artifacts() {
    let workdir = tempfile::tempdir().unwrap();
    let script = StubScript::new(&[
        reply("plan-session", "# Plan\nDo the thing."),
        reply("impl-session", "Files written."),
    ]);

    let config = PipelineConfig {
        goal: "Build me a thing".to_string(),
        workdir: workdir.path().to_path_buf(),
        verify_commands: vec!["echo ok".to_string()],
        max_refine_iterations: 3,
        escalate_after: 2,
        plan_model: "fable".to_string(),
        implement_model: "sonnet".to_string(),
        refine_model: "sonnet".to_string(),
        escalation_model: "fable".to_string(),
        binary: script.binary(),
    };

    let (tx, rx) = mpsc::channel();
    let outcome = run_pipeline(&config, tx, CancelToken::new()).unwrap();
    let events: Vec<EngineEvent> = rx.try_iter().collect();

    assert!(outcome.green);
    assert_eq!(outcome.refine_iterations, 0);
    assert_eq!(outcome.stage_results.len(), 3); // plan, implement, refine
    assert!(outcome.final_report.is_none());

    let kinds = kinds(&events);
    assert!(kinds.contains(&"stage_started".to_string()));
    assert!(kinds.contains(&"verify_finished".to_string()));
    assert_eq!(kinds.last().map(String::as_str), Some("pipeline_finished"));

    // Output survives independent of the app.
    let run_dir = outcome.run_dir.unwrap();
    assert!(run_dir.starts_with(workdir.path().join("torch")));
    let artifact = std::fs::read_to_string(run_dir.join("artifact.md")).unwrap();
    assert!(artifact.contains("Build me a thing"));
    assert!(artifact.contains("# Plan"));
    assert!(run_dir.join("verify-1.log").is_file());
}

#[test]
fn refine_loop_escalates_then_admits_failure() {
    let workdir = tempfile::tempdir().unwrap();
    let script = StubScript::new(&[
        reply("plan-session", "# Plan"),
        reply("impl-session", "Files written."),
        reply("impl-session", "Tried a fix."),
        reply("impl-session", "Tried a deeper fix."),
    ]);

    let config = PipelineConfig {
        goal: "Build me a thing".to_string(),
        workdir: workdir.path().to_path_buf(),
        verify_commands: vec!["exit 7".to_string()],
        max_refine_iterations: 2,
        escalate_after: 1,
        plan_model: "fable".to_string(),
        implement_model: "sonnet".to_string(),
        refine_model: "sonnet".to_string(),
        escalation_model: "fable".to_string(),
        binary: script.binary(),
    };

    let (tx, rx) = mpsc::channel();
    let outcome = run_pipeline(&config, tx, CancelToken::new()).unwrap();
    let events: Vec<EngineEvent> = rx.try_iter().collect();

    // It never claims success it didn't verify.
    assert!(!outcome.green);
    assert_eq!(outcome.refine_iterations, 2);
    let report = outcome.final_report.expect("failing run keeps its report");
    assert!(report.summary().contains("FAILED"));
    assert!(report.failure_summary(10).contains("exit 7"));

    // The same failure surviving consecutive iterations escalates.
    let escalated: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            EngineEvent::RefineEscalated { model, .. } => Some(model.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(escalated, vec!["fable".to_string()]);

    let finished = events
        .iter()
        .rev()
        .find_map(|e| match e {
            EngineEvent::PipelineFinished {
                green,
                refine_iterations,
            } => Some((*green, *refine_iterations)),
            _ => None,
        })
        .unwrap();
    assert_eq!(finished, (false, 2));
}

fn standard_config(script: &StubScript, workdir: &std::path::Path) -> RunConfig {
    RunConfig {
        goal: "Build me a thing".to_string(),
        workdir: workdir.to_path_buf(),
        preset: Preset::Standard,
        intake: setting("sonnet", Effort::Low),
        plan: setting("fable", Effort::Max),
        critic_a: setting("opus", Effort::High),
        critic_b: None, // single critic keeps stub replies in order
        merge: setting("sonnet", Effort::Medium),
        implement: setting("sonnet", Effort::Medium),
        refine: setting("sonnet", Effort::Medium),
        escalation_model: "fable".to_string(),
        max_refine_iterations: 3,
        escalate_after: 2,
        checkpoint_before_implement: true,
        verify_commands: Vec::new(),
        binaries: [("claude".to_string(), script.binary())].into(),
        templates: Templates::default(),
    }
}

#[test]
fn standard_preset_threads_intake_critics_checkpoint_and_loop() {
    let workdir = tempfile::tempdir().unwrap();
    let script = StubScript::new(&[
        reply("main-session", "1. What platform?\n2. Are tests required?"),
        reply("main-session", "Brief.\nVERIFY: echo ok"),
        reply("main-session", "# Plan"),
        reply("critic-session", "F1. Reorder milestones."),
        reply("main-session", "# Final spec"),
        reply("impl-session", "Files written."),
    ]);
    // verify_commands left empty: the brief's VERIFY: line decides.
    let config = standard_config(&script, workdir.path());

    let (event_tx, event_rx) = mpsc::channel();
    let (control_tx, control_rx) = mpsc::channel();
    // Answer the pauses up front; mpsc buffers until the engine asks.
    control_tx
        .send(ControlMsg::IntakeAnswers(vec![
            "CLI".to_string(),
            "Yes".to_string(),
        ]))
        .unwrap();
    control_tx
        .send(ControlMsg::CheckpointDecision(true))
        .unwrap();

    let outcome = run_orchestrated(&config, event_tx, control_rx, CancelToken::new()).unwrap();
    let events: Vec<EngineEvent> = event_rx.try_iter().collect();

    assert!(outcome.green);
    let stages: Vec<&str> = outcome
        .stage_results
        .iter()
        .map(|(stage, _)| stage.as_str())
        .collect();
    assert_eq!(
        stages,
        vec!["intake", "plan", "critic-a", "merge", "implement", "refine"]
    );

    let kinds = kinds(&events);
    assert!(kinds.contains(&"awaiting_intake_answers".to_string()));
    assert!(kinds.contains(&"awaiting_checkpoint".to_string()));
    assert_eq!(kinds.last().map(String::as_str), Some("pipeline_finished"));

    let questions = events
        .iter()
        .find_map(|e| match e {
            EngineEvent::AwaitingIntakeAnswers { questions } => Some(questions.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        questions,
        vec![
            "What platform?".to_string(),
            "Are tests required?".to_string()
        ]
    );

    // The verify command came from the brief's VERIFY: line and went green.
    assert!(events
        .iter()
        .any(|e| matches!(e, EngineEvent::VerifyFinished { green: true, .. })));
}

#[test]
fn rejected_checkpoint_stops_before_any_file_is_written() {
    let workdir = tempfile::tempdir().unwrap();
    let script = StubScript::new(&[
        reply("main-session", "1. What platform?"),
        reply("main-session", "Brief."),
        reply("main-session", "# Plan"),
        reply("critic-session", "F1. Finding."),
        reply("main-session", "# Final spec"),
    ]);
    let mut config = standard_config(&script, workdir.path());
    config.verify_commands = vec!["echo ok".to_string()];

    let (event_tx, event_rx) = mpsc::channel();
    let (control_tx, control_rx) = mpsc::channel();
    control_tx
        .send(ControlMsg::IntakeAnswers(vec!["CLI".to_string()]))
        .unwrap();
    control_tx
        .send(ControlMsg::CheckpointDecision(false))
        .unwrap();

    let error = run_orchestrated(&config, event_tx, control_rx, CancelToken::new()).unwrap_err();
    let events: Vec<EngineEvent> = event_rx.try_iter().collect();

    assert_eq!(error.to_string(), "checkpoint rejected");
    // The implementer never started.
    assert!(!events.iter().any(|e| matches!(
        e,
        EngineEvent::StageStarted { stage, .. } if stage == "implement"
    )));
}
