use crate::binding::BindingReport;
use crate::execution::{CpuTenancy, ExecutionPlan, HostTenancy};
use crate::model::RunnerShape;
use crate::worker_state::{WorkerLifecycle, WorkerPressure, WorkerState, WorkerStateSnapshot};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const ADMISSION_POLICY_SCHEMA_VERSION: u32 = 1;
pub const ADMISSION_REPORT_SCHEMA_VERSION: u32 = 2;
pub const ADMISSION_ALGORITHM_VERSION: &str = "worker-admission-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapacityBasis {
    Physical,
    AllocationLimit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdmissionPolicy {
    pub schema_version: u32,
    pub policy_id: String,
    pub cpu_capacity_basis: CapacityBasis,
    pub memory_capacity_basis: CapacityBasis,
    #[serde(default)]
    pub reserve_cpu_millis: u32,
    #[serde(default)]
    pub reserve_memory_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cpu_utilization_ratio: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_memory_utilization_ratio: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_io_pressure_ratio: Option<f64>,
}

impl AdmissionPolicy {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("read admission policy {}", path.display()))?;
        let policy: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse admission policy {}", path.display()))?;
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.schema_version == ADMISSION_POLICY_SCHEMA_VERSION,
            "unsupported admission policy schema version {}",
            self.schema_version
        );
        anyhow::ensure!(!self.policy_id.trim().is_empty(), "policy_id is required");

        validate_threshold("max_cpu_utilization_ratio", self.max_cpu_utilization_ratio)?;
        validate_threshold(
            "max_memory_utilization_ratio",
            self.max_memory_utilization_ratio,
        )?;
        validate_threshold("max_io_pressure_ratio", self.max_io_pressure_ratio)?;

        if self.cpu_capacity_basis == CapacityBasis::AllocationLimit {
            anyhow::ensure!(
                self.max_cpu_utilization_ratio.is_some(),
                "allocation_limit CPU basis requires max_cpu_utilization_ratio"
            );
        }
        if self.memory_capacity_basis == CapacityBasis::AllocationLimit {
            anyhow::ensure!(
                self.max_memory_utilization_ratio.is_some(),
                "allocation_limit memory basis requires max_memory_utilization_ratio"
            );
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionOutcome {
    Admit,
    Defer,
    NoEligibleWorker,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionExclusionCode {
    ExecutorNotCompatible,
    WorkerNotReady,
    AllocationCountExhausted,
    InsufficientCpu,
    InsufficientMemory,
    CpuReserveViolation,
    MemoryReserveViolation,
    CpuPressureUnknown,
    CpuPressureExceeded,
    MemoryPressureUnknown,
    MemoryPressureExceeded,
    IoPressureUnknown,
    IoPressureExceeded,
    ExclusiveCpuEvidenceUnavailable,
    DedicatedHostOccupied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdmissionExclusion {
    pub code: AdmissionExclusionCode,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkerAdmission {
    pub worker_id: String,
    pub executor_id: String,
    pub state_revision: u64,
    pub admissible: bool,
    pub available_before: RunnerShape,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_after: Option<RunnerShape>,
    pub running_allocations: u32,
    pub reserved_allocations: u32,
    pub committed_allocations: u32,
    pub max_allocations: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure: Option<WorkerPressure>,
    pub exclusions: Vec<AdmissionExclusion>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdmissionReport {
    pub schema_version: u32,
    pub algorithm: String,
    pub policy_id: String,
    pub job: String,
    pub repository: Option<String>,
    pub plan: ExecutionPlan,
    pub worker_snapshot_observed_at: String,
    pub worker_snapshot_source: String,
    pub outcome: AdmissionOutcome,
    pub admissible_worker_ids: Vec<String>,
    pub workers: Vec<WorkerAdmission>,
}

impl AdmissionReport {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("read admission report {}", path.display()))?;
        let report: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse admission report {}", path.display()))?;
        report.validate()?;
        Ok(report)
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.schema_version == ADMISSION_REPORT_SCHEMA_VERSION,
            "unsupported admission report schema version {}",
            self.schema_version
        );
        anyhow::ensure!(
            !self.algorithm.trim().is_empty(),
            "admission algorithm is required"
        );
        anyhow::ensure!(
            !self.policy_id.trim().is_empty(),
            "admission policy_id is required"
        );
        self.plan.validate()?;
        anyhow::ensure!(
            self.job == self.plan.job,
            "admission job does not match embedded ExecutionPlan"
        );
        anyhow::ensure!(
            self.repository == self.plan.repository,
            "admission repository does not match embedded ExecutionPlan"
        );

        let mut worker_ids = std::collections::BTreeSet::new();
        let mut admissible = Vec::new();
        for worker in &self.workers {
            anyhow::ensure!(
                !worker.worker_id.trim().is_empty(),
                "admission worker_id is required"
            );
            anyhow::ensure!(
                worker_ids.insert(worker.worker_id.as_str()),
                "duplicate admission worker_id {}",
                worker.worker_id
            );
            anyhow::ensure!(
                worker.state_revision > 0,
                "admission worker {} state_revision must be positive",
                worker.worker_id
            );
            anyhow::ensure!(
                worker.admissible == worker.exclusions.is_empty(),
                "admission worker {} admissible flag does not match exclusions",
                worker.worker_id
            );
            if worker.admissible {
                admissible.push(worker.worker_id.clone());
            }
        }
        admissible.sort();
        let mut declared = self.admissible_worker_ids.clone();
        declared.sort();
        anyhow::ensure!(
            declared == admissible,
            "admissible_worker_ids do not match worker admission evidence"
        );
        anyhow::ensure!(
            declared.windows(2).all(|pair| pair[0] != pair[1]),
            "admissible_worker_ids contain duplicates"
        );

        let expected_outcome = if !declared.is_empty() {
            AdmissionOutcome::Admit
        } else if self.workers.iter().any(|worker| {
            worker
                .exclusions
                .iter()
                .all(|reason| reason.code != AdmissionExclusionCode::ExecutorNotCompatible)
        }) {
            AdmissionOutcome::Defer
        } else {
            AdmissionOutcome::NoEligibleWorker
        };
        anyhow::ensure!(
            self.outcome == expected_outcome,
            "admission outcome does not match worker evidence"
        );

        Ok(())
    }
}

pub fn evaluate(
    plan: &ExecutionPlan,
    binding: &BindingReport,
    snapshot: &WorkerStateSnapshot,
    policy: &AdmissionPolicy,
) -> Result<AdmissionReport> {
    plan.validate()?;
    binding.validate()?;
    snapshot.validate()?;
    policy.validate()?;

    anyhow::ensure!(
        binding.plan == *plan,
        "BindingReport was produced for a different ExecutionPlan"
    );

    let mut workers = snapshot.workers.iter().collect::<Vec<_>>();
    workers.sort_by(|left, right| left.worker_id.cmp(&right.worker_id));

    let mut evaluations = Vec::with_capacity(workers.len());
    let mut compatible_worker_seen = false;

    for worker in workers {
        let executor_compatible = binding
            .compatible_executor_ids
            .iter()
            .any(|id| id == &worker.executor_id);
        compatible_worker_seen |= executor_compatible;
        evaluations.push(evaluate_worker(plan, worker, policy, executor_compatible)?);
    }

    let admissible_worker_ids = evaluations
        .iter()
        .filter(|worker| worker.admissible)
        .map(|worker| worker.worker_id.clone())
        .collect::<Vec<_>>();

    let outcome = if !admissible_worker_ids.is_empty() {
        AdmissionOutcome::Admit
    } else if compatible_worker_seen {
        AdmissionOutcome::Defer
    } else {
        AdmissionOutcome::NoEligibleWorker
    };

    let report = AdmissionReport {
        schema_version: ADMISSION_REPORT_SCHEMA_VERSION,
        algorithm: ADMISSION_ALGORITHM_VERSION.into(),
        policy_id: policy.policy_id.clone(),
        job: plan.job.clone(),
        repository: plan.repository.clone(),
        plan: plan.clone(),
        worker_snapshot_observed_at: snapshot.observed_at.clone(),
        worker_snapshot_source: snapshot.source.clone(),
        outcome,
        admissible_worker_ids,
        workers: evaluations,
    };
    report.validate()?;
    Ok(report)
}

fn evaluate_worker(
    plan: &ExecutionPlan,
    worker: &WorkerState,
    policy: &AdmissionPolicy,
    executor_compatible: bool,
) -> Result<WorkerAdmission> {
    let available_before = effective_available_capacity(plan, worker, policy)?;
    let mut exclusions = Vec::new();

    if !executor_compatible {
        exclusions.push(AdmissionExclusion {
            code: AdmissionExclusionCode::ExecutorNotCompatible,
            detail: format!(
                "executor {} is not compatible with this ExecutionPlan",
                worker.executor_id
            ),
        });
    }

    if worker.lifecycle != WorkerLifecycle::Ready {
        exclusions.push(AdmissionExclusion {
            code: AdmissionExclusionCode::WorkerNotReady,
            detail: format!("worker lifecycle is {:?}", worker.lifecycle),
        });
    }

    let committed_allocations = worker.committed_allocations()?;
    if committed_allocations >= worker.max_allocations {
        exclusions.push(AdmissionExclusion {
            code: AdmissionExclusionCode::AllocationCountExhausted,
            detail: format!(
                "worker has {}/{} committed running + reserved allocations",
                committed_allocations, worker.max_allocations
            ),
        });
    }

    if plan.cpu_tenancy == CpuTenancy::Exclusive {
        exclusions.push(AdmissionExclusion {
            code: AdmissionExclusionCode::ExclusiveCpuEvidenceUnavailable,
            detail: "worker state does not yet prove exclusive CPU/core availability".into(),
        });
    }

    if plan.host_tenancy == HostTenancy::Dedicated && committed_allocations > 0 {
        exclusions.push(AdmissionExclusion {
            code: AdmissionExclusionCode::DedicatedHostOccupied,
            detail: format!(
                "dedicated-host plan requires an empty worker but {} allocations are committed",
                committed_allocations
            ),
        });
    }

    if available_before.cpu_millis < plan.target_runner.cpu_millis {
        exclusions.push(AdmissionExclusion {
            code: AdmissionExclusionCode::InsufficientCpu,
            detail: format!(
                "requires {}m CPU but policy-eligible available CPU is {}m",
                plan.target_runner.cpu_millis, available_before.cpu_millis
            ),
        });
    }

    if available_before.memory_bytes < plan.target_runner.memory_bytes {
        exclusions.push(AdmissionExclusion {
            code: AdmissionExclusionCode::InsufficientMemory,
            detail: format!(
                "requires {} bytes memory but policy-eligible available memory is {}",
                plan.target_runner.memory_bytes, available_before.memory_bytes
            ),
        });
    }

    let available_after = if available_before.cpu_millis >= plan.target_runner.cpu_millis
        && available_before.memory_bytes >= plan.target_runner.memory_bytes
    {
        Some(RunnerShape::new(
            available_before.cpu_millis - plan.target_runner.cpu_millis,
            available_before.memory_bytes - plan.target_runner.memory_bytes,
        ))
    } else {
        None
    };

    if let Some(after) = &available_after {
        if after.cpu_millis < policy.reserve_cpu_millis {
            exclusions.push(AdmissionExclusion {
                code: AdmissionExclusionCode::CpuReserveViolation,
                detail: format!(
                    "admission would leave {}m CPU below {}m policy reserve",
                    after.cpu_millis, policy.reserve_cpu_millis
                ),
            });
        }
        if after.memory_bytes < policy.reserve_memory_bytes {
            exclusions.push(AdmissionExclusion {
                code: AdmissionExclusionCode::MemoryReserveViolation,
                detail: format!(
                    "admission would leave {} bytes memory below {} policy reserve",
                    after.memory_bytes, policy.reserve_memory_bytes
                ),
            });
        }
    }

    evaluate_pressure(worker.pressure.as_ref(), policy, &mut exclusions);

    Ok(WorkerAdmission {
        worker_id: worker.worker_id.clone(),
        executor_id: worker.executor_id.clone(),
        state_revision: worker.state_revision,
        admissible: exclusions.is_empty(),
        available_before,
        available_after,
        running_allocations: worker.running_allocations,
        reserved_allocations: worker.reserved_allocations,
        committed_allocations,
        max_allocations: worker.max_allocations,
        pressure: worker.pressure.clone(),
        exclusions,
    })
}

fn effective_available_capacity(
    plan: &ExecutionPlan,
    worker: &WorkerState,
    policy: &AdmissionPolicy,
) -> Result<RunnerShape> {
    worker.available_capacity()?;

    let cpu_ceiling = if plan.host_tenancy == HostTenancy::Dedicated {
        worker
            .physical_capacity
            .cpu_millis
            .min(worker.allocation_limit.cpu_millis)
    } else {
        match policy.cpu_capacity_basis {
            CapacityBasis::Physical => worker
                .physical_capacity
                .cpu_millis
                .min(worker.allocation_limit.cpu_millis),
            CapacityBasis::AllocationLimit => worker.allocation_limit.cpu_millis,
        }
    };

    let memory_ceiling = if plan.host_tenancy == HostTenancy::Dedicated {
        worker
            .physical_capacity
            .memory_bytes
            .min(worker.allocation_limit.memory_bytes)
    } else {
        match policy.memory_capacity_basis {
            CapacityBasis::Physical => worker
                .physical_capacity
                .memory_bytes
                .min(worker.allocation_limit.memory_bytes),
            CapacityBasis::AllocationLimit => worker.allocation_limit.memory_bytes,
        }
    };

    let committed = worker.committed_capacity()?;
    Ok(RunnerShape::new(
        cpu_ceiling.saturating_sub(committed.cpu_millis),
        memory_ceiling.saturating_sub(committed.memory_bytes),
    ))
}

fn evaluate_pressure(
    pressure: Option<&WorkerPressure>,
    policy: &AdmissionPolicy,
    exclusions: &mut Vec<AdmissionExclusion>,
) {
    evaluate_pressure_metric(
        "CPU utilization",
        pressure.and_then(|value| value.cpu_utilization_ratio),
        policy.max_cpu_utilization_ratio,
        AdmissionExclusionCode::CpuPressureUnknown,
        AdmissionExclusionCode::CpuPressureExceeded,
        exclusions,
    );
    evaluate_pressure_metric(
        "memory utilization",
        pressure.and_then(|value| value.memory_utilization_ratio),
        policy.max_memory_utilization_ratio,
        AdmissionExclusionCode::MemoryPressureUnknown,
        AdmissionExclusionCode::MemoryPressureExceeded,
        exclusions,
    );
    evaluate_pressure_metric(
        "I/O pressure",
        pressure.and_then(|value| value.io_pressure_ratio),
        policy.max_io_pressure_ratio,
        AdmissionExclusionCode::IoPressureUnknown,
        AdmissionExclusionCode::IoPressureExceeded,
        exclusions,
    );
}

fn evaluate_pressure_metric(
    label: &str,
    observed: Option<f64>,
    threshold: Option<f64>,
    unknown_code: AdmissionExclusionCode,
    exceeded_code: AdmissionExclusionCode,
    exclusions: &mut Vec<AdmissionExclusion>,
) {
    let Some(threshold) = threshold else {
        return;
    };

    match observed {
        Some(observed) if observed <= threshold => {}
        Some(observed) => exclusions.push(AdmissionExclusion {
            code: exceeded_code,
            detail: format!(
                "{label} {:.3} exceeds policy threshold {:.3}",
                observed, threshold
            ),
        }),
        None => exclusions.push(AdmissionExclusion {
            code: unknown_code,
            detail: format!("{label} evidence is required by policy but missing"),
        }),
    }
}

fn validate_threshold(field: &str, value: Option<f64>) -> Result<()> {
    if let Some(value) = value {
        anyhow::ensure!(
            value.is_finite() && value > 0.0 && value <= 1.0,
            "{field} must be finite and in (0, 1]"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::{BINDING_ALGORITHM_VERSION, BINDING_REPORT_SCHEMA_VERSION, ExecutorFit};
    use crate::execution::{CacheRequirement, EXECUTION_PLAN_SCHEMA_VERSION, ExecutionEnvironment};
    use crate::model::GIB;
    use crate::worker_state::{WORKER_STATE_SNAPSHOT_SCHEMA_VERSION, WorkerState};

    fn plan() -> ExecutionPlan {
        ExecutionPlan {
            schema_version: EXECUTION_PLAN_SCHEMA_VERSION,
            planner_version: "execution-plan-v1".into(),
            requirements_id: "untrusted-pr-v1".into(),
            job: "test".into(),
            repository: Some("acme/api".into()),
            target_runner: RunnerShape::new(4_000, 8 * GIB),
            predicted_p95_ms: 42_000.0,
            environment: ExecutionEnvironment::Microvm,
            cpu_tenancy: CpuTenancy::Shared,
            host_tenancy: HostTenancy::Shared,
            cache: Some(CacheRequirement::WarmPreferred),
            max_parallelism: Some(4),
            placement: None,
            sizing_algorithm: "deterministic-fit-v1@default-v1:policy-schema-v1".into(),
        }
    }

    fn binding(plan: &ExecutionPlan) -> BindingReport {
        BindingReport {
            schema_version: BINDING_REPORT_SCHEMA_VERSION,
            algorithm: BINDING_ALGORITHM_VERSION.into(),
            catalog_id: "test-v1".into(),
            job: plan.job.clone(),
            repository: plan.repository.clone(),
            plan: plan.clone(),
            compatible_executor_ids: vec!["microvm-x86_64".into()],
            fits: vec![ExecutorFit {
                executor_id: "microvm-x86_64".into(),
                compatible: true,
                exclusions: vec![],
                preferences: vec![],
            }],
        }
    }

    fn worker(id: &str) -> WorkerState {
        WorkerState {
            worker_id: id.into(),
            executor_id: "microvm-x86_64".into(),
            state_revision: 7,
            lifecycle: WorkerLifecycle::Ready,
            physical_capacity: RunnerShape::new(32_000, 128 * GIB),
            allocation_limit: RunnerShape::new(48_000, 120 * GIB),
            allocated_capacity: RunnerShape::new(20_000, 40 * GIB),
            reserved_capacity: RunnerShape::new(0, 0),
            running_allocations: 6,
            reserved_allocations: 0,
            max_allocations: 16,
            pressure: Some(WorkerPressure {
                cpu_utilization_ratio: Some(0.42),
                memory_utilization_ratio: Some(0.31),
                io_pressure_ratio: Some(0.08),
            }),
        }
    }

    fn snapshot(workers: Vec<WorkerState>) -> WorkerStateSnapshot {
        WorkerStateSnapshot {
            schema_version: WORKER_STATE_SNAPSHOT_SCHEMA_VERSION,
            observed_at: "2026-09-29T18:00:00Z".into(),
            source: "fixture".into(),
            workers,
        }
    }

    fn physical_policy() -> AdmissionPolicy {
        AdmissionPolicy {
            schema_version: ADMISSION_POLICY_SCHEMA_VERSION,
            policy_id: "physical-v1".into(),
            cpu_capacity_basis: CapacityBasis::Physical,
            memory_capacity_basis: CapacityBasis::Physical,
            reserve_cpu_millis: 1_000,
            reserve_memory_bytes: GIB,
            max_cpu_utilization_ratio: Some(0.90),
            max_memory_utilization_ratio: Some(0.90),
            max_io_pressure_ratio: Some(0.90),
        }
    }

    #[test]
    fn admits_compatible_ready_worker_with_capacity_and_low_pressure() {
        let plan = plan();
        let report = evaluate(
            &plan,
            &binding(&plan),
            &snapshot(vec![worker("worker-a")]),
            &physical_policy(),
        )
        .expect("admission");

        assert_eq!(report.outcome, AdmissionOutcome::Admit);
        assert_eq!(report.admissible_worker_ids, vec!["worker-a".to_string()]);
        assert!(report.workers[0].admissible);
        assert_eq!(
            report.workers[0].available_before,
            RunnerShape::new(12_000, 80 * GIB)
        );
    }

    #[test]
    fn physical_basis_does_not_silently_use_oversubscribed_cpu_limit() {
        let plan = plan();
        let mut busy = worker("worker-a");
        busy.allocated_capacity.cpu_millis = 30_000;

        let report = evaluate(
            &plan,
            &binding(&plan),
            &snapshot(vec![busy]),
            &physical_policy(),
        )
        .expect("admission");

        assert_eq!(report.outcome, AdmissionOutcome::Defer);
        assert!(
            report.workers[0]
                .exclusions
                .iter()
                .any(|reason| reason.code == AdmissionExclusionCode::InsufficientCpu)
        );
    }

    #[test]
    fn allocation_limit_basis_requires_pressure_threshold() {
        let mut policy = physical_policy();
        policy.cpu_capacity_basis = CapacityBasis::AllocationLimit;
        policy.max_cpu_utilization_ratio = None;

        let error = policy.validate().unwrap_err();

        assert!(
            error
                .to_string()
                .contains("requires max_cpu_utilization_ratio")
        );
    }

    #[test]
    fn allocation_limit_basis_can_explicitly_admit_beyond_physical_cpu() {
        let plan = plan();
        let mut busy = worker("worker-a");
        busy.allocated_capacity.cpu_millis = 30_000;
        let mut policy = physical_policy();
        policy.cpu_capacity_basis = CapacityBasis::AllocationLimit;

        let report =
            evaluate(&plan, &binding(&plan), &snapshot(vec![busy]), &policy).expect("admission");

        assert_eq!(report.outcome, AdmissionOutcome::Admit);
        assert_eq!(report.workers[0].available_before.cpu_millis, 18_000);
    }

    #[test]
    fn missing_required_pressure_evidence_defers() {
        let plan = plan();
        let mut candidate = worker("worker-a");
        candidate.pressure = None;

        let report = evaluate(
            &plan,
            &binding(&plan),
            &snapshot(vec![candidate]),
            &physical_policy(),
        )
        .expect("admission");

        assert_eq!(report.outcome, AdmissionOutcome::Defer);
        assert!(
            report.workers[0]
                .exclusions
                .iter()
                .any(|reason| reason.code == AdmissionExclusionCode::CpuPressureUnknown)
        );
    }

    #[test]
    fn no_compatible_executor_in_snapshot_is_no_eligible_worker() {
        let plan = plan();
        let mut candidate = worker("worker-a");
        candidate.executor_id = "container-x86_64".into();

        let report = evaluate(
            &plan,
            &binding(&plan),
            &snapshot(vec![candidate]),
            &physical_policy(),
        )
        .expect("admission");

        assert_eq!(report.outcome, AdmissionOutcome::NoEligibleWorker);
    }

    #[test]
    fn stale_binding_report_fails_closed() {
        let plan = plan();
        let mut stale_plan = plan.clone();
        stale_plan.target_runner = RunnerShape::new(8_000, 16 * GIB);
        let stale_binding = binding(&stale_plan);

        let error = evaluate(
            &plan,
            &stale_binding,
            &snapshot(vec![worker("worker-a")]),
            &physical_policy(),
        )
        .unwrap_err();

        assert!(error.to_string().contains("different ExecutionPlan"));
    }

    #[test]
    fn exclusive_cpu_fails_closed_without_core_allocation_evidence() {
        let mut plan = plan();
        plan.cpu_tenancy = CpuTenancy::Exclusive;
        let binding = binding(&plan);

        let report = evaluate(
            &plan,
            &binding,
            &snapshot(vec![worker("worker-a")]),
            &physical_policy(),
        )
        .expect("admission");

        assert_eq!(report.outcome, AdmissionOutcome::Defer);
        assert!(report.workers[0].exclusions.iter().any(|reason| {
            reason.code == AdmissionExclusionCode::ExclusiveCpuEvidenceUnavailable
        }));
    }

    #[test]
    fn dedicated_host_requires_empty_worker() {
        let mut plan = plan();
        plan.host_tenancy = HostTenancy::Dedicated;
        let binding = binding(&plan);

        let report = evaluate(
            &plan,
            &binding,
            &snapshot(vec![worker("worker-a")]),
            &physical_policy(),
        )
        .expect("admission");

        assert_eq!(report.outcome, AdmissionOutcome::Defer);
        assert!(
            report.workers[0]
                .exclusions
                .iter()
                .any(|reason| reason.code == AdmissionExclusionCode::DedicatedHostOccupied)
        );
    }

    #[test]
    fn workers_are_reported_in_stable_id_order() {
        let plan = plan();
        let report = evaluate(
            &plan,
            &binding(&plan),
            &snapshot(vec![worker("worker-z"), worker("worker-a")]),
            &physical_policy(),
        )
        .expect("admission");

        assert_eq!(report.workers[0].worker_id, "worker-a");
        assert_eq!(report.workers[1].worker_id, "worker-z");
    }
}
