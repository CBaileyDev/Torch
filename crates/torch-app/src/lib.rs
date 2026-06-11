//! Torch Tauri shell: exposes the torch-core orchestrator to the UI as
//! commands + an `engine-event` stream, and persists everything to SQLite.

mod db;

use std::collections::HashMap;
use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use torch_core::claude::CancelToken;
use torch_core::orchestrator::{run_orchestrated, ControlMsg, RunConfig};
use torch_core::pipeline::{EngineEvent, PipelineError};
use torch_core::templates::Templates;

struct RunHandle {
    control_tx: Sender<ControlMsg>,
    cancel: CancelToken,
}

struct AppState {
    runs: Mutex<HashMap<String, RunHandle>>,
    db: Mutex<rusqlite::Connection>,
}

#[derive(Clone, Serialize)]
struct EventEnvelope {
    #[serde(rename = "runId")]
    run_id: String,
    event: EngineEvent,
}

type CmdResult<T> = Result<T, String>;

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Overlay user-edited templates from the DB onto the defaults.
fn templates_with_overrides(conn: &rusqlite::Connection) -> Templates {
    let mut templates = Templates::default();
    if let Ok(saved) = db::all_templates(conn) {
        for (name, content) in saved {
            match name.as_str() {
                "intake_questions" => templates.intake_questions = content,
                "intake_brief" => templates.intake_brief = content,
                "plan" => templates.plan = content,
                "critic" => templates.critic = content,
                "merge" => templates.merge = content,
                "implement" => templates.implement = content,
                "refine" => templates.refine = content,
                "architect" => templates.architect = content,
                "drafter" => templates.drafter = content,
                "reviser" => templates.reviser = content,
                _ => {}
            }
        }
    }
    templates
}

#[tauri::command]
fn start_run(
    app: AppHandle,
    state: State<'_, AppState>,
    mut config: RunConfig,
) -> CmdResult<String> {
    let run_id = uuid::Uuid::new_v4().to_string();
    let preset = serde_json::to_value(config.preset)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "standard".into());

    // Resolve every provider CLI to an absolute path so spawning works even
    // when the GUI's PATH omits Homebrew / ~/.local/bin. User-supplied
    // overrides (if any) win.
    for provider in torch_core::claude::Provider::ALL {
        config
            .binaries
            .entry(provider.id().to_string())
            .or_insert_with(|| {
                provider
                    .resolve_binary()
                    .unwrap_or_else(|| std::path::PathBuf::from(provider.binary()))
            });
    }

    {
        let conn = state.db.lock().unwrap();
        config.templates = templates_with_overrides(&conn);
        db::insert_run(
            &conn,
            &run_id,
            &config.goal,
            &config.workdir.display().to_string(),
            &preset,
            now_secs(),
        )
        .map_err(|e| e.to_string())?;
    }

    let (event_tx, event_rx) = mpsc::channel::<EngineEvent>();
    let (control_tx, control_rx) = mpsc::channel::<ControlMsg>();
    let cancel = CancelToken::new();

    state.runs.lock().unwrap().insert(
        run_id.clone(),
        RunHandle {
            control_tx,
            cancel: cancel.clone(),
        },
    );

    // Event pump: persist every event and forward it to the UI.
    let pump_app = app.clone();
    let pump_run_id = run_id.clone();
    let pump = std::thread::spawn(move || {
        let state = pump_app.state::<AppState>();
        for event in event_rx {
            let envelope = EventEnvelope {
                run_id: pump_run_id.clone(),
                event,
            };
            if let Ok(json) = serde_json::to_string(&envelope.event) {
                let conn = state.db.lock().unwrap();
                let _ = db::append_event(&conn, &pump_run_id, &json);
                let status = match &envelope.event {
                    EngineEvent::AwaitingIntakeAnswers { .. }
                    | EngineEvent::AwaitingCheckpoint { .. } => Some("waiting"),
                    EngineEvent::StageStarted { .. } => Some("running"),
                    _ => None,
                };
                if let Some(status) = status {
                    let _ = db::set_run_status(&conn, &pump_run_id, status);
                }
            }
            let _ = pump_app.emit("engine-event", &envelope);
        }
    });

    // Orchestrator thread: owns the run from start to finish.
    let orch_app = app.clone();
    let orch_run_id = run_id.clone();
    std::thread::spawn(move || {
        let outcome = run_orchestrated(&config, event_tx, control_rx, cancel);
        let _ = pump.join(); // event_tx dropped above → pump drains and exits

        let state = orch_app.state::<AppState>();
        state.runs.lock().unwrap().remove(&orch_run_id);

        let conn = state.db.lock().unwrap();
        match outcome {
            Ok(outcome) => {
                let (turns, tokens) = outcome
                    .stage_results
                    .iter()
                    .fold((0i64, 0i64), |(t, o), (_, r)| {
                        (t + r.num_turns as i64, o + r.usage.output_tokens as i64)
                    });
                let status = if outcome.green { "green" } else { "not_green" };
                let _ = db::finish_run(
                    &conn,
                    &orch_run_id,
                    status,
                    outcome.refine_iterations as i64,
                    turns,
                    tokens,
                );
            }
            Err(error) => {
                let status = match &error {
                    PipelineError::Cancelled | PipelineError::CheckpointRejected => "cancelled",
                    _ => "failed",
                };
                let _ = orch_app.emit(
                    "run-failed",
                    json!({ "runId": orch_run_id, "error": error.to_string() }),
                );
            }
        }
    });

    Ok(run_id)
}

#[tauri::command]
fn send_intake_answers(
    state: State<'_, AppState>,
    run_id: String,
    answers: Vec<String>,
) -> CmdResult<()> {
    let runs = state.runs.lock().unwrap();
    let handle = runs.get(&run_id).ok_or("run not found or finished")?;
    handle
        .control_tx
        .send(ControlMsg::IntakeAnswers(answers))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn checkpoint_decision(
    state: State<'_, AppState>,
    run_id: String,
    approved: bool,
) -> CmdResult<()> {
    let runs = state.runs.lock().unwrap();
    let handle = runs.get(&run_id).ok_or("run not found or finished")?;
    handle
        .control_tx
        .send(ControlMsg::CheckpointDecision(approved))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn cancel_run(state: State<'_, AppState>, run_id: String) -> CmdResult<()> {
    let runs = state.runs.lock().unwrap();
    let handle = runs.get(&run_id).ok_or("run not found or finished")?;
    handle.cancel.cancel();
    Ok(())
}

#[tauri::command]
fn list_runs(state: State<'_, AppState>) -> CmdResult<Vec<db::RunSummary>> {
    let conn = state.db.lock().unwrap();
    db::list_runs(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_run_events(state: State<'_, AppState>, run_id: String) -> CmdResult<Vec<serde_json::Value>> {
    let conn = state.db.lock().unwrap();
    let rows = db::run_events(&conn, &run_id).map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .filter_map(|j| serde_json::from_str::<serde_json::Value>(&j).ok())
        .map(|event| json!({ "runId": run_id, "event": event }))
        .collect())
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> CmdResult<HashMap<String, String>> {
    let conn = state.db.lock().unwrap();
    Ok(db::all_settings(&conn)
        .map_err(|e| e.to_string())?
        .into_iter()
        .collect())
}

#[tauri::command]
fn save_setting(state: State<'_, AppState>, key: String, value: String) -> CmdResult<()> {
    let conn = state.db.lock().unwrap();
    db::save_setting(&conn, &key, &value).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_templates(state: State<'_, AppState>) -> CmdResult<HashMap<String, String>> {
    let conn = state.db.lock().unwrap();
    let defaults = Templates::default();
    let mut map: HashMap<String, String> = [
        ("intake_questions", defaults.intake_questions),
        ("intake_brief", defaults.intake_brief),
        ("plan", defaults.plan),
        ("critic", defaults.critic),
        ("merge", defaults.merge),
        ("implement", defaults.implement),
        ("refine", defaults.refine),
        ("architect", defaults.architect),
        ("drafter", defaults.drafter),
        ("reviser", defaults.reviser),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    for (name, content) in db::all_templates(&conn).map_err(|e| e.to_string())? {
        map.insert(name, content);
    }
    Ok(map)
}

#[tauri::command]
fn save_template(state: State<'_, AppState>, name: String, content: String) -> CmdResult<()> {
    let conn = state.db.lock().unwrap();
    db::save_template(&conn, &name, &content).map_err(|e| e.to_string())
}

/// Probe which model aliases this machine's `claude` login can use.
/// Results are cached in settings; pass `force` to re-probe.
#[tauri::command]
async fn probe_models(state: State<'_, AppState>, force: Option<bool>) -> CmdResult<Vec<String>> {
    if force != Some(true) {
        let conn = state.db.lock().unwrap();
        if let Ok(Some(cached)) = db::get_setting(&conn, "available_models") {
            if let Ok(models) = serde_json::from_str::<Vec<String>>(&cached) {
                if !models.is_empty() {
                    return Ok(models);
                }
            }
        }
    }

    let claude_bin = torch_core::claude::Provider::Claude
        .resolve_binary()
        .unwrap_or_else(|| std::path::PathBuf::from("claude"));
    const CANDIDATES: [&str; 4] = ["sonnet", "opus", "fable", "haiku"];
    let handles: Vec<_> = CANDIDATES
        .iter()
        .map(|alias| {
            let alias = alias.to_string();
            let claude_bin = claude_bin.clone();
            std::thread::spawn(move || {
                let output = std::process::Command::new(&claude_bin)
                    .args([
                        "-p",
                        "Reply with exactly: OK",
                        "--model",
                        &alias,
                        "--output-format",
                        "json",
                        "--max-turns",
                        "1",
                    ])
                    .output();
                let available = output
                    .ok()
                    .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok())
                    .map(|v| v.get("is_error").and_then(|e| e.as_bool()) == Some(false))
                    .unwrap_or(false);
                (alias, available)
            })
        })
        .collect();

    let mut models = Vec::new();
    for handle in handles {
        if let Ok((alias, true)) = handle.join() {
            models.push(alias);
        }
    }

    let conn = state.db.lock().unwrap();
    let _ = db::save_setting(
        &conn,
        "available_models",
        &serde_json::to_string(&models).unwrap_or_default(),
    );
    Ok(models)
}

/// Which provider CLIs are installed, plus model suggestions for each.
/// Claude's models come from the cached live probe; other providers use
/// the registry's suggestions (their CLIs accept any model id).
#[tauri::command]
async fn probe_providers(state: State<'_, AppState>) -> CmdResult<Vec<serde_json::Value>> {
    use torch_core::claude::Provider;

    let claude_models: Vec<String> = {
        let conn = state.db.lock().unwrap();
        db::get_setting(&conn, "available_models")
            .ok()
            .flatten()
            .and_then(|cached| serde_json::from_str(&cached).ok())
            .unwrap_or_default()
    };

    let providers = Provider::ALL
        .iter()
        .map(|p| {
            let resolved = p.resolve_binary();
            eprintln!("[probe] {} -> {:?}", p.id(), resolved);
            let available = resolved.is_some();
            let models: Vec<String> = if *p == Provider::Claude && !claude_models.is_empty() {
                claude_models.clone()
            } else {
                p.suggested_models().iter().map(|m| m.to_string()).collect()
            };
            json!({ "id": p.id(), "available": available, "models": models })
        })
        .collect();
    Ok(providers)
}

#[tauri::command]
async fn pick_directory(app: AppHandle) -> CmdResult<Option<String>> {
    Ok(app
        .dialog()
        .file()
        .blocking_pick_folder()
        .map(|p| p.to_string()))
}

/// JS-side errors forwarded here so launch failures are debuggable headlessly.
#[tauri::command]
fn js_log(message: String) {
    eprintln!("[webview] {message}");
}

const ERROR_FORWARDER: &str = r#"
window.addEventListener('error', (e) => {
  try { window.__TAURI_INTERNALS__.invoke('js_log', { message: 'error: ' + e.message + ' @ ' + e.filename + ':' + e.lineno }); } catch (_) {}
});
window.addEventListener('unhandledrejection', (e) => {
  try { window.__TAURI_INTERNALS__.invoke('js_log', { message: 'unhandledrejection: ' + (e.reason && (e.reason.stack || e.reason.message) || String(e.reason)) }); } catch (_) {}
});
"#;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .append_invoke_initialization_script(ERROR_FORWARDER)
        .on_page_load(|_, payload| {
            eprintln!("[page-load] {:?} {}", payload.event(), payload.url());
        })
        .setup(move |app| {
            eprintln!("[setup] torch-app starting");
            // Standard asset-protocol window (IPC intact). Built in code —
            // not tauri.conf.json — so we can set the macOS overlay titlebar.
            let window = tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::default(),
            )
            .title("TORCH")
            .inner_size(1280.0, 840.0)
            .min_inner_size(980.0, 640.0)
            .background_color(tauri::webview::Color(18, 19, 21, 255));
            #[cfg(target_os = "macos")]
            let window = window
                .title_bar_style(tauri::TitleBarStyle::Overlay)
                .hidden_title(true);
            window.build()?;
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let conn = db::open(&data_dir.join("torch.db"))?;
            app.manage(AppState {
                runs: Mutex::new(HashMap::new()),
                db: Mutex::new(conn),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_run,
            send_intake_answers,
            checkpoint_decision,
            cancel_run,
            list_runs,
            get_run_events,
            get_settings,
            save_setting,
            get_templates,
            save_template,
            probe_models,
            probe_providers,
            pick_directory,
            js_log,
        ])
        .run(tauri::generate_context!())
        .expect("error while running torch");
}
