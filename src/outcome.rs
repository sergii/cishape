use crate::decision::DecisionRecord;
use crate::model::{RunObservation, RunnerShape};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

pub const OUTCOME_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeStatus {
    NotApplied,
    InsufficientEvidence,
    WithinPrediction,
    OutsidePrediction,
    MixedFailures,
}

impl OutcomeStatus {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::NotApplied => "not_applied",
            Self::InsufficientEvidence => "insufficient_evidence",
            Self::WithinPrediction => "within_prediction",
            Self::OutsidePrediction => "outside_prediction",
            Self::MixedFailures => "mixed_failures",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutcomeRecord {
    pub schema_version: u32,
    pub decision_request_id: String,
    pub evaluated_at_unix_ms: u64,
    pub repository: Option<String>,
    pub job: String,
    pub algorithm: String,
    pub baseline_runner: RunnerShape,
    pub selected_runner: RunnerShape,
    pub baseline_evidence_runs: u64,
    pub outcome_min_runs: u64,
    pub matching_runs: u64,
    pub successful_runs: u64,
    pub failed_runs: u64,
    pub baseline_duration_p95_ms: f64,
    pub predicted_p95_ms: f64,
    pub outcome_duration_p50_ms: Option<f64>,
    pub outcome_duration_p95_ms: Option<f64>,
    pub duration_change_percent: Option<f64>,
    pub outcome_cpu_peak_p95_millis: Option<f64>,
    pub outcome_memory_peak_p99_bytes: Option<f64>,
    pub status: OutcomeStatus,
}

pub fn evaluate(
    decision: &DecisionRecord,
    observations: &[RunObservation],
    min_runs: u64,
) -> Result<OutcomeRecord> {
    anyhow::ensure!(min_runs > 0, "outcome min_runs must be positive");

    let baseline_runner = decision
        .baseline_runner
        .clone()
        .context("decision does not contain baseline_runner; regenerate it with DecisionRecord v2")?;
    let selected_runner = decision
        .selected_runner
        .clone()
        .context("decision does not contain selected_runner; regenerate it with DecisionRecord v2")?;
    let baseline_duration_p95_ms = decision
        .baseline_duration_p95_ms
        .context("decision does not contain baseline_duration_p95_ms; regenerate it with DecisionRecord v2")?;
    let predicted_p95_ms = decision
        .predicted_p95_ms
        .context("decision does not contain predicted_p95_ms; regenerate it with DecisionRecord v2")?;

    let matching = observations
        .iter()
        .filter(|run| {
            run.observed_at_unix_ms > decision.recorded_at_unix_ms
                && run.job == decision.job
                && same_repository(run.ci.repository.as_deref(), decision.repository.as_deref())
                && run.runner == selected_runner
        })
        .collect::<Vec<_>>();

    let failed_runs = matching.iter().filter(|run| run.exit_code != 0).count() as u64;
    let successful = matching
        .iter()
        .copied()
        .filter(|run| run.exit_code == 0)
        .collect::<Vec<_>>();

    let outcome_duration_p50_ms = quantile(
        successful
            .iter()
            .map(|run| run.duration_ms as f64)
            .collect(),
        0.50,
    );
    let outcome_duration_p95_ms = quantile(
        successful
            .iter()
            .map(|run| run.duration_ms as f64)
            .collect(),
        0.95,
    );
    let outcome_cpu_peak_p95_millis = quantile(
        successful
            .iter()
            .map(|run| run.cpu_peak_millis as f64)
            .collect(),
        0.95,
    );
    let outcome_memory_peak_p99_bytes = quantile(
        successful
            .iter()
            .map(|run| run.memory_peak_bytes as f64)
            .collect(),
        0.99,
    );

    let duration_change_percent = outcome_duration_p95_ms.map(|value| {
        if baseline_duration_p95_ms > 0.0 {
            (value / baseline_duration_p95_ms - 1.0) * 100.0
        } else {
            0.0
        }
    });

    let matching_runs = matching.len() as u64;
    let successful_runs = successful.len() as u64;
    let status = if matching_runs == 0 {
        OutcomeStatus::NotApplied
    } else if matching_runs < min_runs {
        OutcomeStatus::InsufficientEvidence
    } else if failed_runs > 0 {
        OutcomeStatus::MixedFailures
    } else if outcome_duration_p95_ms.is_some_and(|value| value <= predicted_p95_ms) {
        OutcomeStatus::WithinPrediction
    } else {
        OutcomeStatus::OutsidePrediction
    };

    Ok(OutcomeRecord {
        schema_version: OUTCOME_SCHEMA_VERSION,
        decision_request_id: decision.request_id.clone(),
        evaluated_at_unix_ms: unix_time_ms()?,
        repository: decision.repository.clone(),
        job: decision.job.clone(),
        algorithm: decision.model.clone(),
        baseline_runner,
        selected_runner,
        baseline_evidence_runs: decision.evidence_runs,
        outcome_min_runs: min_runs,
        matching_runs,
        successful_runs,
        failed_runs,
        baseline_duration_p95_ms,
        predicted_p95_ms,
        outcome_duration_p50_ms,
        outcome_duration_p95_ms,
        duration_change_percent,
        outcome_cpu_peak_p95_millis,
        outcome_memory_peak_p99_bytes,
        status,
    })
}

pub fn to_markdown(record: &OutcomeRecord) -> String {
    let repository = record.repository.as_deref().unwrap_or("local");
    let outcome_p95 = record
        .outcome_duration_p95_ms
        .map(|value| format!("{:.2}s", value / 1000.0))
        .unwrap_or_else(|| "-".into());
    let duration_change = record
        .duration_change_percent
        .map(|value| format!("{value:+.1}%"))
        .unwrap_or_else(|| "-".into());
    let cpu_p95 = record
        .outcome_cpu_peak_p95_millis
        .map(|value| format!("{:.2} cores", value / 1000.0))
        .unwrap_or_else(|| "-".into());
    let memory_p99 = record
        .outcome_memory_peak_p99_bytes
        .map(|value| format!("{:.0} MiB", value / (1024.0 * 1024.0)))
        .unwrap_or_else(|| "-".into());

    format!(
        "# CIShape outcome\n\n         Outcome evaluation is observational. It does not claim that the runner caused a workload failure or latency change.\n\n         | Field | Value |\n         | --- | --- |\n         | Repository | {repository} |\n         | Job | {} |\n         | Decision | `{}` |\n         | Status | **{}** |\n         | Baseline runner | {} |\n         | Selected runner | {} |\n         | Baseline evidence | {} runs |\n         | Matching outcome evidence | {} runs |\n         | Successful / failed | {} / {} |\n         | Baseline p95 | {:.2}s |\n         | Predicted p95 ceiling | {:.2}s |\n         | Outcome p95 | {outcome_p95} |\n         | p95 change vs baseline | {duration_change} |\n         | Outcome CPU p95 | {cpu_p95} |\n         | Outcome RAM p99 | {memory_p99} |\n         | Algorithm | `{}` |\n",
        record.job,
        record.decision_request_id,
        record.status.as_str(),
        record.baseline_runner.display_id(),
        record.selected_runner.display_id(),
        record.baseline_evidence_runs,
        record.matching_runs,
        record.successful_runs,
        record.failed_runs,
        record.baseline_duration_p95_ms / 1000.0,
        record.predicted_p95_ms / 1000.0,
        record.algorithm,
    )
}

fn same_repository(observed: Option<&str>, expected: Option<&str>) -> bool {
    observed == expected
}

fn quantile(mut values: Vec<f64>, q: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }

    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
    if values.len() == 1 {
        return values.first().copied();
    }

    let rank = (values.len() - 1) as f64 * q;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;

    if lower == upper {
        return Some(values[lower]);
    }

    let weight = rank - lower as f64;
    Some(values[lower] + (values[upper] - values[lower]) * weight)
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
    use crate::decision::{DecisionMode, DecisionProvider, JevUsage};
    use crate::model::{CiIdentity, GIB, MIB};
    use std::collections::BTreeMap;

    fn decision() -> DecisionRecord {
        DecisionRecord {
            schema_version: 2,
            request_id: "decision-1".into(),
            recorded_at_unix_ms: 1_000,
            mode: DecisionMode::Shadow,
            provider: DecisionProvider::Deterministic,
            repository: Some("acme/api".into()),
            job: "test".into(),
            evidence_runs: 20,
            deterministic_baseline: "cpu2-mem4".into(),
            selected_candidate: "cpu2-mem4".into(),
            baseline_runner: Some(RunnerShape::new(4_000, 8 * GIB)),
            selected_runner: Some(RunnerShape::new(2_000, 4 * GIB)),
            baseline_duration_p95_ms: Some(10_000.0),
            predicted_p95_ms: Some(10_500.0),
            confidence: 1.0,
            probabilities: BTreeMap::from([("cpu2-mem4".into(), 1.0)]),
            model: "deterministic-fit-v1@default-v1:policy-schema-v1".into(),
            usage: JevUsage {
                input_tokens: 0,
                output_tokens: 0,
            },
            agrees_with_baseline: true,
        }
    }

    fn observation(timestamp: u64, duration_ms: u64, exit_code: i32) -> RunObservation {
        RunObservation {
            schema_version: 2,
            job: "test".into(),
            observed_at_unix_ms: timestamp,
            duration_ms,
            cpu_seconds: duration_ms as f64 / 1000.0,
            cpu_peak_millis: 1_400,
            memory_peak_bytes: 900 * MIB,
            read_bytes: 0,
            write_bytes: 0,
            runner: RunnerShape::new(2_000, 4 * GIB),
            ci: CiIdentity {
                repository: Some("acme/api".into()),
                ..CiIdentity::default()
            },
            runner_name: None,
            provider: None,
            provider_runner: None,
            queue_ms: None,
            cost_usd: None,
            exit_code,
        }
    }

    #[test]
    fn zero_matching_runs_means_not_applied() {
        let record = evaluate(&decision(), &[], 3).expect("outcome");
        assert_eq!(record.status, OutcomeStatus::NotApplied);
    }

    #[test]
    fn low_sample_is_insufficient_evidence() {
        let runs = vec![
            observation(2_000, 9_500, 0),
            observation(3_000, 9_700, 0),
        ];
        let record = evaluate(&decision(), &runs, 3).expect("outcome");
        assert_eq!(record.status, OutcomeStatus::InsufficientEvidence);
    }

    #[test]
    fn enough_successful_runs_inside_prediction_validate_latency() {
        let runs = vec![
            observation(2_000, 9_500, 0),
            observation(3_000, 9_700, 0),
            observation(4_000, 9_900, 0),
        ];
        let record = evaluate(&decision(), &runs, 3).expect("outcome");
        assert_eq!(record.status, OutcomeStatus::WithinPrediction);
        assert!(record.outcome_duration_p95_ms.unwrap() <= 10_500.0);
    }

    #[test]
    fn enough_successful_runs_outside_prediction_are_flagged() {
        let runs = vec![
            observation(2_000, 11_000, 0),
            observation(3_000, 11_300, 0),
            observation(4_000, 11_500, 0),
        ];
        let record = evaluate(&decision(), &runs, 3).expect("outcome");
        assert_eq!(record.status, OutcomeStatus::OutsidePrediction);
    }

    #[test]
    fn failed_runs_are_mixed_failures_not_causal_claims() {
        let runs = vec![
            observation(2_000, 9_500, 0),
            observation(3_000, 9_700, 7),
            observation(4_000, 9_900, 0),
        ];
        let record = evaluate(&decision(), &runs, 3).expect("outcome");
        assert_eq!(record.status, OutcomeStatus::MixedFailures);
        assert_eq!(record.failed_runs, 1);
    }
}
