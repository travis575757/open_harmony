use cp_engine::analyze;
use cp_io::{parse_request_json, to_response_json};

pub fn analyze_json(input: &str) -> String {
    match parse_request_json(input)
        .map_err(|e| e.to_string())
        .and_then(|req| analyze(&req).map_err(|e| e.to_string()))
        .and_then(|resp| to_response_json(&resp).map_err(|e| e.to_string()))
    {
        Ok(s) => s,
        Err(e) => format!("{{\"error\":\"{}\"}}", e.replace('"', "\\\"")),
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm_export {
    use super::analyze_json;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    pub fn analyze_json_wasm(input: &str) -> String {
        analyze_json(input)
    }
}

#[cfg(test)]
mod tests {
    use super::analyze_json;

    #[test]
    fn returns_json_error_on_invalid_input() {
        let out = analyze_json("not json");
        assert!(out.contains("error"));
    }
}
