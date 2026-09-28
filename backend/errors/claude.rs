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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_exceeded_displays_the_real_dependency_down_message() {
        assert_eq!(
            ClaudeError::QuotaExceeded.to_string(),
            "DEPENDENCY DOWN: Anthropic API credits/quota exhausted"
        );
    }

    #[test]
    fn unauthorized_displays_the_real_dependency_down_message() {
        assert_eq!(
            ClaudeError::Unauthorized.to_string(),
            "DEPENDENCY DOWN: Anthropic API key rejected (401/403)"
        );
    }

    #[test]
    fn service_unavailable_displays_the_real_status_code_it_was_given() {
        assert_eq!(
            ClaudeError::ServiceUnavailable(503).to_string(),
            "DEPENDENCY DOWN: Anthropic API returned status 503"
        );
    }

    #[test]
    fn service_unavailable_correctly_interpolates_a_different_code_too() {
        assert_eq!(
            ClaudeError::ServiceUnavailable(500).to_string(),
            "DEPENDENCY DOWN: Anthropic API returned status 500"
        );
    }
}
