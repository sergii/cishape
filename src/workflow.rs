use crate::catalog::ProviderCatalog;
use crate::economics::{CapacitySnapshot, CapacityState, evaluate_offer};
use crate::model::RunnerShape;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const WORKFLOW_DEMAND_SCHEMA_VERSION: u32 = 1;
pub const WORKFLOW_ECONOMICS_REPORT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDemand {
    pub schema_version: u32,
    pub workflow_id: String,
    pub jobs: Vec<WorkflowJobDemand>,
}

impl WorkflowDemand {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes =
            std::fs::read(path).with_context(|| format!("read workflow demand {}", path.display()))?;
        let demand: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse workflow demand {}", path.display()))?;
        demand.validate()?;
        Ok(demand)
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.schema_version == WORKFLOW_DEMAND_SCHEMA_VERSION,
            "unsupported workflow demand schema version {}",
            self.schema_version
        );
        anyhow::ensure!(!self.workflow_id.trim().is_empty(), "workflow_id is required");
        anyhow::ensure!(!self.jobs.is_empty(), "workflow must contain at least one job");

        let mut ids = BTreeSet::new();
        for job in &self.jobs {
            job.validate()?;
            anyhow::ensure!(
                ids.insert(job.id.clone()),
                "duplicate workflow job id {}",
                job.id
            );

            let mut dependencies = BTreeSet::new();
            for dependency in &job.dependencies {
                anyhow::ensure!(
                    dependencies.insert(dependency),
                    "job {} has duplicate dependency {}",
                    job.id,
                    dependency
                );
                anyhow::ensure!(
                    dependency != &job.id,
                    "job {} cannot depend on itself",
                    job.id
                );
            }
        }

        for job in &self.jobs {
            for dependency in &job.dependencies {
                anyhow::ensure!(
                    ids.contains(dependency),
                    "job {} depends on missing job {}",
                    job.id,
                    dependency
                );
            }
        }

        topological_order(self)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowJobDemand {
    pub id: String,
    pub target: RunnerShape,
    pub predicted_warm_duration_ms: u64,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

impl WorkflowJobDemand {
    fn validate(&self) -> Result<()> {
        anyhow::ensure!(!self.id.trim().is_empty(), "workflow job id is required");
        anyhow::ensure!(
            self.target.cpu_millis > 0,
            "job {} CPU target must be positive",
            self.id
        );
        anyhow::ensure!(
            self.target.memory_bytes > 0,
            "job {} memory target must be positive",
            self.id
        );
        anyhow::ensure!(
            self.predicted_warm_duration_ms > 0,
            "job {} predicted warm duration must be positive",
            self.id
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowJobSchedule {
    pub id: String,
    pub target: RunnerShape,
    pub dependencies: Vec<String>,
    pub predicted_warm_duration_ms: u64,
    pub effective_runtime_ms: u64,
    pub slot_index: usize,
    pub start_ms: u64,
    pub finish_ms: u64,
    pub effective_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEconomicsEvaluation {
    pub provider: String,
    pub offer_id: String,
    pub offer_shape: RunnerShape,
    pub queue_depth: u32,
    pub running_jobs: u32,
    pub parallel_slots: u32,
    pub critical_path_ms: u64,
    pub time_to_green_ms: u64,
    pub effective_cost_usd: f64,
    pub jobs: Vec<WorkflowJobSchedule>,
    pub pareto_optimal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEconomicsSkip {
    pub provider: String,
    pub offer_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEconomicsReport {
    pub schema_version: u32,
    pub workflow_id: String,
    pub snapshot_observed_at: String,
    pub snapshot_source: String,
    pub evaluations: Vec<WorkflowEconomicsEvaluation>,
    pub skipped: Vec<WorkflowEconomicsSkip>,
}

#[derive(Debug, Clone)]
struct PreparedJob {
    effective_runtime_ms: u64,
    effective_cost_usd: f64,
}

pub fn evaluate(
    catalog: &ProviderCatalog,
    snapshot: &CapacitySnapshot,
    workflow: &WorkflowDemand,
) -> Result<WorkflowEconomicsReport> {
    snapshot.validate()?;
    workflow.validate()?;

    let mut evaluations = Vec::new();
    let mut skipped = Vec::new();

    for state in &snapshot.states {
        let Some(offer) = catalog
            .offers
            .iter()
            .find(|offer| offer.provider == state.provider && offer.offer_id == state.offer_id)
        else {
            skipped.push(WorkflowEconomicsSkip {
                provider: state.provider.clone(),
                offer_id: state.offer_id.clone(),
                reason: "offer not found in provider catalog".into(),
            });
            continue;
        };

        let mut prepared = Vec::with_capacity(workflow.jobs.len());
        let mut rejection = None;

        for job in &workflow.jobs {
            match evaluate_offer(offer, state, &job.target, job.predicted_warm_duration_ms) {
                Ok(base) => prepared.push(PreparedJob {
                    effective_runtime_ms: base.effective_runtime_ms,
                    effective_cost_usd: base.effective_cost_usd,
                }),
                Err(reason) => {
                    rejection = Some(format!("job {}: {reason}", job.id));
                    break;
                }
            }
        }

        if let Some(reason) = rejection {
            skipped.push(WorkflowEconomicsSkip {
                provider: state.provider.clone(),
                offer_id: state.offer_id.clone(),
                reason,
            });
            continue;
        }

        match schedule(workflow, state, &prepared) {
            Ok(mut evaluation) => {
                evaluation.provider = offer.provider.clone();
                evaluation.offer_id = offer.offer_id.clone();
                evaluation.offer_shape = offer
                    .capacity
                    .complete_shape()
                    .expect("all jobs were evaluated against complete capacity");
                evaluations.push(evaluation);
            }
            Err(error) => skipped.push(WorkflowEconomicsSkip {
                provider: state.provider.clone(),
                offer_id: state.offer_id.clone(),
                reason: error.to_string(),
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

    Ok(WorkflowEconomicsReport {
        schema_version: WORKFLOW_ECONOMICS_REPORT_SCHEMA_VERSION,
        workflow_id: workflow.workflow_id.clone(),
        snapshot_observed_at: snapshot.observed_at.clone(),
        snapshot_source: snapshot.source.clone(),
        evaluations,
        skipped,
    })
}

fn schedule(
    workflow: &WorkflowDemand,
    state: &CapacityState,
    prepared: &[PreparedJob],
) -> Result<WorkflowEconomicsEvaluation> {
    let order = topological_order(workflow)?;
    let by_id = workflow
        .jobs
        .iter()
        .enumerate()
        .map(|(index, job)| (job.id.as_str(), index))
        .collect::<BTreeMap<_, _>>();

    let mut critical_finish = vec![0_u64; workflow.jobs.len()];
    for &index in &order {
        let dependency_finish = workflow.jobs[index]
            .dependencies
            .iter()
            .map(|dependency| critical_finish[by_id[dependency.as_str()]])
            .max()
            .unwrap_or(0);
        critical_finish[index] = dependency_finish
            .checked_add(prepared[index].effective_runtime_ms)
            .ok_or_else(|| anyhow::anyhow!("critical path overflow"))?;
    }
    let critical_path_ms = critical_finish.into_iter().max().unwrap_or(0);

    let mut availability = initial_slot_availability(state)?;
    let mut scheduled: Vec<Option<WorkflowJobSchedule>> = vec![None; workflow.jobs.len()];

    for _ in 0..workflow.jobs.len() {
        let mut best: Option<(u64, String, usize, usize)> = None;

        for (job_index, job) in workflow.jobs.iter().enumerate() {
            if scheduled[job_index].is_some() {
                continue;
            }

            let mut dependency_ready_ms = 0_u64;
            let mut dependencies_scheduled = true;
            for dependency in &job.dependencies {
                let dependency_index = by_id[dependency.as_str()];
                match &scheduled[dependency_index] {
                    Some(item) => dependency_ready_ms = dependency_ready_ms.max(item.finish_ms),
                    None => {
                        dependencies_scheduled = false;
                        break;
                    }
                }
            }
            if !dependencies_scheduled {
                continue;
            }

            let (slot_index, start_ms) = earliest_feasible_slot(&availability, dependency_ready_ms);
            let candidate = (start_ms, job.id.clone(), job_index, slot_index);
            if best.as_ref().is_none_or(|current| candidate < *current) {
                best = Some(candidate);
            }
        }

        let (start_ms, _, job_index, slot_index) =
            best.ok_or_else(|| anyhow::anyhow!("workflow scheduler made no progress"))?;
        let job = &workflow.jobs[job_index];
        let finish_ms = start_ms
            .checked_add(prepared[job_index].effective_runtime_ms)
            .ok_or_else(|| anyhow::anyhow!("workflow completion overflow"))?;
        availability[slot_index] = finish_ms;

        scheduled[job_index] = Some(WorkflowJobSchedule {
            id: job.id.clone(),
            target: job.target.clone(),
            dependencies: job.dependencies.clone(),
            predicted_warm_duration_ms: job.predicted_warm_duration_ms,
            effective_runtime_ms: prepared[job_index].effective_runtime_ms,
            slot_index,
            start_ms,
            finish_ms,
            effective_cost_usd: prepared[job_index].effective_cost_usd,
        });
    }

    let mut jobs = scheduled
        .into_iter()
        .map(|item| item.expect("all jobs scheduled"))
        .collect::<Vec<_>>();
    jobs.sort_by(|left, right| {
        left.start_ms
            .cmp(&right.start_ms)
            .then_with(|| left.id.cmp(&right.id))
    });

    let time_to_green_ms = jobs.iter().map(|job| job.finish_ms).max().unwrap_or(0);
    let effective_cost_usd = jobs.iter().map(|job| job.effective_cost_usd).sum();

    Ok(WorkflowEconomicsEvaluation {
        provider: String::new(),
        offer_id: String::new(),
        offer_shape: RunnerShape::new(1, 1),
        queue_depth: state.queue_depth,
        running_jobs: state.running_jobs,
        parallel_slots: state.parallel_slots,
        critical_path_ms,
        time_to_green_ms,
        effective_cost_usd,
        jobs,
        pareto_optimal: false,
    })
}

fn initial_slot_availability(state: &CapacityState) -> Result<Vec<u64>> {
    let mut availability = vec![0_u64; state.parallel_slots as usize];
    for slot in availability.iter_mut().take(state.running_jobs as usize) {
        *slot = state.slot_turnover_ms;
    }

    for _ in 0..state.queue_depth {
        let slot = earliest_available_slot(&availability);
        availability[slot] = availability[slot]
            .checked_add(state.slot_turnover_ms)
            .ok_or_else(|| anyhow::anyhow!("queued slot availability overflow"))?;
    }

    Ok(availability)
}

fn earliest_available_slot(availability: &[u64]) -> usize {
    availability
        .iter()
        .enumerate()
        .min_by_key(|(index, available_at)| (**available_at, *index))
        .map(|(index, _)| index)
        .expect("capacity snapshot guarantees at least one slot")
}

fn earliest_feasible_slot(availability: &[u64], ready_ms: u64) -> (usize, u64) {
    availability
        .iter()
        .enumerate()
        .map(|(index, available_at)| (index, (*available_at).max(ready_ms)))
        .min_by_key(|(index, start_ms)| (*start_ms, *index))
        .expect("capacity snapshot guarantees at least one slot")
}

fn topological_order(workflow: &WorkflowDemand) -> Result<Vec<usize>> {
    let by_id = workflow
        .jobs
        .iter()
        .enumerate()
        .map(|(index, job)| (job.id.as_str(), index))
        .collect::<BTreeMap<_, _>>();

    let mut indegree = workflow
        .jobs
        .iter()
        .map(|job| job.dependencies.len())
        .collect::<Vec<_>>();
    let mut dependents = vec![Vec::new(); workflow.jobs.len()];

    for (index, job) in workflow.jobs.iter().enumerate() {
        for dependency in &job.dependencies {
            let dependency_index = *by_id
                .get(dependency.as_str())
                .ok_or_else(|| anyhow::anyhow!("job {} depends on missing job {}", job.id, dependency))?;
            dependents[dependency_index].push(index);
        }
    }

    let mut ready = BTreeSet::new();
    for (index, job) in workflow.jobs.iter().enumerate() {
        if indegree[index] == 0 {
            ready.insert((job.id.clone(), index));
        }
    }

    let mut order = Vec::with_capacity(workflow.jobs.len());
    while let Some((id, index)) = ready.iter().next().cloned() {
        ready.remove(&(id, index));
        order.push(index);

        for &dependent in &dependents[index] {
            indegree[dependent] -= 1;
            if indegree[dependent] == 0 {
                ready.insert((workflow.jobs[dependent].id.clone(), dependent));
            }
        }
    }

    anyhow::ensure!(
        order.len() == workflow.jobs.len(),
        "workflow dependency graph contains a cycle"
    );
    Ok(order)
}

fn mark_pareto_frontier(evaluations: &mut [WorkflowEconomicsEvaluation]) {
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

pub fn to_markdown(report: &WorkflowEconomicsReport) -> String {
    let mut output = String::new();
    output.push_str("# CIShape workflow economics\n\n");
    output.push_str(&format!("- Workflow: `{}`\n", report.workflow_id));
    output.push_str(&format!(
        "- Snapshot: `{}` from `{}`\n\n",
        report.snapshot_observed_at, report.snapshot_source
    ));
    output.push_str("| Provider | Offer | Slots | Critical path | Time-to-green | Total cost | Pareto |\n");
    output.push_str("| --- | --- | ---: | ---: | ---: | ---: | --- |\n");

    for evaluation in &report.evaluations {
        output.push_str(&format!(
            "| {} | {} | {} | {:.2}s | {:.2}s | ${:.6} | {} |\n",
            evaluation.provider,
            evaluation.offer_id,
            evaluation.parallel_slots,
            evaluation.critical_path_ms as f64 / 1000.0,
            evaluation.time_to_green_ms as f64 / 1000.0,
            evaluation.effective_cost_usd,
            if evaluation.pareto_optimal { "yes" } else { "no" }
        ));
    }

    for evaluation in &report.evaluations {
        output.push_str(&format!(
            "\n## Schedule: {}/{}\n\n",
            evaluation.provider, evaluation.offer_id
        ));
        output.push_str("| Job | Shape | Dependencies | Slot | Start | Finish | Runtime | Cost |\n");
        output.push_str("| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |\n");

        for job in &evaluation.jobs {
            let dependencies = if job.dependencies.is_empty() {
                "-".into()
            } else {
                job.dependencies.join(", ")
            };
            output.push_str(&format!(
                "| {} | {} | {} | {} | {:.2}s | {:.2}s | {:.2}s | ${:.6} |\n",
                job.id,
                job.target.display_id(),
                dependencies,
                job.slot_index,
                job.start_ms as f64 / 1000.0,
                job.finish_ms as f64 / 1000.0,
                job.effective_runtime_ms as f64 / 1000.0,
                job.effective_cost_usd
            ));
        }
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
    use crate::model::GIB;

    fn job(id: &str, dependencies: &[&str]) -> WorkflowJobDemand {
        WorkflowJobDemand {
            id: id.into(),
            target: RunnerShape::new(1_000, 2 * GIB),
            predicted_warm_duration_ms: 1_000,
            dependencies: dependencies.iter().map(|item| (*item).into()).collect(),
        }
    }

    #[test]
    fn rejects_missing_dependencies() {
        let workflow = WorkflowDemand {
            schema_version: 1,
            workflow_id: "missing".into(),
            jobs: vec![job("build", &["prepare"])],
        };

        assert!(workflow.validate().is_err());
    }

    #[test]
    fn rejects_cycles() {
        let workflow = WorkflowDemand {
            schema_version: 1,
            workflow_id: "cycle".into(),
            jobs: vec![job("a", &["b"]), job("b", &["a"])],
        };

        assert!(workflow.validate().is_err());
    }
}
