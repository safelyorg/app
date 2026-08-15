use std::fmt::{Display, Formatter, Result};

#[derive(Debug)]
pub enum ClaudeError {
    MissingApiKey,
    RequestFailed(String),
    ParseFailed(String),
}

impl Display for ClaudeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            ClaudeError::MissingApiKey => write!(f, "ANTHROPIC_API_KEY is not configured"),
            ClaudeError::RequestFailed(msg) => write!(f, "Claude request failed: {}", msg),
            ClaudeError::ParseFailed(msg) => {
                write!(f, "Failed to parse Claude's response: {}", msg)
            }
        }
    }
}
