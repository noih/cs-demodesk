//! Versioned per-demo/player assessments; analysis producers remain independent.
pub mod clips;
pub mod combat_stats;
pub mod crosshair_lock;
pub mod engagement_context;
pub mod history;
pub mod movement;
pub mod native;
mod smoke_estimate;
pub mod queue;
mod rules;
pub mod shot_view;
pub mod statistics;
pub mod time_to_damage;
pub mod view_angles;
pub use rules::evaluate_match;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const RULESET_VERSION: &str = "16-estimated-smoke-rate";
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum State {
    Passed,
    Findings,
    Unavailable,
    Failed,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Definition {
    pub id: String,
    pub version: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub parameters: serde_json::Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measurement {
    pub name: String,
    pub value: f64,
    pub unit: String,
    pub threshold: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub id: String,
    pub group: String,
    pub round: i32,
    pub start_tick: i32,
    pub end_tick: i32,
    pub target_id: String,
    pub reason: String,
    pub measurements: Vec<Measurement>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub definition: Definition,
    pub state: State,
    pub reason: String,
    pub reason_code: String,
    pub evaluated_samples: usize,
    pub findings: Vec<Finding>,
    pub observations: Vec<Finding>,
    pub occurrences: Vec<Occurrence>,
    pub summary: Vec<Measurement>,
    /// Original measurement evidence for each counted event.
    pub diagnostics: serde_json::Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Occurrence {
    pub id: String,
    pub round: i32,
    pub start_tick: i32,
    pub end_tick: i32,
    pub target_id: String,
    pub source_ids: Vec<String>,
    pub measurements: Vec<Measurement>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assessment {
    pub schema_version: u32,
    pub id: String,
    pub created_at: String,
    pub demo_id: String,
    pub demo_fingerprint: String,
    pub player_id: String,
    pub tick_rate: f64,
    pub ruleset_version: String,
    pub checks: Vec<Check>,
    pub input_provenance: serde_json::Value,
    pub state: State,
}
pub struct Context<'a> {
    pub demo_fingerprint: &'a str,
    pub player_id: &'a str,
    pub measurements: &'a BTreeMap<String, serde_json::Value>,
    pub body_journal: Option<&'a std::fs::File>,
    pub prepared_engagement: Option<&'a [Check]>,
    pub prepared_view: Option<&'a Check>,
    pub prepared_body: Option<Result<&'a crosshair_lock::Report, &'a str>>,
}
pub fn evaluate(context: &Context<'_>) -> Vec<Check> {
    let report = if context.prepared_body.is_none() {
        rules::outcome(context, &crosshair_lock::Parameters::default())
    } else {
        None
    };
    let error = report
        .as_ref()
        .and_then(|r| r.as_ref().err())
        .map(|e| format!("{e:#}"));
    let prepared = Context {
        prepared_body: context.prepared_body.or_else(|| {
            report.as_ref().map(|r| {
                r.as_ref()
                    .map_err(|_| error.as_deref().expect("prepared error"))
            })
        }),
        ..*context
    };
    let mut checks: Vec<_> = rules::ENABLED.iter().map(|rule| rule(&prepared)).collect();
    checks.extend(
        context
            .prepared_engagement
            .map(<[Check]>::to_vec)
            .unwrap_or_else(engagement_context::unavailable),
    );
    checks.push(time_to_damage::unavailable(
        "Qualified body line-of-sight onsets are unavailable.",
    ));
    checks
}

#[cfg(test)]
mod tests;
