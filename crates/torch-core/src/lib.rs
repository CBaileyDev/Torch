//! Torch orchestrator engine.
//!
//! UI-agnostic: front-ends (the Tauri shell, the headless CLI) drive the
//! engine through [`pipeline::run_pipeline`] / [`orchestrator::run_orchestrated`]
//! and consume [`pipeline::EngineEvent`]s from an mpsc channel. The engine
//! talks to AI models exclusively by supervising headless `claude` CLI
//! processes — it never uses the Anthropic API and never sees an API key.

pub mod claude;
pub mod orchestrator;
pub mod pipeline;
pub mod stream;
pub mod templates;
