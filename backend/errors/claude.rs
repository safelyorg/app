#[derive(Debug)]
pub enum ClaudeError {
    MissingApiKey,
    RequestFailed(String),
    ParseFailed(String),
}

impl std::fmt::Display for ClaudeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClaudeError::MissingApiKey => write!(f, "ANTHROPIC_API_KEY is not configured"),
            ClaudeError::RequestFailed(msg) => write!(f, "Claude request failed: {}", msg),
            ClaudeError::ParseFailed(msg) => {
                write!(f, "Failed to parse Claude's response: {}", msg)
            }
        }
    }
}
