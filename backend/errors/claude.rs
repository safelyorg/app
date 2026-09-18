use std::fmt::{Display, Formatter, Result};

#[derive(Debug)]
pub enum ClaudeError {
    MissingApiKey,
    RequestFailed(String),
    ParseFailed(String),
    QuotaExceeded,
    Unauthorized,
    ServiceUnavailable(u16),
}

impl Display for ClaudeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            ClaudeError::MissingApiKey => write!(f, "ANTHROPIC_API_KEY is not configured"),
            ClaudeError::RequestFailed(msg) => write!(f, "Claude request failed: {}", msg),
            ClaudeError::ParseFailed(msg) => {
                write!(f, "Failed to parse Claude's response: {}", msg)
            }
            ClaudeError::QuotaExceeded => {
                write!(f, "DEPENDENCY DOWN: Anthropic API credits/quota exhausted")
            }
            ClaudeError::Unauthorized => {
                write!(f, "DEPENDENCY DOWN: Anthropic API key rejected (401/403)")
            }
            ClaudeError::ServiceUnavailable(code) => {
                write!(f, "DEPENDENCY DOWN: Anthropic API returned status {}", code)
            }
        }
    }
}
