//! Test stand-in for the `claude` CLI: replays canned stream-json output so
//! the engine's tests (and yours) need no login and no network.
//!
//! Reply source, first match wins:
//! - `TORCH_STUB_REPLAY` env — path of one stream-json file to print.
//! - `TORCH_STUB_DIR` env — directory of `reply-<n>.jsonl` files replayed in
//!   invocation order (atomic `claim-<n>` files in the directory track `n`,
//!   so parallel invocations stay race-free).
//! - a `torch-stub.config` file beside the executable whose contents are the
//!   reply directory path — this is how tests run isolated stubs in parallel
//!   without touching the process environment.
//!
//! `TORCH_STUB_LOG` (optional) — each invocation appends its argv line so
//! tests can assert how the engine called the CLI.

use std::env;
use std::fs;
use std::path::PathBuf;

fn reply_dir_from_sidecar() -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    let config = exe.parent()?.join("torch-stub.config");
    let dir = fs::read_to_string(config).ok()?;
    Some(PathBuf::from(dir.trim()))
}

fn next_reply_in(dir: PathBuf) -> PathBuf {
    // Claim the next sequence number atomically: `create_new` can only
    // succeed for one process per claim file, so parallel invocations
    // (e.g. ensemble critics) each replay a distinct reply.
    let mut n: u32 = 1;
    while n < 10_000 {
        let claim = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(format!("claim-{n}")));
        if claim.is_ok() {
            break;
        }
        n += 1;
    }
    let numbered = dir.join(format!("reply-{n}.jsonl"));
    if numbered.is_file() {
        numbered
    } else {
        dir.join("reply-default.jsonl")
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if let Ok(log) = env::var("TORCH_STUB_LOG") {
        let mut line = args.join(" ");
        line.push('\n');
        let existing = fs::read_to_string(&log).unwrap_or_default();
        let _ = fs::write(&log, existing + &line);
    }

    let replay = if let Ok(path) = env::var("TORCH_STUB_REPLAY") {
        Some(PathBuf::from(path))
    } else {
        env::var("TORCH_STUB_DIR")
            .ok()
            .map(PathBuf::from)
            .or_else(reply_dir_from_sidecar)
            .map(next_reply_in)
    };

    match replay {
        Some(path) => match fs::read_to_string(&path) {
            Ok(content) => print!("{content}"),
            Err(error) => {
                eprintln!("torch-stub-claude: cannot read {}: {error}", path.display());
                std::process::exit(2);
            }
        },
        None => {
            eprintln!(
                "torch-stub-claude: set TORCH_STUB_REPLAY or TORCH_STUB_DIR, or place a \
                 torch-stub.config beside the executable; this binary only replays \
                 canned output for tests"
            );
            std::process::exit(2);
        }
    }
}
