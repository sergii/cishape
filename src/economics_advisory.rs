use crate::catalog::ProviderCatalog;
use crate::economics::{CapacitySnapshot, EconomicsReport, evaluate as evaluate_economics};
use crate::economics_policy::{EconomicsPolicy, EconomicsSelection, select as select_economics};
use crate::model::{JobShape, RunnerShape};
use crate::optimize::{default_catalog, recommend_with_policy};
use crate::policy::OptimizationPolicy;
use anyhow::Result;
use serde::{Deserialize, Serialize};

pub const ECONOMICS_ADVISORY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EconomicsAdvisoryStatus {
    InsufficientEvidence,
    NoRunnerCandidate,
    NoEligibleOffer,
    Selected,
}

impl EconomicsAdvisoryStatus {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::InsufficientEvidence => "insufficient_evidence",
            Self::NoRunnerCandidate => "no_runner_candidate",
            Self::NoEligibleOffer => "no_eligible_offer",
            Self::Selected => "selected",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicsAdvisoryItem {
    pub repository: Option<String>,
    pub job: String,
    pub runs: u64,
    pub min_runs: u64,
    pub status: EconomicsAdvisoryStatus,
    pub current_runner: RunnerShape,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_runner: Option<RunnerShape>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predicted_p95_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optimization_algorithm: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub economics: Option<EconomicsReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<EconomicsSelection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicsAdvisoryReport {
    pub schema_version: u32,
    pub optimization_policy_id: String,
    pub optimization_policy_schema_version: u32,
    pub economics_policy_id: String,
    pub economics_policy_schema_version: u32,
    pub min_runs: u64,
    pub catalog_schema_version: u32,
    pub catalog_observed_at: String,
    pub snapshot_observed_at: String,
    pub snapshot_source: String,
    pub items: Vec<EconomicsAdvisoryItem>,
}

pub fn build_report(
    profiles: &[JobShape],
    optimization_policy: &OptimizationPolicy,
    catalog: &ProviderCatalog,
    snapshot: &CapacitySnapshot,
    economics_policy: &EconomicsPolicy,
) -> Result<EconomicsAdvisoryReport> {
    optimization_policy.validate()?;
    catalog.validate()?;
    snapshot.validate()?;
    economics_policy.validate()?;

    let mut items = Vec::with_capacity(profiles.len());
    for profile in profiles {
        items.push(build_item(
            profile,
            optimization_policy,
            catalog,
            snapshot,
            economics_policy,
        )?);
    }

    items.sort_by(|left, right| {
        left.repository
            .cmp(&right.repository)
            .then_with(|| left.job.cmp(&right.job))
    });

    Ok(EconomicsAdvisoryReport {
        schema_version: ECONOMICS_ADVISORY_SCHEMA_VERSION,
        optimization_policy_id: optimization_policy.policy_id.clone(),
        optimization_policy_schema_version: optimization_policy.schema_version,
        economics_policy_id: economics_policy.policy_id.clone(),
        economics_policy_schema_version: economics_policy.schema_version,
        min_runs: optimization_policy.min_runs,
        catalog_schema_version: catalog.schema_version,
        catalog_observed_at: catalog.observed_at.clone(),
        snapshot_observed_at: snapshot.observed_at.clone(),
        snapshot_source: snapshot.source.clone(),
        items,
    })
}

pub fn build_item(
    profile: &JobShape,
    optimization_policy: &OptimizationPolicy,
    catalog: &ProviderCatalog,
    snapshot: &CapacitySnapshot,
    economics_policy: &EconomicsPolicy,
) -> Result<EconomicsAdvisoryItem> {
    if profile.runs < optimization_policy.min_runs {
        return Ok(base_item(
            profile,
            optimization_policy.min_runs,
            EconomicsAdvisoryStatus::InsufficientEvidence,
        ));
    }

    let Some(recommendation) =
        recommend_with_policy(profile, &default_catalog(), optimization_policy)
    else {
        return Ok(base_item(
            profile,
            optimization_policy.min_runs,
            EconomicsAdvisoryStatus::NoRunnerCandidate,
        ));
    };

    anyhow::ensure!(
        recommendation.predicted_p95_ms.is_finite()
            && recommendation.predicted_p95_ms > 0.0
            && recommendation.predicted_p95_ms <= u64::MAX as f64,
        "predicted p95 for {} is outside supported duration range",
        profile.job
    );
    let predicted_p95_ms = recommendation.predicted_p95_ms.ceil() as u64;
    let economics = evaluate_economics(
        catalog,
        snapshot,
        &recommendation.recommended.shape,
        predicted_p95_ms,
    )?;
    let selection = select_economics(&economics, economics_policy)?;
    let status = if selection.selected.is_some() {
        EconomicsAdvisoryStatus::Selected
    } else {
        EconomicsAdvisoryStatus::NoEligibleOffer
    };

    Ok(EconomicsAdvisoryItem {
        repository: profile.repository.clone(),
        job: profile.job.clone(),
        runs: profile.runs,
        min_runs: optimization_policy.min_runs,
        status,
        current_runner: profile.current_runner.clone(),
        target_runner: Some(recommendation.recommended.shape),
        predicted_p95_ms: Some(predicted_p95_ms),
        optimization_algorithm: Some(recommendation.algorithm),
        economics: Some(economics),
        selection: Some(selection),
    })
}

fn base_item(
    profile: &JobShape,
    min_runs: u64,
    status: EconomicsAdvisoryStatus,
) -> EconomicsAdvisoryItem {
    EconomicsAdvisoryItem {
        repository: profile.repository.clone(),
        job: profile.job.clone(),
        runs: profile.runs,
        min_runs,
        status,
        current_runner: profile.current_runner.clone(),
        target_runner: None,
        predicted_p95_ms: None,
        optimization_algorithm: None,
        economics: None,
        selection: None,
    }
}

pub fn to_markdown(report: &EconomicsAdvisoryReport) -> String {
    let mut output = String::new();
    output.push_str("# CIShape economics-aware advisory\n\n");
    output.push_str(&format!(
        "- Optimization policy: `{}` (schema v{})\n",
        report.optimization_policy_id, report.optimization_policy_schema_version
    ));
    output.push_str(&format!(
        "- Economics policy: `{}` (schema v{})\n",
        report.economics_policy_id, report.economics_policy_schema_version
    ));
    output.push_str(&format!(
        "- Provider catalog: schema v{}, observed `{}`\n",
        report.catalog_schema_version, report.catalog_observed_at
    ));
    output.push_str(&format!(
        "- Capacity snapshot: `{}` from `{}`\n",
        report.snapshot_observed_at, report.snapshot_source
    ));
    output.push_str(&format!("- Minimum evidence: {} runs\n", report.min_runs));
    output.push_str("- Advisory only - no CI execution changes are made.\n\n");

    output.push_str(
        "| Repository | Job | Runs | Status | Current | Target | Predicted p95 | Selected offer | Effective cost | Time-to-green |\n",
    );
    output.push_str("| --- | --- | ---: | --- | --- | --- | ---: | --- | ---: | ---: |\n");

    for item in &report.items {
        let repository = item.repository.as_deref().unwrap_or("local");
        let target = item
            .target_runner
            .as_ref()
            .map(RunnerShape::display_id)
            .unwrap_or_else(|| "-".into());
        let predicted = item
            .predicted_p95_ms
            .map(|value| format!("{:.2}s", value as f64 / 1000.0))
            .unwrap_or_else(|| "-".into());
        let selected = item
            .selection
            .as_ref()
            .and_then(|selection| selection.selected.as_ref());
        let selected_offer = selected
            .map(|value| format!("{}/{}", value.provider, value.offer_id))
            .unwrap_or_else(|| "-".into());
        let cost = selected
            .map(|value| format!("${:.6}", value.effective_cost_usd))
            .unwrap_or_else(|| "-".into());
        let time_to_green = selected
            .map(|value| format!("{:.2}s", value.time_to_green_ms as f64 / 1000.0))
            .unwrap_or_else(|| "-".into());

        output.push_str(&format!(
            "| {repository} | {} | {} | {} | {} | {target} | {predicted} | {selected_offer} | {cost} | {time_to_green} |\n",
            item.job,
            item.runs,
            item.status.as_str(),
            item.current_runner.display_id(),
        ));
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economics::CapacitySnapshot;
    use crate::model::{GIB, MIB};
    use std::path::PathBuf;

    fn repo_path(path: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
    }

    fn profile(runs: u64) -> JobShape {
        JobShape {
            job: "lint".into(),
            repository: Some("acme/api".into()),
            runs,
            duration_p50_ms: 9_000.0,
            duration_p95_ms: 11_000.0,
            cpu_peak_p95_millis: 850.0,
            memory_peak_p99_bytes: 800.0 * MIB as f64,
            current_runner: RunnerShape::new(4_000, 8 * GIB),
        }
    }

    #[test]
    fn selects_provider_from_history_derived_target() {
        let catalog =
            ProviderCatalog::load(&repo_path("catalogs/providers-v1.json")).expect("catalog");
        let snapshot = CapacitySnapshot::load(&repo_path("examples/capacity-snapshot-v1.json"))
            .expect("snapshot");
        let item = build_item(
            &profile(20),
            &OptimizationPolicy::default_v1(),
            &catalog,
            &snapshot,
            &EconomicsPolicy::default_v1(),
        )
        .expect("advisory");

        assert_eq!(item.status, EconomicsAdvisoryStatus::Selected);
        assert_eq!(item.target_runner, Some(RunnerShape::new(2_000, 4 * GIB)));
        let selected = item
            .selection
            .as_ref()
            .and_then(|selection| selection.selected.as_ref())
            .expect("selected");
        assert_eq!(selected.provider, "depot");
    }

    #[test]
    fn insufficient_history_never_selects_provider() {
        let catalog =
            ProviderCatalog::load(&repo_path("catalogs/providers-v1.json")).expect("catalog");
        let snapshot = CapacitySnapshot::load(&repo_path("examples/capacity-snapshot-v1.json"))
            .expect("snapshot");
        let item = build_item(
            &profile(5),
            &OptimizationPolicy::default_v1(),
            &catalog,
            &snapshot,
            &EconomicsPolicy::default_v1(),
        )
        .expect("advisory");

        assert_eq!(item.status, EconomicsAdvisoryStatus::InsufficientEvidence);
        assert!(item.target_runner.is_none());
        assert!(item.economics.is_none());
        assert!(item.selection.is_none());
    }
}
