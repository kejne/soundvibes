use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub const API_VERSION: &str = "1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlRequest {
    #[serde(default = "api_version_string")]
    pub api_version: String,
    #[serde(flatten)]
    pub command: ControlCommand,
}

impl ControlRequest {
    pub fn new(command: ControlCommand) -> Self {
        Self {
            api_version: api_version_string(),
            command,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "kebab-case")]
pub enum ControlCommand {
    Toggle {
        #[serde(skip_serializing_if = "Option::is_none")]
        lang: Option<String>,
    },
    Status,
    SetLanguage {
        lang: String,
    },
    Stop,
}

impl ControlCommand {
    pub fn toggle(lang: Option<String>) -> Self {
        Self::Toggle { lang }
    }

    pub fn set_language(lang: String) -> Self {
        Self::SetLanguage { lang }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlResponse {
    #[serde(default = "api_version_string")]
    pub api_version: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl ControlResponse {
    pub fn ok(state: Option<String>, language: Option<String>) -> Self {
        Self {
            api_version: api_version_string(),
            ok: true,
            state,
            language,
            error: None,
            message: None,
        }
    }

    pub fn error(error: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            api_version: api_version_string(),
            ok: false,
            state: None,
            language: None,
            error: Some(error.into()),
            message: Some(message.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonEvent {
    #[serde(default = "api_version_string")]
    pub api_version: String,
    pub timestamp: String,
    #[serde(flatten)]
    pub event: DaemonEventType,
}

impl DaemonEvent {
    pub fn new(timestamp: impl Into<String>, event: DaemonEventType) -> Self {
        Self {
            api_version: api_version_string(),
            timestamp: timestamp.into(),
            event,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonEventType {
    DaemonReady,
    RecordingStarted {
        language: String,
    },
    RecordingStopped {
        language: String,
    },
    TranscriptFinal {
        language: String,
        utterance: u64,
        duration_ms: u64,
        text: String,
    },
    ModelLoaded {
        language: String,
        model_size: String,
        model_language: String,
    },
    Error {
        message: String,
    },
}

pub fn to_json_line<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let mut json = serde_json::to_string(value)?;
    json.push('\n');
    Ok(json)
}

pub fn from_json_line<T: DeserializeOwned>(line: &str) -> Result<T, serde_json::Error> {
    serde_json::from_str(line.trim_end_matches('\n'))
}

pub fn parse_control_response(line: &str) -> Result<ControlResponse, serde_json::Error> {
    from_json_line(line)
}

pub fn parse_control_request(command: &str) -> Result<ControlRequest, String> {
    let mut tokens = command.split_whitespace();
    let Some(action) = tokens.next() else {
        return Ok(ControlRequest::new(ControlCommand::toggle(None)));
    };

    match action {
        "toggle" => {
            let mut lang = None;
            for token in tokens {
                if let Some(value) = token.strip_prefix("lang=") {
                    if value.is_empty() {
                        return Err("lang value cannot be empty".to_string());
                    }
                    if lang.is_some() {
                        return Err("duplicate lang token".to_string());
                    }
                    lang = Some(value.to_string());
                } else {
                    return Err(format!("unexpected token '{token}' for toggle"));
                }
            }
            Ok(ControlRequest::new(ControlCommand::toggle(lang)))
        }
        "status" => {
            if let Some(token) = tokens.next() {
                return Err(format!("unexpected token '{token}' for status"));
            }
            Ok(ControlRequest::new(ControlCommand::Status))
        }
        "set-language" => {
            let mut lang = None;
            for token in tokens {
                if let Some(value) = token.strip_prefix("lang=") {
                    if value.is_empty() {
                        return Err("lang value cannot be empty".to_string());
                    }
                    if lang.is_some() {
                        return Err("duplicate lang token".to_string());
                    }
                    lang = Some(value.to_string());
                } else {
                    return Err(format!("unexpected token '{token}' for set-language"));
                }
            }
            let lang = lang.ok_or_else(|| "missing lang=<CODE>".to_string())?;
            Ok(ControlRequest::new(ControlCommand::set_language(lang)))
        }
        "stop" => {
            if let Some(token) = tokens.next() {
                return Err(format!("unexpected token '{token}' for stop"));
            }
            Ok(ControlRequest::new(ControlCommand::Stop))
        }
        _ => Err(format!("unsupported daemon command: {action}")),
    }
}

fn api_version_string() -> String {
    API_VERSION.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_request_json_round_trip_includes_api_version() {
        let request = ControlRequest::new(ControlCommand::toggle(Some("fr".to_string())));

        let line = to_json_line(&request).expect("request should serialize");
        assert!(line.ends_with('\n'));
        assert!(line.contains("\"api_version\":\"1\""));

        let parsed: ControlRequest = from_json_line(&line).expect("request should parse");
        assert_eq!(parsed, request);
    }

    #[test]
    fn daemon_event_json_round_trip_includes_api_version() {
        let event = DaemonEvent::new(
            "2026-02-05T12:01:12Z",
            DaemonEventType::TranscriptFinal {
                language: "fr".to_string(),
                utterance: 1,
                duration_ms: 1_200,
                text: "bonjour".to_string(),
            },
        );

        let line = to_json_line(&event).expect("event should serialize");
        assert!(line.ends_with('\n'));
        assert!(line.contains("\"api_version\":\"1\""));

        let parsed: DaemonEvent = from_json_line(&line).expect("event should parse");
        assert_eq!(parsed, event);
    }

    #[test]
    fn parses_control_response_json_line() {
        let line = "{\"api_version\":\"1\",\"ok\":true,\"state\":\"idle\",\"language\":\"en\"}\n";
        let response = parse_control_response(line).expect("response should parse");

        assert!(response.ok);
        assert_eq!(response.state.as_deref(), Some("idle"));
        assert_eq!(response.language.as_deref(), Some("en"));
    }
}
