use crate::capacity_scope::CapacityScope;
use crate::catalog::{
    ExecutionModel, OfferPricing, ProviderCatalog, RepositoryVisibility, RunnerOffer,
};
use crate::model::RunnerShape;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const CAPACITY_SNAPSHOT_SCHEMA_VERSION: u32 = 1;
pub const ECONOMICS_REPORT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapacitySnapshot {
    pub schema_version: u32,
    pub observed_at: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_visibility: Option<RepositoryVisibility>,
    pub states: Vec<CapacityState>,
}

impl CapacitySnapshot {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("read capacity snapshot {}", path.display()))?;
        let snapshot: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse capacity snapshot {}", path.display()))?;
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.schema_version == CAPACITY_SNAPSHOT_SCHEMA_VERSION,
            "unsupported capacity snapshot schema version {}",
            self.schema_version
        );
        anyhow::ensure!(
            !self.observed_at.trim().is_empty(),
            "capacity snapshot observed_at is required"
        );
        anyhow::ensure!(
            !self.source.trim().is_empty(),
            "capacity snapshot source is required"
        );

        let mut identities = std::collections::BTreeSet::new();
        for state in &self.states {
            state.validate()?;
            let identity = format!("{}:{}", state.provider, state.offer_id);
            anyhow::ensure!(
                identities.insert(identity.clone()),
                "duplicate capacity state {identity}"
            );
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CacheState {
    Warm,
    Cold,
}

impl std::fmt::Display for CacheState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Warm => write!(formatter, "warm"),
            Self::Cold => write!(formatter, "cold"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapacityState {
    pub provider: String,
    pub offer_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity_scope: Option<CapacityScope>,
    pub queue_depth: u32,
    pub running_jobs: u32,
    pub parallel_slots: u32,
    pub slot_turnover_ms: u64,
    pub cache_state: CacheState,
    pub cache_penalty_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub utilization: Option<f64>,
}

impl CapacityState {
    fn validate(&self) -> Result<()> {
        anyhow::ensure!(!self.provider.trim().is_empty(), "provider is required");
        anyhow::ensure!(!self.offer_id.trim().is_empty(), "offer_id is required");
        if let Some(scope) = &self.capacity_scope {
            scope.validate()?;
        }
        anyhow::ensure!(self.parallel_slots > 0, "parallel_slots must be positive");
        anyhow::ensure!(
            self.running_jobs <= self.parallel_slots,
            "running_jobs cannot exceed parallel_slots"
        );
        anyhow::ensure!(
            self.slot_turnover_ms > 0,
            "slot_turnover_ms must be positive"
        );
        if self.cache_state == CacheState::Warm {
            anyhow::ensure!(
                self.cache_penalty_ms == 0,
                "warm cache cannot have a cache penalty"
            );
        }
        if let Some(utilization) = self.utilization {
            anyhow::ensure!(
                utilization > 0.0 && utilization <= 1.0,
                "utilization must be in (0, 1]"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CostBasis {
    ManagedBilling,
    AllocatedFixedCapacity,
}

impl std::fmt::Display for CostBasis {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ManagedBilling => write!(formatter, "managed_billing"),
            Self::AllocatedFixedCapacity => write!(formatter, "allocated_fixed_capacity"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicsEvaluation {
    pub provider: String,
    pub offer_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity_scope: Option<CapacityScope>,
    pub execution_model: ExecutionModel,
    pub offer_shape: RunnerShape,
    pub cache_state: CacheState,
    pub base_duration_ms: u64,
    pub cache_penalty_ms: u64,
    pub effective_runtime_ms: u64,
    pub queue_depth: u32,
    pub running_jobs: u32,
    pub parallel_slots: u32,
    pub queue_waves_before_start: u64,
    pub estimated_queue_ms: u64,
    pub time_to_green_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billed_seconds: Option<u64>,
    pub effective_cost_usd: f64,
    pub cost_basis: CostBasis,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub utilization: Option<f64>,
    pub pareto_optimal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicsSkip {
    pub provider: String,
    pub offer_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicsReport {
    pub schema_version: u32,
    pub snapshot_observed_at: String,
    pub snapshot_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_repository_visibility: Option<RepositoryVisibility>,
    pub target: RunnerShape,
    pub predicted_warm_duration_ms: u64,
    pub evaluations: Vec<EconomicsEvaluation>,
    pub skipped: Vec<EconomicsSkip>,
}

pub fn evaluate(
    catalog: &ProviderCatalog,
    snapshot: &CapacitySnapshot,
    target: &RunnerShape,
    predicted_warm_duration_ms: u64,
) -> Result<EconomicsReport> {
    snapshot.validate()?;
    anyhow::ensure!(
        predicted_warm_duration_ms > 0,
        "predicted warm duration must be positive"
    );

    let mut evaluations = Vec::new();
    let mut skipped = Vec::new();

    for state in &snapshot.states {
        let Some(offer) = catalog
            .offers
            .iter()
            .find(|offer| offer.provider == state.provider && offer.offer_id == state.offer_id)
        else {
            skipped.push(EconomicsSkip {
                provider: state.provider.clone(),
                offer_id: state.offer_id.clone(),
                reason: "offer not found in provider catalog".into(),
            });
            continue;
        };

        match evaluate_offer(
            offer,
            state,
            snapshot.repository_visibility.as_ref(),
            target,
            predicted_warm_duration_ms,
        ) {
            Ok(evaluation) => evaluations.push(evaluation),
            Err(reason) => skipped.push(EconomicsSkip {
                provider: state.provider.clone(),
                offer_id: state.offer_id.clone(),
                reason,
            }),
        }
    }

    mark_pareto_frontier(&mut evaluations);
    evaluations.sort_by(|left, right| {
        left.provider
            .cmp(&right.provider)
            .then_with(|| left.offer_id.cmp(&right.offer_id))
    });
    skipped.sort_by(|left, right| {
        left.provider
            .cmp(&right.provider)
            .then_with(|| left.offer_id.cmp(&right.offer_id))
    });

    Ok(EconomicsReport {
        schema_version: ECONOMICS_REPORT_SCHEMA_VERSION,
        snapshot_observed_at: snapshot.observed_at.clone(),
        snapshot_source: snapshot.source.clone(),
        snapshot_repository_visibility: snapshot.repository_visibility.clone(),
        target: target.clone(),
        predicted_warm_duration_ms,
        evaluations,
        skipped,
    })
}

pub(crate) fn evaluate_offer(
    offer: &RunnerOffer,
    state: &CapacityState,
    repository_visibility: Option<&RepositoryVisibility>,
    target: &RunnerShape,
    predicted_warm_duration_ms: u64,
) -> std::result::Result<EconomicsEvaluation, String> {
    if let Some(required_visibility) = &offer.repository_visibility {
        match repository_visibility {
            Some(actual_visibility) if actual_visibility == required_visibility => {}
            Some(actual_visibility) => {
                return Err(format!(
                    "offer requires {required_visibility} repository visibility but snapshot is {actual_visibility}"
                ));
            }
            None => {
                return Err(format!(
                    "offer requires {required_visibility} repository visibility but snapshot repository visibility is unknown"
                ));
            }
        }
    }

    if let Some(required_scope) = &offer.required_capacity_scope {
        match &state.capacity_scope {
            Some(actual_scope) if &actual_scope.kind == required_scope => {}
            Some(actual_scope) => {
                return Err(format!(
                    "offer requires {required_scope} capacity scope but snapshot state is {} ({})",
                    actual_scope.kind, actual_scope.key
                ));
            }
            None => {
                return Err(format!(
                    "offer requires {required_scope} capacity scope but snapshot state scope is unknown"
                ));
            }
        }
    }

    let shape = offer
        .capacity
        .complete_shape()
        .ok_or_else(|| "offer capacity is incomplete".to_string())?;
    if shape.cpu_millis < target.cpu_millis || shape.memory_bytes < target.memory_bytes {
        return Err("offer does not satisfy target capacity".into());
    }

    let effective_runtime_ms = predicted_warm_duration_ms
        .checked_add(state.cache_penalty_ms)
        .ok_or_else(|| "effective runtime overflow".to_string())?;
    let queue_waves_before_start = queue_waves_before_start(state);
    let estimated_queue_ms = queue_waves_before_start
        .checked_mul(state.slot_turnover_ms)
        .ok_or_else(|| "queue estimate overflow".to_string())?;
    let time_to_green_ms = estimated_queue_ms
        .checked_add(effective_runtime_ms)
        .ok_or_else(|| "time-to-green overflow".to_string())?;

    let (billed_seconds, effective_cost_usd, cost_basis, utilization) = match &offer.pricing {
        OfferPricing::PerMinute {
            usd_per_minute,
            billing_increment_seconds: Some(increment_seconds),
            ..
        } => {
            let runtime_seconds = effective_runtime_ms.max(1).div_ceil(1000);
            let billed_seconds = runtime_seconds.div_ceil(*increment_seconds) * *increment_seconds;
            (
                Some(billed_seconds),
                *usd_per_minute * billed_seconds as f64 / 60.0,
                CostBasis::ManagedBilling,
                state.utilization,
            )
        }
        OfferPricing::PerMinute {
            billing_increment_seconds: None,
            ..
        } => return Err("billing increment is unknown".into()),
        OfferPricing::FixedServer { usd_per_hour, .. } => {
            let utilization = state
                .utilization
                .ok_or_else(|| "fixed-server utilization is required".to_string())?;
            let productive_slot_hour_rate =
                *usd_per_hour / (state.parallel_slots as f64 * utilization);
            let effective_cost_usd =
                productive_slot_hour_rate * effective_runtime_ms as f64 / 3_600_000.0;
            (
                None,
                effective_cost_usd,
                CostBasis::AllocatedFixedCapacity,
                Some(utilization),
            )
        }
        OfferPricing::Unknown { .. } => return Err("pricing is unknown".into()),
    };

    if offer.execution_model == ExecutionModel::SelfHostedVm && utilization.is_none() {
        return Err("self-hosted utilization is required".into());
    }

    Ok(EconomicsEvaluation {
        provider: offer.provider.clone(),
        offer_id: offer.offer_id.clone(),
        capacity_scope: state.capacity_scope.clone(),
        execution_model: offer.execution_model.clone(),
        offer_shape: shape,
        cache_state: state.cache_state.clone(),
        base_duration_ms: predicted_warm_duration_ms,
        cache_penalty_ms: state.cache_penalty_ms,
        effective_runtime_ms,
        queue_depth: state.queue_depth,
        running_jobs: state.running_jobs,
        parallel_slots: state.parallel_slots,
        queue_waves_before_start,
        estimated_queue_ms,
        time_to_green_ms,
        billed_seconds,
        effective_cost_usd,
        cost_basis,
        utilization,
        pareto_optimal: false,
    })
}

fn queue_waves_before_start(state: &CapacityState) -> u64 {
    let free_slots = state.parallel_slots - state.running_jobs;
    if state.queue_depth < free_slots {
        return 0;
    }

    let queued_after_free_slots = state.queue_depth - free_slots;
    u64::from(queued_after_free_slots) / u64::from(state.parallel_slots) + 1
}

fn mark_pareto_frontier(evaluations: &mut [EconomicsEvaluation]) {
    for index in 0..evaluations.len() {
        let candidate_cost = evaluations[index].effective_cost_usd;
        let candidate_time = evaluations[index].time_to_green_ms;

        let dominated = evaluations.iter().enumerate().any(|(other_index, other)| {
            if other_index == index {
                return false;
            }

            let no_worse = other.effective_cost_usd <= candidate_cost
                && other.time_to_green_ms <= candidate_time;
            let strictly_better = other.effective_cost_usd < candidate_cost
                || other.time_to_green_ms < candidate_time;
            no_worse && strictly_better
        });

        evaluations[index].pareto_optimal = !dominated;
    }
}

pub fn to_markdown(report: &EconomicsReport) -> String {
    let mut output = String::new();
    output.push_str("# CIShape capacity economics\n\n");
    output.push_str(&format!(
        "- Snapshot: `{}` from `{}`\n",
        report.snapshot_observed_at, report.snapshot_source
    ));
    if let Some(visibility) = &report.snapshot_repository_visibility {
        output.push_str(&format!("- Repository visibility: `{visibility}`\n"));
    }
    output.push_str(&format!("- Target: `{}`\n", report.target.display_id()));
    output.push_str(&format!(
        "- Predicted warm runtime: {:.2}s\n\n",
        report.predicted_warm_duration_ms as f64 / 1000.0
    ));

    output.push_str("| Provider | Offer | Cache | Queue | Running/slots | Runtime | Queue wait | Time-to-green | Effective cost | Cost basis | Pareto |\n");
    output.push_str("| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |\n");

    for evaluation in &report.evaluations {
        output.push_str(&format!(
            "| {} | {} | {} | {} | {}/{} | {:.2}s | {:.2}s | {:.2}s | ${:.6} | {} | {} |\n",
            evaluation.provider,
            evaluation.offer_id,
            evaluation.cache_state,
            evaluation.queue_depth,
            evaluation.running_jobs,
            evaluation.parallel_slots,
            evaluation.effective_runtime_ms as f64 / 1000.0,
            evaluation.estimated_queue_ms as f64 / 1000.0,
            evaluation.time_to_green_ms as f64 / 1000.0,
            evaluation.effective_cost_usd,
            evaluation.cost_basis,
            if evaluation.pareto_optimal {
                "yes"
            } else {
                "no"
            }
        ));
    }

    if !report.skipped.is_empty() {
        output.push_str("\n## Skipped\n\n");
        for skipped in &report.skipped {
            output.push_str(&format!(
                "- `{}/{}`: {}\n",
                skipped.provider, skipped.offer_id, skipped.reason
            ));
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{RunnerCapacity, RunnerOffer};
    use crate::model::GIB;

    fn state(queue_depth: u32, running_jobs: u32, parallel_slots: u32) -> CapacityState {
        CapacityState {
            provider: "test".into(),
            offer_id: "test".into(),
            capacity_scope: None,
            queue_depth,
            running_jobs,
            parallel_slots,
            slot_turnover_ms: 10_000,
            cache_state: CacheState::Warm,
            cache_penalty_ms: 0,
            utilization: None,
        }
    }

    fn fixed_offer() -> RunnerOffer {
        RunnerOffer {
            provider: "host".into(),
            offer_id: "vm".into(),
            runner_label: None,
            capacity: RunnerCapacity {
                cpu_millis: Some(4_000),
                memory_bytes: Some(8 * GIB),
            },
            os: "linux".into(),
            architecture: "x86_64".into(),
            execution_model: ExecutionModel::SelfHostedVm,
            repository_visibility: None,
            required_capacity_scope: None,
            pricing: OfferPricing::FixedServer {
                usd_per_hour: 0.016,
                monthly_cap_usd: None,
                context: None,
            },
            source_url: "https://example.com".into(),
            observed_at: "2026-09-29".into(),
            region: None,
            notes: None,
        }
    }

    #[test]
    fn queue_uses_free_slots_before_waiting_for_turnover() {
        assert_eq!(queue_waves_before_start(&state(0, 1, 2)), 0);
        assert_eq!(queue_waves_before_start(&state(1, 1, 2)), 1);
        assert_eq!(queue_waves_before_start(&state(3, 2, 2)), 2);
    }

    #[test]
    fn warm_cache_rejects_nonzero_penalty() {
        let mut input = state(0, 0, 1);
        input.cache_penalty_ms = 1;
        assert!(input.validate().is_err());
    }

    #[test]
    fn fixed_capacity_cost_uses_utilization_and_parallel_slots() {
        let offer = fixed_offer();
        let mut input = state(0, 0, 2);
        input.provider = "host".into();
        input.offer_id = "vm".into();
        input.utilization = Some(0.5);

        let evaluation = evaluate_offer(
            &offer,
            &input,
            None,
            &RunnerShape::new(2_000, 4 * GIB),
            11_000,
        )
        .expect("evaluation");

        let expected = 0.016 / (2.0 * 0.5) * 11_000.0 / 3_600_000.0;
        assert!((evaluation.effective_cost_usd - expected).abs() < 1e-12);
        assert_eq!(evaluation.cost_basis, CostBasis::AllocatedFixedCapacity);
    }
}
