use crate::model::{JobShape, Recommendation, RunnerShape};
use crate::optimize::FeasibleCandidate;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DECISION_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_JEV_MODEL: &str = "jev-latest";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionMode {
    Shadow,
}

impl DecisionMode {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Shadow => "shadow",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionProvider {
    Deterministic,
    Jev,
}

impl DecisionProvider {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Deterministic => "deterministic",
            Self::Jev => "jev",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionCandidate {
    pub id: String,
    pub runner: RunnerShape,
    pub usd_per_minute: f64,
    pub estimated_p95_cost_usd: f64,
    pub cpu_headroom: f64,
    pub memory_headroom: f64,
}

impl From<&FeasibleCandidate> for DecisionCandidate {
    fn from(candidate: &FeasibleCandidate) -> Self {
        Self {
            id: candidate.name.clone(),
            runner: candidate.shape.clone(),
            usd_per_minute: candidate.usd_per_minute,
            estimated_p95_cost_usd: candidate.estimated_p95_cost_usd,
            cpu_headroom: candidate.cpu_headroom,
            memory_headroom: candidate.memory_headroom,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRequest {
    pub schema_version: u32,
    pub request_id: String,
    pub created_at_unix_ms: u64,
    pub mode: DecisionMode,
    pub provider: DecisionProvider,
    pub repository: Option<String>,
    pub job: String,
    pub evidence_runs: u64,
    pub current_runner: RunnerShape,
    pub duration_p95_ms: f64,
    pub cpu_peak_p95_millis: f64,
    pub memory_peak_p99_bytes: f64,
    pub deterministic_baseline: String,
    pub candidates: Vec<DecisionCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JevRequestBundle {
    pub decision: DecisionRequest,
    pub wire: JevSystemOneRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JevSystemOneRequest {
    pub model: String,
    pub state: Value,
    pub questions: BTreeMap<String, JevChoiceQuestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JevChoiceQuestion {
    #[serde(rename = "type")]
    pub question_type: String,
    pub instructions: String,
    pub criteria: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JevSystemOneResponse {
    pub model: String,
    pub answers: BTreeMap<String, JevChoiceAnswer>,
    pub usage: JevUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JevChoiceAnswer {
    #[serde(rename = "type")]
    pub answer_type: String,
    pub choice: String,
    pub confidence: f64,
    pub probabilities: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JevUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub schema_version: u32,
    pub request_id: String,
    pub recorded_at_unix_ms: u64,
    pub mode: DecisionMode,
    pub provider: DecisionProvider,
    pub repository: Option<String>,
    pub job: String,
    pub evidence_runs: u64,
    pub deterministic_baseline: String,
    pub selected_candidate: String,
    pub confidence: f64,
    pub probabilities: BTreeMap<String, f64>,
    pub model: String,
    pub usage: JevUsage,
    pub agrees_with_baseline: bool,
}

pub fn record_deterministic_decision(
    profile: &JobShape,
    recommendation: &Recommendation,
    feasible: &[FeasibleCandidate],
) -> Result<DecisionRecord> {
    anyhow::ensure!(!feasible.is_empty(), "no feasible runner candidates");
    anyhow::ensure!(
        feasible
            .iter()
            .any(|candidate| candidate.name == recommendation.recommended.name),
        "deterministic recommendation is not in the feasible candidate set"
    );

    let selected = recommendation.recommended.name.clone();
    let probabilities = feasible
        .iter()
        .map(|candidate| {
            (
                candidate.name.clone(),
                if candidate.name == selected { 1.0 } else { 0.0 },
            )
        })
        .collect::<BTreeMap<_, _>>();

    Ok(DecisionRecord {
        schema_version: DECISION_SCHEMA_VERSION,
        request_id: deterministic_request_id(profile, recommendation),
        recorded_at_unix_ms: unix_time_ms()?,
        mode: DecisionMode::Shadow,
        provider: DecisionProvider::Deterministic,
        repository: profile.repository.clone(),
        job: profile.job.clone(),
        evidence_runs: profile.runs,
        deterministic_baseline: selected.clone(),
        selected_candidate: selected,
        confidence: 1.0,
        probabilities,
        model: recommendation.algorithm.clone(),
        usage: JevUsage {
            input_tokens: 0,
            output_tokens: 0,
        },
        agrees_with_baseline: true,
    })
}

fn deterministic_request_id(profile: &JobShape, recommendation: &Recommendation) -> String {
    let repository = profile.repository.as_deref().unwrap_or("local");
    format!(
        "deterministic:{repository}:{}:{}:{:.0}:{:.0}:{:.0}:{}",
        profile.job,
        profile.runs,
        profile.duration_p95_ms,
        profile.cpu_peak_p95_millis,
        profile.memory_peak_p99_bytes,
        recommendation.algorithm
    )
}

pub fn prepare_jev_request(
    profile: &JobShape,
    recommendation: &Recommendation,
    feasible: &[FeasibleCandidate],
    model: Option<&str>,
) -> Result<JevRequestBundle> {
    anyhow::ensure!(!feasible.is_empty(), "no feasible runner candidates");
    anyhow::ensure!(
        feasible
            .iter()
            .any(|candidate| candidate.name == recommendation.recommended.name),
        "deterministic recommendation is not in the feasible candidate set"
    );

    let created_at_unix_ms = unix_time_ms()?;
    let repository = profile.repository.as_deref().unwrap_or("local");
    let request_id = format!(
        "jev-shadow:{repository}:{}:{}:{created_at_unix_ms}",
        profile.job, profile.runs
    );

    let candidates = feasible
        .iter()
        .map(DecisionCandidate::from)
        .collect::<Vec<_>>();

    let decision = DecisionRequest {
        schema_version: DECISION_SCHEMA_VERSION,
        request_id,
        created_at_unix_ms,
        mode: DecisionMode::Shadow,
        provider: DecisionProvider::Jev,
        repository: profile.repository.clone(),
        job: profile.job.clone(),
        evidence_runs: profile.runs,
        current_runner: profile.current_runner.clone(),
        duration_p95_ms: profile.duration_p95_ms,
        cpu_peak_p95_millis: profile.cpu_peak_p95_millis,
        memory_peak_p99_bytes: profile.memory_peak_p99_bytes,
        deterministic_baseline: recommendation.recommended.name.clone(),
        candidates,
    };

    let mut criteria = BTreeMap::new();
    for candidate in &decision.candidates {
        criteria.insert(
            candidate.id.clone(),
            json!({
                "runner": candidate.runner.display_id(),
                "usd_per_minute": candidate.usd_per_minute,
                "estimated_p95_cost_usd": candidate.estimated_p95_cost_usd,
                "cpu_headroom": candidate.cpu_headroom,
                "memory_headroom": candidate.memory_headroom
            }),
        );
    }

    let mut questions = BTreeMap::new();
    questions.insert(
        "runner_choice".into(),
        JevChoiceQuestion {
            question_type: "choice".into(),
            instructions: "Choose the most appropriate runner candidate for this CI workload in shadow mode. Every option already satisfies CIShape hard resource constraints. Prefer lower estimated cost unless materially more CPU or memory headroom is justified by the observed workload. Select only one supplied candidate.".into(),
            criteria,
        },
    );

    let state = json!({
        "workload": {
            "repository": decision.repository,
            "job": decision.job,
            "history_runs": decision.evidence_runs
        },
        "observed_shape": {
            "current_runner": decision.current_runner.display_id(),
            "duration_p95_ms": decision.duration_p95_ms,
            "cpu_peak_p95_millis": decision.cpu_peak_p95_millis,
            "memory_peak_p99_bytes": decision.memory_peak_p99_bytes
        },
        "deterministic_baseline": decision.deterministic_baseline,
        "policy": {
            "mode": "shadow",
            "hard_constraints_already_applied": true,
            "execution_side_effects_allowed": false
        }
    });

    Ok(JevRequestBundle {
        decision,
        wire: JevSystemOneRequest {
            model: model.unwrap_or(DEFAULT_JEV_MODEL).to_string(),
            state,
            questions,
        },
    })
}

pub fn record_jev_response(
    bundle: &JevRequestBundle,
    response: JevSystemOneResponse,
) -> Result<DecisionRecord> {
    let answer = response
        .answers
        .get("runner_choice")
        .context("Jev response is missing runner_choice")?;

    anyhow::ensure!(
        answer.answer_type == "choice",
        "runner_choice answer must have type choice"
    );
    anyhow::ensure!(
        (0.0..=1.0).contains(&answer.confidence),
        "Jev confidence must be between 0 and 1"
    );

    let candidate_ids = bundle
        .decision
        .candidates
        .iter()
        .map(|candidate| candidate.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    anyhow::ensure!(
        candidate_ids.contains(answer.choice.as_str()),
        "Jev selected candidate {} outside the feasible set",
        answer.choice
    );

    for (candidate, probability) in &answer.probabilities {
        anyhow::ensure!(
            candidate_ids.contains(candidate.as_str()),
            "Jev returned probability for unknown candidate {candidate}"
        );
        anyhow::ensure!(
            (0.0..=1.0).contains(probability),
            "Jev probability for {candidate} must be between 0 and 1"
        );
    }

    Ok(DecisionRecord {
        schema_version: DECISION_SCHEMA_VERSION,
        request_id: bundle.decision.request_id.clone(),
        recorded_at_unix_ms: unix_time_ms()?,
        mode: DecisionMode::Shadow,
        provider: DecisionProvider::Jev,
        repository: bundle.decision.repository.clone(),
        job: bundle.decision.job.clone(),
        evidence_runs: bundle.decision.evidence_runs,
        deterministic_baseline: bundle.decision.deterministic_baseline.clone(),
        selected_candidate: answer.choice.clone(),
        confidence: answer.confidence,
        probabilities: answer.probabilities.clone(),
        model: response.model,
        usage: response.usage,
        agrees_with_baseline: answer.choice == bundle.decision.deterministic_baseline,
    })
}

fn unix_time_ms() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before Unix epoch")?
        .as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GIB, JobShape, MIB};
    use crate::optimize::{default_catalog, feasible_candidates, recommend};

    fn profile() -> JobShape {
        JobShape {
            job: "lint".into(),
            repository: Some("acme/api".into()),
            runs: 42,
            duration_p50_ms: 9_000.0,
            duration_p95_ms: 11_000.0,
            cpu_peak_p95_millis: 850.0,
            memory_peak_p99_bytes: 800.0 * MIB as f64,
            current_runner: RunnerShape::new(16_000, 64 * GIB),
        }
    }

    #[test]
    fn deterministic_decision_matches_baseline_and_is_stable() {
        let profile = profile();
        let catalog = default_catalog();
        let feasible = feasible_candidates(&profile, &catalog);
        let recommendation = recommend(&profile, &catalog).expect("recommendation");

        let first =
            record_deterministic_decision(&profile, &recommendation, &feasible).expect("decision");
        let second =
            record_deterministic_decision(&profile, &recommendation, &feasible).expect("decision");

        assert_eq!(first.provider, DecisionProvider::Deterministic);
        assert_eq!(first.selected_candidate, recommendation.recommended.name);
        assert!(first.agrees_with_baseline);
        assert_eq!(first.confidence, 1.0);
        assert_eq!(first.request_id, second.request_id);
        assert_eq!(
            first.probabilities[&first.selected_candidate],
            1.0
        );
    }

    #[test]
    fn request_contains_only_feasible_candidates() {
        let profile = profile();
        let catalog = default_catalog();
        let feasible = feasible_candidates(&profile, &catalog);
        let recommendation = recommend(&profile, &catalog).expect("recommendation");
        let bundle =
            prepare_jev_request(&profile, &recommendation, &feasible, None).expect("request");

        let criteria = &bundle.wire.questions["runner_choice"].criteria;
        assert!(criteria.contains_key("cpu2-mem4"));
        assert!(!criteria.contains_key("cpu1-mem2"));
        assert_eq!(
            bundle.decision.deterministic_baseline,
            recommendation.recommended.name
        );
    }

    #[test]
    fn valid_response_becomes_shadow_decision_record() {
        let profile = profile();
        let catalog = default_catalog();
        let feasible = feasible_candidates(&profile, &catalog);
        let recommendation = recommend(&profile, &catalog).expect("recommendation");
        let bundle =
            prepare_jev_request(&profile, &recommendation, &feasible, None).expect("request");

        let response = JevSystemOneResponse {
            model: "jev-1.13.0".into(),
            answers: BTreeMap::from([(
                "runner_choice".into(),
                JevChoiceAnswer {
                    answer_type: "choice".into(),
                    choice: "cpu4-mem8".into(),
                    confidence: 0.74,
                    probabilities: BTreeMap::from([
                        ("cpu2-mem4".into(), 0.25),
                        ("cpu4-mem8".into(), 0.74),
                        ("cpu8-mem16".into(), 0.01),
                        ("cpu16-mem64".into(), 0.0),
                    ]),
                },
            )]),
            usage: JevUsage {
                input_tokens: 321,
                output_tokens: 12,
            },
        };

        let record = record_jev_response(&bundle, response).expect("record");
        assert_eq!(record.selected_candidate, "cpu4-mem8");
        assert!(!record.agrees_with_baseline);
        assert_eq!(record.model, "jev-1.13.0");
    }

    #[test]
    fn response_outside_feasible_set_fails_closed() {
        let profile = profile();
        let catalog = default_catalog();
        let feasible = feasible_candidates(&profile, &catalog);
        let recommendation = recommend(&profile, &catalog).expect("recommendation");
        let bundle =
            prepare_jev_request(&profile, &recommendation, &feasible, None).expect("request");

        let response = JevSystemOneResponse {
            model: "jev-latest".into(),
            answers: BTreeMap::from([(
                "runner_choice".into(),
                JevChoiceAnswer {
                    answer_type: "choice".into(),
                    choice: "cpu1-mem2".into(),
                    confidence: 0.99,
                    probabilities: BTreeMap::from([("cpu1-mem2".into(), 0.99)]),
                },
            )]),
            usage: JevUsage {
                input_tokens: 1,
                output_tokens: 1,
            },
        };

        let error = record_jev_response(&bundle, response).expect_err("must fail closed");
        assert!(error.to_string().contains("outside the feasible set"));
    }
}
