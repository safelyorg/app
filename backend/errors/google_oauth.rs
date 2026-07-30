use std::fmt;

pub enum GoogleOauthConfigError {
    GoogleClientId,
    GoogleRedirectUri,
    GoogleClientSecret,
    TokenRequestFailed(String),
    TokenExchangeFailed(String),
    TokenResponseParseFailed(String),
    UserInfoRequestFailed(String),
    UserInfoFetchFailed(String),
    UserInfoResponseParseFailed(String),
}

impl fmt::Display for GoogleOauthConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GoogleOauthConfigError::GoogleClientId => write!(f, "GOOGLE_CLIENT_ID needs to be set"),
            GoogleOauthConfigError::GoogleRedirectUri => {
                write!(f, "GOOGLE_REDIRECT_URI needs to be set")
            }
            GoogleOauthConfigError::GoogleClientSecret => {
                write!(f, "GOOGLE_CLIENT_SECRET needs to be set")
            }
            GoogleOauthConfigError::TokenRequestFailed(e) => {
                write!(f, "Token request needs to succeed {}", e)
            }
            GoogleOauthConfigError::TokenExchangeFailed(e) => {
                write!(f, "Google token exchange needs to succeed {}", e)
            }
            GoogleOauthConfigError::TokenResponseParseFailed(e) => {
                write!(f, "Token response needs to pass {}", e)
            }
            GoogleOauthConfigError::UserInfoRequestFailed(e) => {
                write!(f, "User info request needs to pass {}", e)
            }
            GoogleOauthConfigError::UserInfoFetchFailed(e) => {
                write!(f, "User info fetching should succeed {}", e)
            }
            GoogleOauthConfigError::UserInfoResponseParseFailed(e) => {
                write!(f, "User info response parse should pass {}", e)
            }
        }
    }
}
