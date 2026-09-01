use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct RiskFactor {
    pub severity: String,
    pub name: String,
    pub description: String,
    pub contributing_signals: Vec<String>,
}
