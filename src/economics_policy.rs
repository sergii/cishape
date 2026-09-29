use crate::batch::{
    BatchEconomicsEvaluation, BatchEconomicsReport, to_markdown as batch_to_markdown,
};
use crate::economics::{
    EconomicsEvaluation, EconomicsReport, to_markdown as economics_to_markdown,
};
use crate::workflow::{
    WorkflowEconomicsEvaluation, WorkflowEconomicsReport, to_markdown as workflow_to_markdown,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::path::Path;

pub const ECONOMICS_POLICY_SCHEMA_VERSION: u32 = 1;
pub const ECONOMICS_SELECTION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EconomicsObjective {
    MinimizeEffectiveCost,
    MinimizeTimeToGreen,
}

impl std::fmt::Display for EconomicsObjective {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MinimizeEffectiveCost => write!(formatter, "minimize_effective_cost"),
            Self::MinimizeTimeToGreen => write!(formatter, "minimize_time_to_green"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicsPolicy {
    pub schema_version: u32,
    pub policy_id: String,
    pub objective: EconomicsObjective,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_time_to_green_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_effective_cost_usd: Option<f64>,
}

impl EconomicsPolicy {
    pub fn default_v1() -> Self {
        Self {
            schema_version: ECONOMICS_POLICY_SCHEMA_VERSION,
            policy_id: "economics-default-v1".into(),
            objective: EconomicsObjective::MinimizeEffectiveCost,
            max_time_to_green_ms: Some(30_000),
            max_effective_cost_usd: None,
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("read economics policy {}", path.display()))?;
        let policy: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse economics policy {}", path.display()))?;
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.schema_version == ECONOMICS_POLICY_SCHEMA_VERSION,
            "unsupported economics policy schema version {}",
            self.schema_version
        );
        anyhow::ensure!(!self.policy_id.trim().is_empty(), "policy_id is required");
        if let Some(limit) = self.max_time_to_green_ms {
            anyhow::ensure!(
                limit > 0,
                "max_time_to_green_ms must be positive when present"
            );
        }
        if let Some(limit) = self.max_effective_cost_usd {
            anyhow::ensure!(
                limit.is_finite() && limit > 0.0,
                "max_effective_cost_usd must be finite and positive when present"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicsExclusion {
    pub provider: String,
    pub offer_id: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedEconomicsOffer {
    pub provider: String,
    pub offer_id: String,
    pub effective_cost_usd: f64,
    pub time_to_green_ms: u64,
    pub pareto_optimal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicsSelection {
    pub schema_version: u32,
    pub policy_id: String,
    pub policy_schema_version: u32,
    pub objective: EconomicsObjective,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_time_to_green_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_effective_cost_usd: Option<f64>,
    pub eligible_candidates: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<SelectedEconomicsOffer>,
    pub excluded: Vec<EconomicsExclusion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicsDecisionReport {
    pub economics: EconomicsReport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch: Option<BatchEconomicsReport>,
    pub selection: EconomicsSelection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEconomicsDecisionReport {
    pub workflow: WorkflowEconomicsReport,
    pub selection: EconomicsSelection,
}

pub fn select(report: &EconomicsReport, policy: &EconomicsPolicy) -> Result<EconomicsSelection> {
    policy.validate()?;
    let mut eligible = Vec::new();
    let mut excluded = Vec::new();

    for evaluation in &report.evaluations {
        let reasons = exclusion_reasons(evaluation, policy);
        if reasons.is_empty() {
            eligible.push(evaluation);
        } else {
            excluded.push(EconomicsExclusion {
                provider: evaluation.provider.clone(),
                offer_id: evaluation.offer_id.clone(),
                reasons,
            });
        }
    }

    eligible.sort_by(|left, right| compare(left, right, &policy.objective));
    excluded.sort_by(|left, right| {
        left.provider
            .cmp(&right.provider)
            .then_with(|| left.offer_id.cmp(&right.offer_id))
    });

    let selected = eligible.first().map(|evaluation| SelectedEconomicsOffer {
        provider: evaluation.provider.clone(),
        offer_id: evaluation.offer_id.clone(),
        effective_cost_usd: evaluation.effective_cost_usd,
        time_to_green_ms: evaluation.time_to_green_ms,
        pareto_optimal: evaluation.pareto_optimal,
    });

    Ok(EconomicsSelection {
        schema_version: ECONOMICS_SELECTION_SCHEMA_VERSION,
        policy_id: policy.policy_id.clone(),
        policy_schema_version: policy.schema_version,
        objective: policy.objective.clone(),
        max_time_to_green_ms: policy.max_time_to_green_ms,
        max_effective_cost_usd: policy.max_effective_cost_usd,
        eligible_candidates: eligible.len(),
        selected,
        excluded,
    })
}

pub fn select_batch(
    report: &BatchEconomicsReport,
    policy: &EconomicsPolicy,
) -> Result<EconomicsSelection> {
    policy.validate()?;
    let mut eligible = Vec::new();
    let mut excluded = Vec::new();

    for evaluation in &report.evaluations {
        let reasons = batch_exclusion_reasons(evaluation, policy);
        if reasons.is_empty() {
            eligible.push(evaluation);
        } else {
            excluded.push(EconomicsExclusion {
                provider: evaluation.provider.clone(),
                offer_id: evaluation.offer_id.clone(),
                reasons,
            });
        }
    }

    eligible.sort_by(|left, right| compare_batch(left, right, &policy.objective));
    excluded.sort_by(|left, right| {
        left.provider
            .cmp(&right.provider)
            .then_with(|| left.offer_id.cmp(&right.offer_id))
    });

    let selected = eligible.first().map(|evaluation| SelectedEconomicsOffer {
        provider: evaluation.provider.clone(),
        offer_id: evaluation.offer_id.clone(),
        effective_cost_usd: evaluation.effective_cost_usd,
        time_to_green_ms: evaluation.time_to_green_ms,
        pareto_optimal: evaluation.pareto_optimal,
    });

    Ok(EconomicsSelection {
        schema_version: ECONOMICS_SELECTION_SCHEMA_VERSION,
        policy_id: policy.policy_id.clone(),
        policy_schema_version: policy.schema_version,
        objective: policy.objective.clone(),
        max_time_to_green_ms: policy.max_time_to_green_ms,
        max_effective_cost_usd: policy.max_effective_cost_usd,
        eligible_candidates: eligible.len(),
        selected,
        excluded,
    })
}

pub fn select_workflow(
    report: &WorkflowEconomicsReport,
    policy: &EconomicsPolicy,
) -> Result<EconomicsSelection> {
    policy.validate()?;
    let mut eligible = Vec::new();
    let mut excluded = Vec::new();

    for evaluation in &report.evaluations {
        let reasons = workflow_exclusion_reasons(evaluation, policy);
        if reasons.is_empty() {
            eligible.push(evaluation);
        } else {
            excluded.push(EconomicsExclusion {
                provider: evaluation.provider.clone(),
                offer_id: evaluation.offer_id.clone(),
                reasons,
            });
        }
    }

    eligible.sort_by(|left, right| compare_workflow(left, right, &policy.objective));
    excluded.sort_by(|left, right| {
        left.provider
            .cmp(&right.provider)
            .then_with(|| left.offer_id.cmp(&right.offer_id))
    });

    let selected = eligible.first().map(|evaluation| SelectedEconomicsOffer {
        provider: evaluation.provider.clone(),
        offer_id: evaluation.offer_id.clone(),
        effective_cost_usd: evaluation.effective_cost_usd,
        time_to_green_ms: evaluation.time_to_green_ms,
        pareto_optimal: evaluation.pareto_optimal,
    });

    Ok(EconomicsSelection {
        schema_version: ECONOMICS_SELECTION_SCHEMA_VERSION,
        policy_id: policy.policy_id.clone(),
        policy_schema_version: policy.schema_version,
        objective: policy.objective.clone(),
        max_time_to_green_ms: policy.max_time_to_green_ms,
        max_effective_cost_usd: policy.max_effective_cost_usd,
        eligible_candidates: eligible.len(),
        selected,
        excluded,
    })
}

fn exclusion_reasons(evaluation: &EconomicsEvaluation, policy: &EconomicsPolicy) -> Vec<String> {
    let mut reasons = Vec::new();

    if let Some(limit) = policy.max_time_to_green_ms
        && evaluation.time_to_green_ms > limit
    {
        reasons.push(format!(
            "time_to_green_ms {} exceeds policy limit {}",
            evaluation.time_to_green_ms, limit
        ));
    }

    if let Some(limit) = policy.max_effective_cost_usd
        && evaluation.effective_cost_usd > limit
    {
        reasons.push(format!(
            "effective_cost_usd {:.6} exceeds policy limit {:.6}",
            evaluation.effective_cost_usd, limit
        ));
    }

    reasons
}

fn compare(
    left: &EconomicsEvaluation,
    right: &EconomicsEvaluation,
    objective: &EconomicsObjective,
) -> Ordering {
    match objective {
        EconomicsObjective::MinimizeEffectiveCost => left
            .effective_cost_usd
            .partial_cmp(&right.effective_cost_usd)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.time_to_green_ms.cmp(&right.time_to_green_ms))
            .then_with(|| left.provider.cmp(&right.provider))
            .then_with(|| left.offer_id.cmp(&right.offer_id)),
        EconomicsObjective::MinimizeTimeToGreen => left
            .time_to_green_ms
            .cmp(&right.time_to_green_ms)
            .then_with(|| {
                left.effective_cost_usd
                    .partial_cmp(&right.effective_cost_usd)
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| left.provider.cmp(&right.provider))
            .then_with(|| left.offer_id.cmp(&right.offer_id)),
    }
}

fn batch_exclusion_reasons(
    evaluation: &BatchEconomicsEvaluation,
    policy: &EconomicsPolicy,
) -> Vec<String> {
    let mut reasons = Vec::new();

    if let Some(limit) = policy.max_time_to_green_ms
        && evaluation.time_to_green_ms > limit
    {
        reasons.push(format!(
            "time_to_green_ms {} exceeds policy limit {}",
            evaluation.time_to_green_ms, limit
        ));
    }

    if let Some(limit) = policy.max_effective_cost_usd
        && evaluation.effective_cost_usd > limit
    {
        reasons.push(format!(
            "effective_cost_usd {:.6} exceeds policy limit {:.6}",
            evaluation.effective_cost_usd, limit
        ));
    }

    reasons
}

fn compare_batch(
    left: &BatchEconomicsEvaluation,
    right: &BatchEconomicsEvaluation,
    objective: &EconomicsObjective,
) -> Ordering {
    match objective {
        EconomicsObjective::MinimizeEffectiveCost => left
            .effective_cost_usd
            .partial_cmp(&right.effective_cost_usd)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.time_to_green_ms.cmp(&right.time_to_green_ms))
            .then_with(|| left.provider.cmp(&right.provider))
            .then_with(|| left.offer_id.cmp(&right.offer_id)),
        EconomicsObjective::MinimizeTimeToGreen => left
            .time_to_green_ms
            .cmp(&right.time_to_green_ms)
            .then_with(|| {
                left.effective_cost_usd
                    .partial_cmp(&right.effective_cost_usd)
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| left.provider.cmp(&right.provider))
            .then_with(|| left.offer_id.cmp(&right.offer_id)),
    }
}

fn workflow_exclusion_reasons(
    evaluation: &WorkflowEconomicsEvaluation,
    policy: &EconomicsPolicy,
) -> Vec<String> {
    let mut reasons = Vec::new();

    if let Some(limit) = policy.max_time_to_green_ms
        && evaluation.time_to_green_ms > limit
    {
        reasons.push(format!(
            "time_to_green_ms {} exceeds policy limit {}",
            evaluation.time_to_green_ms, limit
        ));
    }

    if let Some(limit) = policy.max_effective_cost_usd
        && evaluation.effective_cost_usd > limit
    {
        reasons.push(format!(
            "effective_cost_usd {:.6} exceeds policy limit {:.6}",
            evaluation.effective_cost_usd, limit
        ));
    }

    reasons
}

fn compare_workflow(
    left: &WorkflowEconomicsEvaluation,
    right: &WorkflowEconomicsEvaluation,
    objective: &EconomicsObjective,
) -> Ordering {
    match objective {
        EconomicsObjective::MinimizeEffectiveCost => left
            .effective_cost_usd
            .partial_cmp(&right.effective_cost_usd)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.time_to_green_ms.cmp(&right.time_to_green_ms))
            .then_with(|| left.provider.cmp(&right.provider))
            .then_with(|| left.offer_id.cmp(&right.offer_id)),
        EconomicsObjective::MinimizeTimeToGreen => left
            .time_to_green_ms
            .cmp(&right.time_to_green_ms)
            .then_with(|| {
                left.effective_cost_usd
                    .partial_cmp(&right.effective_cost_usd)
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| left.provider.cmp(&right.provider))
            .then_with(|| left.offer_id.cmp(&right.offer_id)),
    }
}

pub fn to_markdown(report: &EconomicsDecisionReport) -> String {
    let mut output = economics_to_markdown(&report.economics);
    if let Some(batch) = &report.batch {
        output.push('\n');
        output.push_str(&batch_to_markdown(batch));
    }
    output.push_str("\n## Deterministic selection\n\n");
    output.push_str(&format!("- Policy: `{}`\n", report.selection.policy_id));
    output.push_str(&format!("- Objective: `{}`\n", report.selection.objective));

    if let Some(limit) = report.selection.max_time_to_green_ms {
        output.push_str(&format!(
            "- Max time-to-green: {:.2}s\n",
            limit as f64 / 1000.0
        ));
    }
    if let Some(limit) = report.selection.max_effective_cost_usd {
        output.push_str(&format!("- Max effective cost: ${limit:.6}\n"));
    }

    output.push_str(&format!(
        "- Eligible candidates: {}\n",
        report.selection.eligible_candidates
    ));

    match &report.selection.selected {
        Some(selected) => output.push_str(&format!(
            "- Selected: `{}/{}` at ${:.6}, {:.2}s time-to-green\n",
            selected.provider,
            selected.offer_id,
            selected.effective_cost_usd,
            selected.time_to_green_ms as f64 / 1000.0
        )),
        None => output.push_str("- Selected: none\n"),
    }

    if !report.selection.excluded.is_empty() {
        output.push_str("\n### Policy exclusions\n\n");
        for excluded in &report.selection.excluded {
            output.push_str(&format!(
                "- `{}/{}`: {}\n",
                excluded.provider,
                excluded.offer_id,
                excluded.reasons.join("; ")
            ));
        }
    }

    output
}

pub fn workflow_decision_to_markdown(report: &WorkflowEconomicsDecisionReport) -> String {
    let mut output = workflow_to_markdown(&report.workflow);
    output.push_str("\n## Deterministic selection\n\n");
    output.push_str(&format!("- Policy: `{}`\n", report.selection.policy_id));
    output.push_str(&format!("- Objective: `{}`\n", report.selection.objective));

    if let Some(limit) = report.selection.max_time_to_green_ms {
        output.push_str(&format!(
            "- Max time-to-green: {:.2}s\n",
            limit as f64 / 1000.0
        ));
    }
    if let Some(limit) = report.selection.max_effective_cost_usd {
        output.push_str(&format!("- Max effective cost: ${limit:.6}\n"));
    }

    output.push_str(&format!(
        "- Eligible candidates: {}\n",
        report.selection.eligible_candidates
    ));

    match &report.selection.selected {
        Some(selected) => output.push_str(&format!(
            "- Selected: `{}/{}` at ${:.6}, {:.2}s time-to-green\n",
            selected.provider,
            selected.offer_id,
            selected.effective_cost_usd,
            selected.time_to_green_ms as f64 / 1000.0
        )),
        None => output.push_str("- Selected: none\n"),
    }

    if !report.selection.excluded.is_empty() {
        output.push_str("\n### Policy exclusions\n\n");
        for excluded in &report.selection.excluded {
            output.push_str(&format!(
                "- `{}/{}`: {}\n",
                excluded.provider,
                excluded.offer_id,
                excluded.reasons.join("; ")
            ));
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::ExecutionModel;
    use crate::economics::{CacheState, CostBasis};
    use crate::model::{GIB, RunnerShape};

    fn evaluation(provider: &str, cost: f64, time_ms: u64) -> EconomicsEvaluation {
        EconomicsEvaluation {
            provider: provider.into(),
            offer_id: "offer".into(),
            execution_model: ExecutionModel::ManagedEphemeral,
            offer_shape: RunnerShape::new(2_000, 4 * GIB),
            cache_state: CacheState::Warm,
            scope: None,
            base_duration_ms: 10_000,
            cache_penalty_ms: 0,
            effective_runtime_ms: 10_000,
            queue_depth: 0,
            running_jobs: 0,
            parallel_slots: 1,
            queue_waves_before_start: 0,
            estimated_queue_ms: 0,
            time_to_green_ms: time_ms,
            billed_seconds: Some(10),
            effective_cost_usd: cost,
            cost_basis: CostBasis::ManagedBilling,
            utilization: None,
            pareto_optimal: true,
        }
    }

    fn report(evaluations: Vec<EconomicsEvaluation>) -> EconomicsReport {
        EconomicsReport {
            schema_version: 1,
            snapshot_observed_at: "2026-09-29T00:00:00Z".into(),
            snapshot_source: "test".into(),
            snapshot_repository_visibility: None,
            target: RunnerShape::new(2_000, 4 * GIB),
            predicted_warm_duration_ms: 10_000,
            evaluations,
            skipped: Vec::new(),
        }
    }

    #[test]
    fn cost_objective_respects_time_guard() {
        let report = report(vec![
            evaluation("cheap-slow", 0.001, 40_000),
            evaluation("fast", 0.002, 10_000),
        ]);
        let selection = select(&report, &EconomicsPolicy::default_v1()).expect("selection");
        assert_eq!(selection.selected.expect("selected").provider, "fast");
        assert_eq!(selection.excluded.len(), 1);
    }

    #[test]
    fn time_objective_respects_cost_guard() {
        let report = report(vec![
            evaluation("fast-expensive", 0.01, 5_000),
            evaluation("slower-cheap", 0.001, 10_000),
        ]);
        let policy = EconomicsPolicy {
            schema_version: 1,
            policy_id: "time-budget".into(),
            objective: EconomicsObjective::MinimizeTimeToGreen,
            max_time_to_green_ms: None,
            max_effective_cost_usd: Some(0.002),
        };
        let selection = select(&report, &policy).expect("selection");
        assert_eq!(
            selection.selected.expect("selected").provider,
            "slower-cheap"
        );
    }
}
