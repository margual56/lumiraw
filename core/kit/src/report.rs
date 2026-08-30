//! What each stage decided, for the interface to show and for tests to read.

use serde_json::{Map, Value};

/// Everything the pipeline decided, keyed by stage name (insertion ordered,
/// like the Python dict the interface expects).
#[derive(Default, Clone, Debug)]
pub struct Report {
    pub order: Vec<String>,
    pub stages: Map<String, Value>,
}

impl Report {
    pub fn new() -> Self {
        Report::default()
    }
    pub fn add(&mut self, stage: &str, info: Value) {
        if !self.stages.contains_key(stage) {
            self.order.push(stage.to_string());
        }
        self.stages.insert(stage.to_string(), info);
    }
    pub fn get(&self, stage: &str) -> Value {
        self.stages.get(stage).cloned().unwrap_or(Value::Object(Map::new()))
    }
    pub fn to_json(&self) -> Value {
        let mut out = Map::new();
        for k in &self.order {
            out.insert(k.clone(), self.stages[k].clone());
        }
        Value::Object(out)
    }
}

impl Report {
    /// A stage that did not run: what it measured, marked not applied, with
    /// the reason (a code the interface translates).
    pub fn skipped(&mut self, stage: &str, reason: &str, measured: Value) {
        self.add(stage, with(measured, serde_json::json!({"applied": false, "reason": reason})));
    }
}

/// `value` with the fields of `extra` added (or replaced). `value` is
/// expected to be a JSON object; anything else is returned unchanged.
pub fn with(mut value: Value, extra: Value) -> Value {
    if let (Some(o), Value::Object(fields)) = (value.as_object_mut(), extra) {
        for (k, v) in fields {
            o.insert(k, v);
        }
    }
    value
}

/// A number rounded for a report, as the interface shows it.
pub fn round_to(v: f32, places: i32) -> f64 {
    let f = 10f64.powi(places);
    ((v as f64) * f).round() / f
}
