use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn risk_level(score: u8) -> String {
    if score <= 33 {
        "low".into()
    } else if score <= 66 {
        "caution".into()
    } else {
        "high".into()
    }
}

#[wasm_bindgen]
pub fn risk_label(level: &str) -> String {
    match level {
        "low" => "Low risk".into(),
        "caution" => "Caution".into(),
        _ => "High risk".into(),
    }
}

#[wasm_bindgen]
pub fn risk_desc(desc: &str) -> String {
    match desc {
        "low" => "Safe to proceed".into(),
        "caution" => "Review before proceeding".into(),
        _ => "High risk detected".into(),
    }
}
