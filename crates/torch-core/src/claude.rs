//! Supervision of headless `claude` CLI processes: spawning, streaming
//! stdout into [`StreamEvent`]s, session resumption, and cancellation.
//!
//! Torch authenticates through the user's existing `claude` login — the
//! engine never handles an API key.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::stream::{parse_line, RunResult, StreamEvent};

/// Cooperative cancellation shared between front-end and engine threads.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// A provider CLI the engine can drive. Only Claude Code today; the
/// registry shape leaves room for more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Claude,
}

impl Provider {
    pub const ALL: &'static [Provider] = &[Provider::Claude];

    pub fn id(&self) -> &'static str {
        match self {
            Provider::Claude => "claude",
        }
    }

    /// Binary name as invoked when no resolved path is known.
    pub fn binary(&self) -> &'static str {
        match self {
            Provider::Claude => "claude",
        }
    }

    /// Model aliases worth offering before a live probe has run.
    pub fn suggested_models(&self) -> &'static [&'static str] {
        match self {
            Provider::Claude => &["sonnet", "opus", "fable", "haiku"],
        }
    }

    /// Find the provider CLI on this machine. GUI processes often launch
    /// with a minimal PATH, so well-known install dirs are searched too.
    pub fn resolve_binary(&self) -> Option<PathBuf> {
        find_in_path(self.binary())
    }
}

fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join(".claude").join("local"));
    }
    dirs.push(PathBuf::from("/opt/homebrew/bin"));
    dirs.push(PathBuf::from("/usr/local/bin"));
    dirs
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let extensions: &[&str] = if cfg!(windows) {
        &[".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    for dir in candidate_dirs() {
        for ext in extensions {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[derive(Debug, thiserror::Error)]
pub enum ClaudeError {
    #[error("failed to spawn {binary}: {source}")]
    Spawn {
        binary: String,
        source: std::io::Error,
    },
    #[error("failed reading claude output: {0}")]
    Read(#[from] std::io::Error),
    #[error("claude exited ({status}) without a result event")]
    NoResult { status: String },
    #[error("run cancelled")]
    Cancelled,
}

/// One headless claude invocation.
pub struct Invocation<'a> {
    pub binary: &'a Path,
    pub workdir: &'a Path,
    pub model: &'a str,
    pub prompt: &'a str,
    /// Session id to resume; `None` starts a fresh session.
    pub resume_session: Option<&'a str>,
    pub cancel: &'a CancelToken,
}

/// Run one invocation to completion, forwarding every stream event, and
/// return the terminal [`RunResult`].
pub fn run_invocation(
    invocation: &Invocation,
    mut on_event: impl FnMut(StreamEvent),
) -> Result<RunResult, ClaudeError> {
    if invocation.cancel.is_cancelled() {
        return Err(ClaudeError::Cancelled);
    }

    let mut command = Command::new(invocation.binary);
    command
        .arg("-p")
        .arg(invocation.prompt)
        .arg("--model")
        .arg(invocation.model)
        .arg("--output-format")
        .arg("stream-json")
        .arg("--verbose");
    if let Some(session) = invocation.resume_session {
        command.arg("--resume").arg(session);
    }
    command
        .current_dir(invocation.workdir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let mut child = command.spawn().map_err(|source| ClaudeError::Spawn {
        binary: invocation.binary.display().to_string(),
        source,
    })?;

    let stdout = child.stdout.take().expect("stdout was piped");
    let reader = BufReader::new(stdout);
    let mut final_result = None;

    for line in reader.lines() {
        if invocation.cancel.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ClaudeError::Cancelled);
        }
        let line = line?;
        for event in parse_line(&line) {
            if let StreamEvent::Result(result) = &event {
                final_result = Some(result.clone());
            }
            on_event(event);
        }
    }

    let status = child.wait()?;
    if invocation.cancel.is_cancelled() {
        return Err(ClaudeError::Cancelled);
    }
    final_result.ok_or(ClaudeError::NoResult {
        status: status.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_token_round_trip() {
        let token = CancelToken::new();
        assert!(!token.is_cancelled());
        let clone = token.clone();
        clone.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn provider_registry_exposes_claude() {
        assert_eq!(Provider::ALL, &[Provider::Claude]);
        assert_eq!(Provider::Claude.id(), "claude");
        assert!(Provider::Claude.suggested_models().contains(&"sonnet"));
    }
}
