use crate::admission::AdmissionReport;
use crate::execution::ExecutionPlan;
use crate::model::RunnerShape;
use crate::worker_state::WorkerState;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

pub const RESERVATION_POLICY_SCHEMA_VERSION: u32 = 1;
pub const RESERVATION_REQUEST_SCHEMA_VERSION: u32 = 1;
pub const RESERVATION_LEASE_SCHEMA_VERSION: u32 = 1;
pub const RESERVATION_LEDGER_SCHEMA_VERSION: u32 = 1;
pub const RESERVATION_TRANSITION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReservationPolicy {
    pub schema_version: u32,
    pub policy_id: String,
    pub min_lease_ttl_ms: u64,
    pub max_lease_ttl_ms: u64,
}

impl ReservationPolicy {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("read reservation policy {}", path.display()))?;
        let policy: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse reservation policy {}", path.display()))?;
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.schema_version == RESERVATION_POLICY_SCHEMA_VERSION,
            "unsupported reservation policy schema version {}",
            self.schema_version
        );
        anyhow::ensure!(!self.policy_id.trim().is_empty(), "policy_id is required");
        anyhow::ensure!(self.min_lease_ttl_ms > 0, "min_lease_ttl_ms must be positive");
        anyhow::ensure!(
            self.max_lease_ttl_ms >= self.min_lease_ttl_ms,
            "max_lease_ttl_ms must be >= min_lease_ttl_ms"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReservationRequest {
    pub schema_version: u32,
    pub request_id: String,
    pub worker_id: String,
    pub expected_worker_state_revision: u64,
    pub lease_ttl_ms: u64,
}

impl ReservationRequest {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("read reservation request {}", path.display()))?;
        let request: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse reservation request {}", path.display()))?;
        request.validate()?;
        Ok(request)
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.schema_version == RESERVATION_REQUEST_SCHEMA_VERSION,
            "unsupported reservation request schema version {}",
            self.schema_version
        );
        anyhow::ensure!(!self.request_id.trim().is_empty(), "request_id is required");
        anyhow::ensure!(!self.worker_id.trim().is_empty(), "worker_id is required");
        anyhow::ensure!(
            self.expected_worker_state_revision > 0,
            "expected_worker_state_revision must be positive"
        );
        anyhow::ensure!(self.lease_ttl_ms > 0, "lease_ttl_ms must be positive");
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaseStatus {
    Active,
    Released,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReservationLease {
    pub schema_version: u32,
    pub lease_id: String,
    pub request_id: String,
    pub worker_id: String,
    pub executor_id: String,
    pub plan: ExecutionPlan,
    pub reserved_capacity: RunnerShape,
    pub created_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    pub requested_ttl_ms: u64,
    pub requested_worker_state_revision: u64,
    pub worker_state_revision_before: u64,
    pub worker_state_revision_after: u64,
    pub status: LeaseStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_at_unix_ms: Option<u64>,
}

impl ReservationLease {
    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.schema_version == RESERVATION_LEASE_SCHEMA_VERSION,
            "unsupported reservation lease schema version {}",
            self.schema_version
        );
        anyhow::ensure!(!self.lease_id.trim().is_empty(), "lease_id is required");
        anyhow::ensure!(!self.request_id.trim().is_empty(), "request_id is required");
        anyhow::ensure!(!self.worker_id.trim().is_empty(), "worker_id is required");
        anyhow::ensure!(!self.executor_id.trim().is_empty(), "executor_id is required");
        self.plan.validate()?;
        anyhow::ensure!(
            self.reserved_capacity == self.plan.target_runner,
            "lease reserved capacity does not match ExecutionPlan target"
        );
        anyhow::ensure!(
            self.expires_at_unix_ms > self.created_at_unix_ms,
            "lease expiry must be after creation"
        );
        anyhow::ensure!(
            self.requested_ttl_ms == self.expires_at_unix_ms - self.created_at_unix_ms,
            "lease requested TTL does not match timestamps"
        );
        anyhow::ensure!(
            self.requested_worker_state_revision == self.worker_state_revision_before,
            "lease requested revision does not match revision before"
        );
        anyhow::ensure!(
            self.worker_state_revision_after == self.worker_state_revision_before + 1,
            "lease reservation must advance worker revision exactly once"
        );
        match self.status {
            LeaseStatus::Active => anyhow::ensure!(
                self.terminal_at_unix_ms.is_none(),
                "active lease must not have terminal timestamp"
            ),
            LeaseStatus::Released | LeaseStatus::Expired => anyhow::ensure!(
                self.terminal_at_unix_ms.is_some(),
                "terminal lease must have terminal timestamp"
            ),
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReservationLedger {
    pub schema_version: u32,
    pub leases: Vec<ReservationLease>,
}

impl ReservationLedger {
    pub fn empty() -> Self {
        Self {
            schema_version: RESERVATION_LEDGER_SCHEMA_VERSION,
            leases: Vec::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("read reservation ledger {}", path.display()))?;
        let ledger: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse reservation ledger {}", path.display()))?;
        ledger.validate()?;
        Ok(ledger)
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.schema_version == RESERVATION_LEDGER_SCHEMA_VERSION,
            "unsupported reservation ledger schema version {}",
            self.schema_version
        );
        let mut lease_ids = BTreeSet::new();
        let mut request_ids = BTreeSet::new();
        for lease in &self.leases {
            lease.validate()?;
            anyhow::ensure!(
                lease_ids.insert(lease.lease_id.as_str()),
                "duplicate lease_id {}",
                lease.lease_id
            );
            anyhow::ensure!(
                request_ids.insert(lease.request_id.as_str()),
                "duplicate reservation request_id {}",
                lease.request_id
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReservationAction {
    Reserve,
    Release,
    Expire,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReservationOutcome {
    Reserved,
    IdempotentReplay,
    StaleWorkerState,
    WorkerNotAdmissible,
    Released,
    AlreadyTerminal,
    Expired,
    Noop,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReservationTransition {
    pub schema_version: u32,
    pub action: ReservationAction,
    pub outcome: ReservationOutcome,
    pub worker_before: WorkerState,
    pub worker_after: WorkerState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease: Option<ReservationLease>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affected_lease_ids: Vec<String>,
    pub ledger_after: ReservationLedger,
}

pub fn reserve(
    current_worker: &WorkerState,
    admission: &AdmissionReport,
    ledger: &ReservationLedger,
    request: &ReservationRequest,
    policy: &ReservationPolicy,
    now_unix_ms: u64,
) -> Result<ReservationTransition> {
    current_worker.validate()?;
    admission.validate()?;
    ledger.validate()?;
    request.validate()?;
    policy.validate()?;
    validate_ttl(request.lease_ttl_ms, policy)?;

    if let Some(existing) = ledger
        .leases
        .iter()
        .find(|lease| lease.request_id == request.request_id)
    {
        ensure_same_request(existing, admission, request)?;
        return Ok(ReservationTransition {
            schema_version: RESERVATION_TRANSITION_SCHEMA_VERSION,
            action: ReservationAction::Reserve,
            outcome: ReservationOutcome::IdempotentReplay,
            worker_before: current_worker.clone(),
            worker_after: current_worker.clone(),
            lease: Some(existing.clone()),
            affected_lease_ids: vec![existing.lease_id.clone()],
            ledger_after: ledger.clone(),
        });
    }

    let admitted_worker = admission
        .workers
        .iter()
        .find(|worker| worker.worker_id == request.worker_id);

    let Some(admitted_worker) = admitted_worker else {
        return Ok(noop_reservation(
            current_worker,
            ledger,
            ReservationOutcome::WorkerNotAdmissible,
        ));
    };

    if !admitted_worker.admissible
        || !admission
            .admissible_worker_ids
            .iter()
            .any(|worker_id| worker_id == &request.worker_id)
    {
        return Ok(noop_reservation(
            current_worker,
            ledger,
            ReservationOutcome::WorkerNotAdmissible,
        ));
    }

    anyhow::ensure!(
        admitted_worker.executor_id == current_worker.executor_id,
        "current worker executor does not match admission evidence"
    );

    if admitted_worker.state_revision != request.expected_worker_state_revision
        || current_worker.state_revision != request.expected_worker_state_revision
    {
        return Ok(noop_reservation(
            current_worker,
            ledger,
            ReservationOutcome::StaleWorkerState,
        ));
    }

    anyhow::ensure!(
        current_worker.worker_id == request.worker_id,
        "current worker_id does not match reservation request"
    );

    let plan = &admission.plan;
    let available = current_worker.available_capacity()?;
    anyhow::ensure!(
        available.cpu_millis >= plan.target_runner.cpu_millis,
        "current worker no longer has enough reservable CPU"
    );
    anyhow::ensure!(
        available.memory_bytes >= plan.target_runner.memory_bytes,
        "current worker no longer has enough reservable memory"
    );
    anyhow::ensure!(
        current_worker.committed_allocations()? < current_worker.max_allocations,
        "current worker no longer has an allocation slot"
    );

    let expires_at_unix_ms = now_unix_ms
        .checked_add(request.lease_ttl_ms)
        .context("reservation expiry timestamp overflow")?;
    let next_revision = current_worker
        .state_revision
        .checked_add(1)
        .context("worker state revision overflow")?;

    let mut worker_after = current_worker.clone();
    worker_after.reserved_capacity = RunnerShape::new(
        worker_after
            .reserved_capacity
            .cpu_millis
            .checked_add(plan.target_runner.cpu_millis)
            .context("reserved CPU overflow")?,
        worker_after
            .reserved_capacity
            .memory_bytes
            .checked_add(plan.target_runner.memory_bytes)
            .context("reserved memory overflow")?,
    );
    worker_after.reserved_allocations = worker_after
        .reserved_allocations
        .checked_add(1)
        .context("reserved allocation count overflow")?;
    worker_after.state_revision = next_revision;
    worker_after.validate()?;

    let lease = ReservationLease {
        schema_version: RESERVATION_LEASE_SCHEMA_VERSION,
        lease_id: request.request_id.clone(),
        request_id: request.request_id.clone(),
        worker_id: request.worker_id.clone(),
        executor_id: current_worker.executor_id.clone(),
        plan: plan.clone(),
        reserved_capacity: plan.target_runner.clone(),
        created_at_unix_ms: now_unix_ms,
        expires_at_unix_ms,
        requested_ttl_ms: request.lease_ttl_ms,
        requested_worker_state_revision: request.expected_worker_state_revision,
        worker_state_revision_before: current_worker.state_revision,
        worker_state_revision_after: next_revision,
        status: LeaseStatus::Active,
        terminal_at_unix_ms: None,
    };
    lease.validate()?;

    let mut ledger_after = ledger.clone();
    ledger_after.leases.push(lease.clone());
    ledger_after
        .leases
        .sort_by(|left, right| left.lease_id.cmp(&right.lease_id));
    ledger_after.validate()?;

    Ok(ReservationTransition {
        schema_version: RESERVATION_TRANSITION_SCHEMA_VERSION,
        action: ReservationAction::Reserve,
        outcome: ReservationOutcome::Reserved,
        worker_before: current_worker.clone(),
        worker_after,
        lease: Some(lease.clone()),
        affected_lease_ids: vec![lease.lease_id],
        ledger_after,
    })
}

pub fn release(
    current_worker: &WorkerState,
    ledger: &ReservationLedger,
    lease_id: &str,
    now_unix_ms: u64,
) -> Result<ReservationTransition> {
    transition_terminal(
        current_worker,
        ledger,
        lease_id,
        now_unix_ms,
        LeaseStatus::Released,
        ReservationAction::Release,
        ReservationOutcome::Released,
    )
}

pub fn expire_due(
    current_worker: &WorkerState,
    ledger: &ReservationLedger,
    now_unix_ms: u64,
) -> Result<ReservationTransition> {
    current_worker.validate()?;
    ledger.validate()?;

    let due = ledger
        .leases
        .iter()
        .filter(|lease| {
            lease.worker_id == current_worker.worker_id
                && lease.status == LeaseStatus::Active
                && lease.expires_at_unix_ms <= now_unix_ms
        })
        .map(|lease| lease.lease_id.clone())
        .collect::<Vec<_>>();

    if due.is_empty() {
        return Ok(ReservationTransition {
            schema_version: RESERVATION_TRANSITION_SCHEMA_VERSION,
            action: ReservationAction::Expire,
            outcome: ReservationOutcome::Noop,
            worker_before: current_worker.clone(),
            worker_after: current_worker.clone(),
            lease: None,
            affected_lease_ids: Vec::new(),
            ledger_after: ledger.clone(),
        });
    }

    let mut worker_after = current_worker.clone();
    let mut ledger_after = ledger.clone();

    for lease_id in &due {
        let lease = ledger_after
            .leases
            .iter_mut()
            .find(|lease| lease.lease_id == *lease_id)
            .context("due lease disappeared during expiry transition")?;
        subtract_lease(&mut worker_after, lease)?;
        lease.status = LeaseStatus::Expired;
        lease.terminal_at_unix_ms = Some(now_unix_ms);
    }

    worker_after.state_revision = worker_after
        .state_revision
        .checked_add(1)
        .context("worker state revision overflow")?;
    worker_after.validate()?;
    ledger_after.validate()?;

    Ok(ReservationTransition {
        schema_version: RESERVATION_TRANSITION_SCHEMA_VERSION,
        action: ReservationAction::Expire,
        outcome: ReservationOutcome::Expired,
        worker_before: current_worker.clone(),
        worker_after,
        lease: None,
        affected_lease_ids: due,
        ledger_after,
    })
}

fn transition_terminal(
    current_worker: &WorkerState,
    ledger: &ReservationLedger,
    lease_id: &str,
    now_unix_ms: u64,
    terminal_status: LeaseStatus,
    action: ReservationAction,
    success_outcome: ReservationOutcome,
) -> Result<ReservationTransition> {
    current_worker.validate()?;
    ledger.validate()?;
    anyhow::ensure!(!lease_id.trim().is_empty(), "lease_id is required");

    let existing = ledger
        .leases
        .iter()
        .find(|lease| lease.lease_id == lease_id)
        .with_context(|| format!("lease {lease_id} not found"))?;

    anyhow::ensure!(
        existing.worker_id == current_worker.worker_id,
        "lease worker does not match current worker"
    );

    if existing.status != LeaseStatus::Active {
        return Ok(ReservationTransition {
            schema_version: RESERVATION_TRANSITION_SCHEMA_VERSION,
            action,
            outcome: ReservationOutcome::AlreadyTerminal,
            worker_before: current_worker.clone(),
            worker_after: current_worker.clone(),
            lease: Some(existing.clone()),
            affected_lease_ids: vec![existing.lease_id.clone()],
            ledger_after: ledger.clone(),
        });
    }

    let mut worker_after = current_worker.clone();
    subtract_lease(&mut worker_after, existing)?;
    worker_after.state_revision = worker_after
        .state_revision
        .checked_add(1)
        .context("worker state revision overflow")?;
    worker_after.validate()?;

    let mut ledger_after = ledger.clone();
    let lease = ledger_after
        .leases
        .iter_mut()
        .find(|lease| lease.lease_id == lease_id)
        .context("lease disappeared during terminal transition")?;
    lease.status = terminal_status;
    lease.terminal_at_unix_ms = Some(now_unix_ms);
    let returned_lease = lease.clone();
    ledger_after.validate()?;

    Ok(ReservationTransition {
        schema_version: RESERVATION_TRANSITION_SCHEMA_VERSION,
        action,
        outcome: success_outcome,
        worker_before: current_worker.clone(),
        worker_after,
        lease: Some(returned_lease.clone()),
        affected_lease_ids: vec![returned_lease.lease_id],
        ledger_after,
    })
}

fn subtract_lease(worker: &mut WorkerState, lease: &ReservationLease) -> Result<()> {
    anyhow::ensure!(
        worker.reserved_capacity.cpu_millis >= lease.reserved_capacity.cpu_millis,
        "worker reserved CPU is smaller than lease reservation"
    );
    anyhow::ensure!(
        worker.reserved_capacity.memory_bytes >= lease.reserved_capacity.memory_bytes,
        "worker reserved memory is smaller than lease reservation"
    );
    anyhow::ensure!(
        worker.reserved_allocations > 0,
        "worker has no reserved allocation to release"
    );

    worker.reserved_capacity = RunnerShape::new(
        worker.reserved_capacity.cpu_millis - lease.reserved_capacity.cpu_millis,
        worker.reserved_capacity.memory_bytes - lease.reserved_capacity.memory_bytes,
    );
    worker.reserved_allocations -= 1;
    Ok(())
}

fn validate_ttl(ttl_ms: u64, policy: &ReservationPolicy) -> Result<()> {
    anyhow::ensure!(
        ttl_ms >= policy.min_lease_ttl_ms,
        "lease TTL {ttl_ms}ms is below policy minimum {}ms",
        policy.min_lease_ttl_ms
    );
    anyhow::ensure!(
        ttl_ms <= policy.max_lease_ttl_ms,
        "lease TTL {ttl_ms}ms exceeds policy maximum {}ms",
        policy.max_lease_ttl_ms
    );
    Ok(())
}

fn ensure_same_request(
    lease: &ReservationLease,
    admission: &AdmissionReport,
    request: &ReservationRequest,
) -> Result<()> {
    anyhow::ensure!(
        lease.worker_id == request.worker_id
            && lease.requested_worker_state_revision == request.expected_worker_state_revision
            && lease.requested_ttl_ms == request.lease_ttl_ms
            && lease.plan == admission.plan,
        "reservation request_id {} conflicts with an existing lease",
        request.request_id
    );
    Ok(())
}

fn noop_reservation(
    current_worker: &WorkerState,
    ledger: &ReservationLedger,
    outcome: ReservationOutcome,
) -> ReservationTransition {
    ReservationTransition {
        schema_version: RESERVATION_TRANSITION_SCHEMA_VERSION,
        action: ReservationAction::Reserve,
        outcome,
        worker_before: current_worker.clone(),
        worker_after: current_worker.clone(),
        lease: None,
        affected_lease_ids: Vec::new(),
        ledger_after: ledger.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admission::{
        ADMISSION_ALGORITHM_VERSION, ADMISSION_REPORT_SCHEMA_VERSION, AdmissionOutcome,
        WorkerAdmission,
    };
    use crate::execution::{
        CacheRequirement, CpuTenancy, EXECUTION_PLAN_SCHEMA_VERSION, ExecutionEnvironment,
        HostTenancy,
    };
    use crate::model::GIB;
    use crate::worker_state::WorkerLifecycle;

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

    fn worker() -> WorkerState {
        WorkerState {
            worker_id: "worker-a".into(),
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
            pressure: None,
        }
    }

    fn admission() -> AdmissionReport {
        let plan = plan();
        AdmissionReport {
            schema_version: ADMISSION_REPORT_SCHEMA_VERSION,
            algorithm: ADMISSION_ALGORITHM_VERSION.into(),
            policy_id: "test-policy".into(),
            job: plan.job.clone(),
            repository: plan.repository.clone(),
            plan,
            worker_snapshot_observed_at: "2026-09-29T18:00:00Z".into(),
            worker_snapshot_source: "fixture".into(),
            outcome: AdmissionOutcome::Admit,
            admissible_worker_ids: vec!["worker-a".into()],
            workers: vec![WorkerAdmission {
                worker_id: "worker-a".into(),
                executor_id: "microvm-x86_64".into(),
                state_revision: 7,
                admissible: true,
                available_before: RunnerShape::new(12_000, 80 * GIB),
                available_after: Some(RunnerShape::new(8_000, 72 * GIB)),
                running_allocations: 6,
                reserved_allocations: 0,
                committed_allocations: 6,
                max_allocations: 16,
                pressure: None,
                exclusions: vec![],
            }],
        }
    }

    fn request(id: &str) -> ReservationRequest {
        ReservationRequest {
            schema_version: RESERVATION_REQUEST_SCHEMA_VERSION,
            request_id: id.into(),
            worker_id: "worker-a".into(),
            expected_worker_state_revision: 7,
            lease_ttl_ms: 60_000,
        }
    }

    fn policy() -> ReservationPolicy {
        ReservationPolicy {
            schema_version: RESERVATION_POLICY_SCHEMA_VERSION,
            policy_id: "reservation-test-v1".into(),
            min_lease_ttl_ms: 1_000,
            max_lease_ttl_ms: 300_000,
        }
    }

    #[test]
    fn reservation_claims_capacity_slot_and_revision() {
        let transition = reserve(
            &worker(),
            &admission(),
            &ReservationLedger::empty(),
            &request("req-1"),
            &policy(),
            1_000_000,
        )
        .expect("reserve");

        assert_eq!(transition.outcome, ReservationOutcome::Reserved);
        assert_eq!(transition.worker_after.state_revision, 8);
        assert_eq!(
            transition.worker_after.reserved_capacity,
            RunnerShape::new(4_000, 8 * GIB)
        );
        assert_eq!(transition.worker_after.reserved_allocations, 1);
        assert_eq!(transition.ledger_after.leases.len(), 1);
        assert_eq!(
            transition.ledger_after.leases[0].status,
            LeaseStatus::Active
        );
    }

    #[test]
    fn second_request_from_same_admission_revision_is_stale() {
        let first = reserve(
            &worker(),
            &admission(),
            &ReservationLedger::empty(),
            &request("req-1"),
            &policy(),
            1_000_000,
        )
        .expect("first reserve");

        let second = reserve(
            &first.worker_after,
            &admission(),
            &first.ledger_after,
            &request("req-2"),
            &policy(),
            1_000_001,
        )
        .expect("second reserve");

        assert_eq!(second.outcome, ReservationOutcome::StaleWorkerState);
        assert_eq!(second.worker_after, first.worker_after);
    }

    #[test]
    fn exact_request_replay_is_idempotent() {
        let first = reserve(
            &worker(),
            &admission(),
            &ReservationLedger::empty(),
            &request("req-1"),
            &policy(),
            1_000_000,
        )
        .expect("first reserve");

        let replay = reserve(
            &first.worker_after,
            &admission(),
            &first.ledger_after,
            &request("req-1"),
            &policy(),
            1_000_100,
        )
        .expect("replay");

        assert_eq!(replay.outcome, ReservationOutcome::IdempotentReplay);
        assert_eq!(replay.worker_after, first.worker_after);
        assert_eq!(replay.ledger_after, first.ledger_after);
    }

    #[test]
    fn conflicting_request_id_fails_closed() {
        let first = reserve(
            &worker(),
            &admission(),
            &ReservationLedger::empty(),
            &request("req-1"),
            &policy(),
            1_000_000,
        )
        .expect("first reserve");

        let mut conflict = request("req-1");
        conflict.lease_ttl_ms = 120_000;

        let error = reserve(
            &first.worker_after,
            &admission(),
            &first.ledger_after,
            &conflict,
            &policy(),
            1_000_100,
        )
        .unwrap_err();

        assert!(error.to_string().contains("conflicts"));
    }

    #[test]
    fn release_restores_reserved_capacity_and_advances_revision() {
        let reserved = reserve(
            &worker(),
            &admission(),
            &ReservationLedger::empty(),
            &request("req-1"),
            &policy(),
            1_000_000,
        )
        .expect("reserve");

        let released = release(
            &reserved.worker_after,
            &reserved.ledger_after,
            "req-1",
            1_001_000,
        )
        .expect("release");

        assert_eq!(released.outcome, ReservationOutcome::Released);
        assert_eq!(released.worker_after.state_revision, 9);
        assert_eq!(released.worker_after.reserved_capacity, RunnerShape::new(0, 0));
        assert_eq!(released.worker_after.reserved_allocations, 0);
        assert_eq!(released.ledger_after.leases[0].status, LeaseStatus::Released);
    }

    #[test]
    fn expiry_restores_due_leases_and_keeps_future_leases() {
        let first = reserve(
            &worker(),
            &admission(),
            &ReservationLedger::empty(),
            &request("req-1"),
            &policy(),
            1_000_000,
        )
        .expect("reserve");

        let mut future_lease = first.ledger_after.leases[0].clone();
        future_lease.lease_id = "future".into();
        future_lease.request_id = "future".into();
        future_lease.created_at_unix_ms = 1_000_000;
        future_lease.expires_at_unix_ms = 1_200_000;
        future_lease.requested_ttl_ms = 200_000;

        let mut worker_with_two = first.worker_after.clone();
        worker_with_two.reserved_capacity = RunnerShape::new(8_000, 16 * GIB);
        worker_with_two.reserved_allocations = 2;
        worker_with_two.state_revision = 9;

        let mut ledger = first.ledger_after.clone();
        ledger.leases.push(future_lease);
        ledger.validate().expect("ledger");

        let expired = expire_due(&worker_with_two, &ledger, 1_060_000).expect("expire");

        assert_eq!(expired.outcome, ReservationOutcome::Expired);
        assert_eq!(expired.affected_lease_ids, vec!["req-1".to_string()]);
        assert_eq!(
            expired.worker_after.reserved_capacity,
            RunnerShape::new(4_000, 8 * GIB)
        );
        assert_eq!(expired.worker_after.reserved_allocations, 1);
        assert_eq!(expired.worker_after.state_revision, 10);
        assert_eq!(
            expired
                .ledger_after
                .leases
                .iter()
                .find(|lease| lease.lease_id == "req-1")
                .expect("expired lease")
                .status,
            LeaseStatus::Expired
        );
    }

    #[test]
    fn ttl_outside_policy_fails_closed() {
        let mut too_long = request("req-1");
        too_long.lease_ttl_ms = 600_000;

        let error = reserve(
            &worker(),
            &admission(),
            &ReservationLedger::empty(),
            &too_long,
            &policy(),
            1_000_000,
        )
        .unwrap_err();

        assert!(error.to_string().contains("exceeds policy maximum"));
    }
}
