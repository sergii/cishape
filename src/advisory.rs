use crate::model::{JobShape, Recommendation, RunnerShape};
use serde::{Deserialize, Serialize};

pub const ADVISORY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdvisoryAction {
    InsufficientEvidence,
    Keep,
    Downsize,
    Upsize,
    Reshape,
    NoCandidate,
}

impl AdvisoryAction {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::InsufficientEvidence => "insufficient_evidence",
            Self::Keep => "keep",
            Self::Downsize => "downsize",
            Self::Upsize => "upsize",
            Self::Reshape => "reshape",
            Self::NoCandidate => "no_candidate",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvisoryItem {
    pub repository: Option<String>,
    pub job: String,
    pub runs: u64,
    pub min_runs: u64,
    pub action: AdvisoryAction,
    pub current_runner: RunnerShape,
    pub recommended_runner: Option<RunnerShape>,
    pub duration_p95_ms: f64,
    pub cpu_peak_p95_millis: f64,
    pub memory_peak_p99_bytes: f64,
    pub algorithm: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvisoryReport {
    pub schema_version: u32,
    pub min_runs: u64,
    pub items: Vec<AdvisoryItem>,
}

pub fn advisory_item(
    profile: &JobShape,
    recommendation: Option<&Recommendation>,
    min_runs: u64,
) -> AdvisoryItem {
    let (action, recommended_runner, algorithm) = if profile.runs < min_runs {
        (
            AdvisoryAction::InsufficientEvidence,
            recommendation.map(|value| value.recommended.shape.clone()),
            recommendation.map(|value| value.algorithm.clone()),
        )
    } else if let Some(recommendation) = recommendation {
        (
            classify(&profile.current_runner, &recommendation.recommended.shape),
            Some(recommendation.recommended.shape.clone()),
            Some(recommendation.algorithm.clone()),
        )
    } else {
        (AdvisoryAction::NoCandidate, None, None)
    };

    AdvisoryItem {
        repository: profile.repository.clone(),
        job: profile.job.clone(),
        runs: profile.runs,
        min_runs,
        action,
        current_runner: profile.current_runner.clone(),
        recommended_runner,
        duration_p95_ms: profile.duration_p95_ms,
        cpu_peak_p95_millis: profile.cpu_peak_p95_millis,
        memory_peak_p99_bytes: profile.memory_peak_p99_bytes,
        algorithm,
    }
}

pub fn classify(current: &RunnerShape, target: &RunnerShape) -> AdvisoryAction {
    if current == target {
        return AdvisoryAction::Keep;
    }

    let cpu_order = target.cpu_millis.cmp(&current.cpu_millis);
    let memory_order = target.memory_bytes.cmp(&current.memory_bytes);

    let non_increasing = !cpu_order.is_gt() && !memory_order.is_gt();
    let non_decreasing = !cpu_order.is_lt() && !memory_order.is_lt();

    if non_increasing {
        AdvisoryAction::Downsize
    } else if non_decreasing {
        AdvisoryAction::Upsize
    } else {
        AdvisoryAction::Reshape
    }
}

pub fn to_markdown(report: &AdvisoryReport) -> String {
    let mut output = String::new();
    output.push_str("# CIShape deterministic advisory\n\n");
    output.push_str(&format!(
        "Minimum evidence: {} runs. Advisory only - no CI execution changes are made.\n\n",
        report.min_runs
    ));
    output.push_str(
        "| Repository | Job | Runs | Action | Current | Target | p95 | CPU p95 | RAM p99 |\n",
    );
    output.push_str("| --- | --- | ---: | --- | --- | --- | ---: | ---: | ---: |\n");

    for item in &report.items {
        let repository = item.repository.as_deref().unwrap_or("local");
        let target = item
            .recommended_runner
            .as_ref()
            .map(RunnerShape::display_id)
            .unwrap_or_else(|| "-".into());

        output.push_str(&format!(
            "| {repository} | {} | {} | {} | {} | {target} | {:.2}s | {:.2} cores | {:.0} MiB |\n",
            item.job,
            item.runs,
            item.action.as_str(),
            item.current_runner.display_id(),
            item.duration_p95_ms / 1000.0,
            item.cpu_peak_p95_millis / 1000.0,
            item.memory_peak_p99_bytes / (1024.0 * 1024.0),
        ));
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::GIB;

    #[test]
    fn classifies_shape_changes_per_dimension() {
        let current = RunnerShape::new(4_000, 8 * GIB);

        assert_eq!(
            classify(&current, &RunnerShape::new(4_000, 8 * GIB)),
            AdvisoryAction::Keep
        );
        assert_eq!(
            classify(&current, &RunnerShape::new(2_000, 4 * GIB)),
            AdvisoryAction::Downsize
        );
        assert_eq!(
            classify(&current, &RunnerShape::new(8_000, 16 * GIB)),
            AdvisoryAction::Upsize
        );
        assert_eq!(
            classify(&current, &RunnerShape::new(8_000, 4 * GIB)),
            AdvisoryAction::Reshape
        );
    }
}
