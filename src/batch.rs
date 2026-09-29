use crate::catalog::RepositoryVisibility;
use crate::economics::{CapacityScope, CapacitySnapshot, EconomicsReport};
use crate::model::RunnerShape;
use anyhow::Result;
use serde::{Deserialize, Serialize};

pub const BATCH_ECONOMICS_REPORT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchEconomicsEvaluation {
    pub provider: String,
    pub offer_id: String,
    pub offer_shape: RunnerShape,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<CapacityScope>,
    pub parallel_jobs: u32,
    pub effective_runtime_ms: u64,
    pub first_job_start_ms: u64,
    pub last_job_start_ms: u64,
    pub time_to_green_ms: u64,
    pub per_job_effective_cost_usd: f64,
    pub effective_cost_usd: f64,
    pub pareto_optimal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchEconomicsReport {
    pub schema_version: u32,
    pub snapshot_observed_at: String,
    pub snapshot_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_repository_visibility: Option<RepositoryVisibility>,
    pub target: RunnerShape,
    pub predicted_warm_duration_ms: u64,
    pub parallel_jobs: u32,
    pub evaluations: Vec<BatchEconomicsEvaluation>,
}

pub fn evaluate(
    economics: &EconomicsReport,
    snapshot: &CapacitySnapshot,
    parallel_jobs: u32,
) -> Result<BatchEconomicsReport> {
    anyhow::ensure!(parallel_jobs > 0, "parallel_jobs must be positive");
    snapshot.validate()?;

    let mut evaluations = Vec::with_capacity(economics.evaluations.len());

    for base in &economics.evaluations {
        let state = snapshot
            .states
            .iter()
            .find(|state| state.provider == base.provider && state.offer_id == base.offer_id)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "missing capacity state for {}/{}",
                    base.provider,
                    base.offer_id
                )
            })?;

        let schedule = simulate(
            state.running_jobs,
            state.parallel_slots,
            state.queue_depth,
            state.slot_turnover_ms,
            base.effective_runtime_ms,
            parallel_jobs,
        )?;

        evaluations.push(BatchEconomicsEvaluation {
            provider: base.provider.clone(),
            offer_id: base.offer_id.clone(),
            offer_shape: base.offer_shape.clone(),
            scope: base.scope.clone(),
            parallel_jobs,
            effective_runtime_ms: base.effective_runtime_ms,
            first_job_start_ms: schedule.first_job_start_ms,
            last_job_start_ms: schedule.last_job_start_ms,
            time_to_green_ms: schedule.time_to_green_ms,
            per_job_effective_cost_usd: base.effective_cost_usd,
            effective_cost_usd: base.effective_cost_usd * f64::from(parallel_jobs),
            pareto_optimal: false,
        });
    }

    mark_pareto_frontier(&mut evaluations);
    evaluations.sort_by(|left, right| {
        left.provider
            .cmp(&right.provider)
            .then_with(|| left.offer_id.cmp(&right.offer_id))
    });

    Ok(BatchEconomicsReport {
        schema_version: BATCH_ECONOMICS_REPORT_SCHEMA_VERSION,
        snapshot_observed_at: economics.snapshot_observed_at.clone(),
        snapshot_source: economics.snapshot_source.clone(),
        snapshot_repository_visibility: economics.snapshot_repository_visibility.clone(),
        target: economics.target.clone(),
        predicted_warm_duration_ms: economics.predicted_warm_duration_ms,
        parallel_jobs,
        evaluations,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BatchSchedule {
    first_job_start_ms: u64,
    last_job_start_ms: u64,
    time_to_green_ms: u64,
}

fn simulate(
    running_jobs: u32,
    parallel_slots: u32,
    queue_depth: u32,
    slot_turnover_ms: u64,
    effective_runtime_ms: u64,
    parallel_jobs: u32,
) -> Result<BatchSchedule> {
    anyhow::ensure!(parallel_slots > 0, "parallel_slots must be positive");
    anyhow::ensure!(
        running_jobs <= parallel_slots,
        "running_jobs cannot exceed parallel_slots"
    );
    anyhow::ensure!(slot_turnover_ms > 0, "slot_turnover_ms must be positive");
    anyhow::ensure!(
        effective_runtime_ms > 0,
        "effective_runtime_ms must be positive"
    );
    anyhow::ensure!(parallel_jobs > 0, "parallel_jobs must be positive");

    let mut availability = vec![0_u64; parallel_slots as usize];
    for slot in availability.iter_mut().take(running_jobs as usize) {
        *slot = slot_turnover_ms;
    }

    for _ in 0..queue_depth {
        let slot = earliest_slot(&availability);
        availability[slot] = availability[slot]
            .checked_add(slot_turnover_ms)
            .ok_or_else(|| anyhow::anyhow!("queued slot availability overflow"))?;
    }

    let mut first_job_start_ms = u64::MAX;
    let mut last_job_start_ms = 0_u64;
    let mut time_to_green_ms = 0_u64;

    for _ in 0..parallel_jobs {
        let slot = earliest_slot(&availability);
        let start_ms = availability[slot];
        let finish_ms = start_ms
            .checked_add(effective_runtime_ms)
            .ok_or_else(|| anyhow::anyhow!("batch completion overflow"))?;
        availability[slot] = finish_ms;

        first_job_start_ms = first_job_start_ms.min(start_ms);
        last_job_start_ms = last_job_start_ms.max(start_ms);
        time_to_green_ms = time_to_green_ms.max(finish_ms);
    }

    Ok(BatchSchedule {
        first_job_start_ms,
        last_job_start_ms,
        time_to_green_ms,
    })
}

fn earliest_slot(availability: &[u64]) -> usize {
    availability
        .iter()
        .enumerate()
        .min_by_key(|(index, available_at)| (**available_at, *index))
        .map(|(index, _)| index)
        .expect("parallel_slots validated as positive")
}

fn mark_pareto_frontier(evaluations: &mut [BatchEconomicsEvaluation]) {
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

pub fn to_markdown(report: &BatchEconomicsReport) -> String {
    let mut output = String::new();
    output.push_str("## Parallel-job batch\n\n");
    if let Some(visibility) = &report.snapshot_repository_visibility {
        output.push_str(&format!("- Repository visibility: `{visibility}`\n"));
    }
    output.push_str(&format!("- Jobs: {}\n\n", report.parallel_jobs));
    output.push_str("| Provider | Offer | Scope | Jobs | First start | Last start | Time-to-green | Per-job cost | Total cost | Pareto |\n");
    output.push_str("| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |\n");

    for evaluation in &report.evaluations {
        output.push_str(&format!(
            "| {} | {} | {} | {} | {:.2}s | {:.2}s | {:.2}s | ${:.6} | ${:.6} | {} |\n",
            evaluation.provider,
            evaluation.offer_id,
            evaluation
                .scope
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "-".into()),
            evaluation.parallel_jobs,
            evaluation.first_job_start_ms as f64 / 1000.0,
            evaluation.last_job_start_ms as f64 / 1000.0,
            evaluation.time_to_green_ms as f64 / 1000.0,
            evaluation.per_job_effective_cost_usd,
            evaluation.effective_cost_usd,
            if evaluation.pareto_optimal {
                "yes"
            } else {
                "no"
            }
        ));
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_job_matches_existing_queue_wave_model() {
        let schedule = simulate(2, 2, 3, 11_000, 11_000, 1).expect("schedule");

        assert_eq!(
            schedule,
            BatchSchedule {
                first_job_start_ms: 22_000,
                last_job_start_ms: 22_000,
                time_to_green_ms: 33_000,
            }
        );
    }

    #[test]
    fn free_slots_start_parallel_jobs_immediately() {
        let schedule = simulate(1, 20, 0, 12_000, 11_000, 4).expect("schedule");

        assert_eq!(schedule.first_job_start_ms, 0);
        assert_eq!(schedule.last_job_start_ms, 0);
        assert_eq!(schedule.time_to_green_ms, 11_000);
    }

    #[test]
    fn batch_spills_into_later_waves() {
        let schedule = simulate(1, 20, 0, 12_000, 11_000, 30).expect("schedule");

        assert_eq!(schedule.first_job_start_ms, 0);
        assert_eq!(schedule.last_job_start_ms, 11_000);
        assert_eq!(schedule.time_to_green_ms, 22_000);
    }
}
