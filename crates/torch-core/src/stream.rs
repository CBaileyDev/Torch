//! Parsing of the `claude --output-format stream-json` line protocol into
//! the engine's [`StreamEvent`]s. The wire shape of `StreamEvent` (tagged on
//! `kind`, snake_case) is part of the IPC contract in `docs/ipc-contract.md`.

use serde::{Deserialize, Serialize};

/// Token accounting attached to a stage result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
}

/// The terminal `result` event of one claude invocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunResult {
    pub subtype: String,
    #[serde(default)]
    pub is_error: bool,
    pub session_id: String,
    #[serde(default)]
    pub num_turns: u64,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub usage: Usage,
}

/// One event from a streaming claude session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamEvent {
    Init {
        session_id: String,
        model: String,
    },
    AssistantText {
        session_id: String,
        text: String,
    },
    AssistantToolUse {
        session_id: String,
        tool_name: String,
        input: serde_json::Value,
    },
    ToolResult {
        session_id: String,
    },
    Result(RunResult),
    Other {
        event_type: String,
    },
}

/// Parse one line of claude stream-json output. A single `assistant` line
/// can carry several content blocks, so this returns every event found on
/// the line; unparseable lines yield nothing.
pub fn parse_line(line: &str) -> Vec<StreamEvent> {
    let line = line.trim();
    if line.is_empty() {
        return Vec::new();
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return Vec::new();
    };
    let event_type = value.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let session_id = || {
        value
            .get("session_id")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string()
    };

    match event_type {
        "system" if value.get("subtype").and_then(|s| s.as_str()) == Some("init") => {
            vec![StreamEvent::Init {
                session_id: session_id(),
                model: value
                    .get("model")
                    .and_then(|m| m.as_str())
                    .unwrap_or("")
                    .to_string(),
            }]
        }
        "assistant" => content_blocks(&value)
            .iter()
            .filter_map(|block| match block.get("type").and_then(|t| t.as_str()) {
                Some("text") => Some(StreamEvent::AssistantText {
                    session_id: session_id(),
                    text: block
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string(),
                }),
                Some("tool_use") => Some(StreamEvent::AssistantToolUse {
                    session_id: session_id(),
                    tool_name: block
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .to_string(),
                    input: block
                        .get("input")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                }),
                _ => None,
            })
            .collect(),
        "user" => content_blocks(&value)
            .iter()
            .filter(|block| block.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
            .map(|_| StreamEvent::ToolResult {
                session_id: session_id(),
            })
            .collect(),
        "result" => match serde_json::from_value::<RunResult>(value) {
            Ok(result) => vec![StreamEvent::Result(result)],
            Err(_) => vec![StreamEvent::Other {
                event_type: "result".to_string(),
            }],
        },
        "" => Vec::new(),
        other => vec![StreamEvent::Other {
            event_type: other.to_string(),
        }],
    }
}

fn content_blocks(value: &serde_json::Value) -> Vec<serde_json::Value> {
    value
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_init() {
        let events =
            parse_line(r#"{"type":"system","subtype":"init","session_id":"abc","model":"sonnet"}"#);
        assert_eq!(
            events,
            vec![StreamEvent::Init {
                session_id: "abc".into(),
                model: "sonnet".into()
            }]
        );
    }

    #[test]
    fn parses_assistant_text_and_tool_use() {
        let events = parse_line(
            r#"{"type":"assistant","session_id":"abc","message":{"content":[
                {"type":"text","text":"hello"},
                {"type":"tool_use","name":"write_file","input":{"file_path":"a.rs"}}
            ]}}"#,
        );
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0],
            StreamEvent::AssistantText {
                session_id: "abc".into(),
                text: "hello".into()
            }
        );
        match &events[1] {
            StreamEvent::AssistantToolUse {
                tool_name, input, ..
            } => {
                assert_eq!(tool_name, "write_file");
                assert_eq!(input.get("file_path").unwrap(), "a.rs");
            }
            other => panic!("expected tool use, got {other:?}"),
        }
    }

    #[test]
    fn parses_tool_result() {
        let events = parse_line(
            r#"{"type":"user","session_id":"abc","message":{"content":[{"type":"tool_result","content":"ok"}]}}"#,
        );
        assert_eq!(
            events,
            vec![StreamEvent::ToolResult {
                session_id: "abc".into()
            }]
        );
    }

    #[test]
    fn parses_result() {
        let events = parse_line(
            r#"{"type":"result","subtype":"success","is_error":false,"session_id":"abc",
               "num_turns":3,"duration_ms":1200,"result":"done",
               "usage":{"input_tokens":10,"output_tokens":20}}"#,
        );
        match &events[0] {
            StreamEvent::Result(result) => {
                assert_eq!(result.subtype, "success");
                assert!(!result.is_error);
                assert_eq!(result.num_turns, 3);
                assert_eq!(result.usage.output_tokens, 20);
            }
            other => panic!("expected result, got {other:?}"),
        }
    }

    #[test]
    fn unknown_type_becomes_other_and_garbage_is_skipped() {
        assert_eq!(
            parse_line(r#"{"type":"telemetry"}"#),
            vec![StreamEvent::Other {
                event_type: "telemetry".into()
            }]
        );
        assert!(parse_line("not json").is_empty());
        assert!(parse_line("").is_empty());
    }

    #[test]
    fn stream_event_serializes_to_contract_shape() {
        let json = serde_json::to_value(StreamEvent::Init {
            session_id: "abc".into(),
            model: "sonnet".into(),
        })
        .unwrap();
        assert_eq!(json["kind"], "init");
        assert_eq!(json["session_id"], "abc");

        let json = serde_json::to_value(StreamEvent::Result(RunResult {
            subtype: "success".into(),
            is_error: false,
            session_id: "abc".into(),
            num_turns: 1,
            duration_ms: 5,
            result: None,
            usage: Usage::default(),
        }))
        .unwrap();
        assert_eq!(json["kind"], "result");
        assert_eq!(json["subtype"], "success");
    }
}
