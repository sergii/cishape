use crate::admission::AdmissionReport;
use crate::reservation::{
    ReservationLedger, ReservationLease, ReservationOutcome, ReservationPolicy,
    ReservationRequest, ReservationTransition, expire_due as expire_transition,
    release as release_transition, reserve as reserve_transition,
};
use crate::worker_state::{WorkerState, WorkerStateSnapshot};
use anyhow::{Context, Result};
use duckdb::{Connection, Transaction, params};
use std::path::Path;

pub trait ReservationStore {
    fn seed_workers(&mut self, snapshot: &WorkerStateSnapshot) -> Result<usize>;
    fn worker(&self, worker_id: &str) -> Result<WorkerState>;
    fn ledger(&self) -> Result<ReservationLedger>;
    fn reserve(
        &mut self,
        admission: &AdmissionReport,
        request: &ReservationRequest,
        policy: &ReservationPolicy,
        now_unix_ms: u64,
    ) -> Result<ReservationTransition>;
    fn release(&mut self, lease_id: &str, now_unix_ms: u64) -> Result<ReservationTransition>;
    fn expire_due(
        &mut self,
        worker_id: &str,
        now_unix_ms: u64,
    ) -> Result<ReservationTransition>;
}

pub struct DuckDbReservationStore {
    connection: Connection,
}

impl DuckDbReservationStore {
    pub fn memory() -> Result<Self> {
        let connection =
            Connection::open_in_memory().context("open in-memory reservation DuckDB")?;
        let store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open(path)
            .with_context(|| format!("open reservation DuckDB at {}", path.display()))?;
        let store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        self.connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS reservation_workers (
                worker_id VARCHAR PRIMARY KEY,
                executor_id VARCHAR NOT NULL,
                state_revision BIGINT NOT NULL,
                state_json VARCHAR NOT NULL
            );

            CREATE TABLE IF NOT EXISTS reservation_leases (
                lease_id VARCHAR PRIMARY KEY,
                request_id VARCHAR NOT NULL UNIQUE,
                worker_id VARCHAR NOT NULL,
                status VARCHAR NOT NULL,
                expires_at_unix_ms BIGINT NOT NULL,
                lease_json VARCHAR NOT NULL
            );
            "#,
        )?;
        Ok(())
    }

    fn load_worker_from_connection(&self, worker_id: &str) -> Result<WorkerState> {
        let mut statement = self.connection.prepare(
            "SELECT executor_id, state_revision, state_json
             FROM reservation_workers
             WHERE worker_id = ?",
        )?;
        let rows = statement.query_map(params![worker_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let records = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        decode_worker_record(worker_id, records)
    }

    fn load_ledger_from_connection(&self) -> Result<ReservationLedger> {
        let mut statement = self.connection.prepare(
            "SELECT lease_id, request_id, worker_id, status, expires_at_unix_ms, lease_json
             FROM reservation_leases
             ORDER BY lease_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        let records = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        decode_ledger_records(records)
    }
}

impl ReservationStore for DuckDbReservationStore {
    fn seed_workers(&mut self, snapshot: &WorkerStateSnapshot) -> Result<usize> {
        snapshot.validate()?;
        let tx = self.connection.transaction()?;
        let mut inserted = 0_usize;

        for worker in &snapshot.workers {
            let existing = load_worker_from_tx(&tx, &worker.worker_id)?;
            match existing {
                Some(existing) => {
                    anyhow::ensure!(
                        existing == *worker,
                        "worker {} already exists with different authoritative state",
                        worker.worker_id
                    );
                }
                None => {
                    insert_worker(&tx, worker)?;
                    inserted += 1;
                }
            }
        }

        tx.commit()?;
        Ok(inserted)
    }

    fn worker(&self, worker_id: &str) -> Result<WorkerState> {
        anyhow::ensure!(!worker_id.trim().is_empty(), "worker_id is required");
        self.load_worker_from_connection(worker_id)
    }

    fn ledger(&self) -> Result<ReservationLedger> {
        self.load_ledger_from_connection()
    }

    fn reserve(
        &mut self,
        admission: &AdmissionReport,
        request: &ReservationRequest,
        policy: &ReservationPolicy,
        now_unix_ms: u64,
    ) -> Result<ReservationTransition> {
        admission.validate()?;
        request.validate()?;
        policy.validate()?;

        let tx = self.connection.transaction()?;
        let worker = load_worker_from_tx(&tx, &request.worker_id)?
            .with_context(|| format!("worker {} not found", request.worker_id))?;
        let ledger = load_ledger_from_tx(&tx)?;
        let transition =
            reserve_transition(&worker, admission, &ledger, request, policy, now_unix_ms)?;

        if transition.outcome == ReservationOutcome::Reserved {
            persist_worker_cas(&tx, &transition.worker_after, worker.state_revision)?;
            let lease = transition
                .lease
                .as_ref()
                .context("reserved transition missing lease")?;
            insert_lease(&tx, lease)?;
        }

        tx.commit()?;
        Ok(transition)
    }

    fn release(&mut self, lease_id: &str, now_unix_ms: u64) -> Result<ReservationTransition> {
        anyhow::ensure!(!lease_id.trim().is_empty(), "lease_id is required");

        let tx = self.connection.transaction()?;
        let ledger = load_ledger_from_tx(&tx)?;
        let lease = ledger
            .leases
            .iter()
            .find(|lease| lease.lease_id == lease_id)
            .with_context(|| format!("lease {lease_id} not found"))?;
        let worker = load_worker_from_tx(&tx, &lease.worker_id)?
            .with_context(|| format!("worker {} not found", lease.worker_id))?;
        let transition = release_transition(&worker, &ledger, lease_id, now_unix_ms)?;

        if transition.outcome == ReservationOutcome::Released {
            persist_worker_cas(&tx, &transition.worker_after, worker.state_revision)?;
            persist_terminal_lease(
                &tx,
                transition
                    .lease
                    .as_ref()
                    .context("released transition missing lease")?,
            )?;
        }

        tx.commit()?;
        Ok(transition)
    }

    fn expire_due(
        &mut self,
        worker_id: &str,
        now_unix_ms: u64,
    ) -> Result<ReservationTransition> {
        anyhow::ensure!(!worker_id.trim().is_empty(), "worker_id is required");

        let tx = self.connection.transaction()?;
        let worker = load_worker_from_tx(&tx, worker_id)?
            .with_context(|| format!("worker {worker_id} not found"))?;
        let ledger = load_ledger_from_tx(&tx)?;
        let transition = expire_transition(&worker, &ledger, now_unix_ms)?;

        if transition.outcome == ReservationOutcome::Expired {
            persist_worker_cas(&tx, &transition.worker_after, worker.state_revision)?;
            for lease_id in &transition.affected_lease_ids {
                let lease = transition
                    .ledger_after
                    .leases
                    .iter()
                    .find(|lease| &lease.lease_id == lease_id)
                    .with_context(|| format!("expired lease {lease_id} missing from transition"))?;
                persist_terminal_lease(&tx, lease)?;
            }
        }

        tx.commit()?;
        Ok(transition)
    }
}

fn load_worker_from_tx(tx: &Transaction<'_>, worker_id: &str) -> Result<Option<WorkerState>> {
    let mut statement = tx.prepare(
        "SELECT executor_id, state_revision, state_json
         FROM reservation_workers
         WHERE worker_id = ?",
    )?;
    let rows = statement.query_map(params![worker_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let records = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    if records.is_empty() {
        return Ok(None);
    }
    decode_worker_record(worker_id, records).map(Some)
}

fn load_ledger_from_tx(tx: &Transaction<'_>) -> Result<ReservationLedger> {
    let mut statement = tx.prepare(
        "SELECT lease_id, request_id, worker_id, status, expires_at_unix_ms, lease_json
         FROM reservation_leases
         ORDER BY lease_id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;
    let records = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    decode_ledger_records(records)
}

fn decode_worker_record(
    worker_id: &str,
    records: Vec<(String, i64, String)>,
) -> Result<WorkerState> {
    anyhow::ensure!(
        records.len() == 1,
        "expected exactly one authoritative worker row for {worker_id}, found {}",
        records.len()
    );
    let (executor_id, revision, json) = records.into_iter().next().context("worker row missing")?;
    let worker: WorkerState =
        serde_json::from_str(&json).context("parse authoritative WorkerState JSON")?;
    worker.validate()?;

    anyhow::ensure!(
        worker.worker_id == worker_id,
        "worker JSON identity does not match indexed worker_id"
    );
    anyhow::ensure!(
        worker.executor_id == executor_id,
        "worker JSON executor does not match indexed executor_id"
    );
    anyhow::ensure!(
        i64::try_from(worker.state_revision).context("worker revision exceeds DB range")?
            == revision,
        "worker JSON revision does not match indexed state_revision"
    );
    Ok(worker)
}

fn decode_ledger_records(
    records: Vec<(String, String, String, String, i64, String)>,
) -> Result<ReservationLedger> {
    let mut leases = Vec::with_capacity(records.len());
    for (lease_id, request_id, worker_id, status, expires_at, json) in records {
        let lease: ReservationLease =
            serde_json::from_str(&json).context("parse authoritative ReservationLease JSON")?;
        anyhow::ensure!(lease.lease_id == lease_id, "lease JSON lease_id mismatch");
        anyhow::ensure!(
            lease.request_id == request_id,
            "lease JSON request_id mismatch"
        );
        anyhow::ensure!(lease.worker_id == worker_id, "lease JSON worker_id mismatch");
        anyhow::ensure!(
            lease_status_name(&lease) == status,
            "lease JSON status mismatch"
        );
        anyhow::ensure!(
            i64::try_from(lease.expires_at_unix_ms).context("lease expiry exceeds DB range")?
                == expires_at,
            "lease JSON expiry mismatch"
        );
        leases.push(lease);
    }

    let ledger = ReservationLedger {
        schema_version: crate::reservation::RESERVATION_LEDGER_SCHEMA_VERSION,
        leases,
    };
    ledger.validate()?;
    Ok(ledger)
}

fn insert_worker(tx: &Transaction<'_>, worker: &WorkerState) -> Result<()> {
    worker.validate()?;
    let json = serde_json::to_string(worker).context("serialize WorkerState")?;
    let changed = tx.execute(
        "INSERT INTO reservation_workers (
            worker_id, executor_id, state_revision, state_json
         ) VALUES (?, ?, ?, ?)",
        params![
            worker.worker_id,
            worker.executor_id,
            i64::try_from(worker.state_revision).context("worker revision exceeds DB range")?,
            json
        ],
    )?;
    anyhow::ensure!(changed == 1, "failed to insert authoritative worker");
    Ok(())
}

fn persist_worker_cas(
    tx: &Transaction<'_>,
    worker_after: &WorkerState,
    expected_revision: u64,
) -> Result<()> {
    worker_after.validate()?;
    let json = serde_json::to_string(worker_after).context("serialize WorkerState")?;
    let changed = tx.execute(
        "UPDATE reservation_workers
         SET executor_id = ?, state_revision = ?, state_json = ?
         WHERE worker_id = ? AND state_revision = ?",
        params![
            worker_after.executor_id,
            i64::try_from(worker_after.state_revision)
                .context("worker revision exceeds DB range")?,
            json,
            worker_after.worker_id,
            i64::try_from(expected_revision).context("expected revision exceeds DB range")?
        ],
    )?;
    anyhow::ensure!(
        changed == 1,
        "authoritative worker CAS failed for {} at revision {}",
        worker_after.worker_id,
        expected_revision
    );
    Ok(())
}

fn insert_lease(tx: &Transaction<'_>, lease: &ReservationLease) -> Result<()> {
    let json = serde_json::to_string(lease).context("serialize ReservationLease")?;
    let changed = tx.execute(
        "INSERT INTO reservation_leases (
            lease_id, request_id, worker_id, status, expires_at_unix_ms, lease_json
         ) VALUES (?, ?, ?, ?, ?, ?)",
        params![
            lease.lease_id,
            lease.request_id,
            lease.worker_id,
            lease_status_name(lease),
            i64::try_from(lease.expires_at_unix_ms).context("lease expiry exceeds DB range")?,
            json
        ],
    )?;
    anyhow::ensure!(changed == 1, "failed to insert authoritative lease");
    Ok(())
}

fn persist_terminal_lease(tx: &Transaction<'_>, lease: &ReservationLease) -> Result<()> {
    let json = serde_json::to_string(lease).context("serialize ReservationLease")?;
    let changed = tx.execute(
        "UPDATE reservation_leases
         SET status = ?, expires_at_unix_ms = ?, lease_json = ?
         WHERE lease_id = ? AND request_id = ? AND status = 'active'",
        params![
            lease_status_name(lease),
            i64::try_from(lease.expires_at_unix_ms).context("lease expiry exceeds DB range")?,
            json,
            lease.lease_id,
            lease.request_id
        ],
    )?;
    anyhow::ensure!(
        changed == 1,
        "failed to persist terminal lease {}",
        lease.lease_id
    );
    Ok(())
}

fn lease_status_name(lease: &ReservationLease) -> &'static str {
    match &lease.status {
        crate::reservation::LeaseStatus::Active => "active",
        crate::reservation::LeaseStatus::Released => "released",
        crate::reservation::LeaseStatus::Expired => "expired",
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
        ExecutionPlan, HostTenancy,
    };
    use crate::model::{GIB, RunnerShape};
    use crate::reservation::{
        RESERVATION_POLICY_SCHEMA_VERSION, RESERVATION_REQUEST_SCHEMA_VERSION, LeaseStatus,
    };
    use crate::worker_state::{WORKER_STATE_SNAPSHOT_SCHEMA_VERSION, WorkerLifecycle};

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

    fn snapshot() -> WorkerStateSnapshot {
        WorkerStateSnapshot {
            schema_version: WORKER_STATE_SNAPSHOT_SCHEMA_VERSION,
            observed_at: "2026-09-29T18:00:00Z".into(),
            source: "fixture".into(),
            workers: vec![worker()],
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

    fn seeded_store() -> DuckDbReservationStore {
        let mut store = DuckDbReservationStore::memory().expect("store");
        assert_eq!(store.seed_workers(&snapshot()).expect("seed"), 1);
        store
    }

    #[test]
    fn successful_reserve_persists_worker_and_lease_together() {
        let mut store = seeded_store();

        let transition = store
            .reserve(&admission(), &request("req-1"), &policy(), 1_000_000)
            .expect("reserve");

        assert_eq!(transition.outcome, ReservationOutcome::Reserved);
        let persisted = store.worker("worker-a").expect("worker");
        assert_eq!(persisted.state_revision, 8);
        assert_eq!(persisted.reserved_capacity, RunnerShape::new(4_000, 8 * GIB));
        let ledger = store.ledger().expect("ledger");
        assert_eq!(ledger.leases.len(), 1);
        assert_eq!(ledger.leases[0].status, LeaseStatus::Active);
    }

    #[test]
    fn stale_revision_does_not_mutate_authoritative_state() {
        let mut store = seeded_store();
        let mut stale = request("req-stale");
        stale.expected_worker_state_revision = 6;

        let transition = store
            .reserve(&admission(), &stale, &policy(), 1_000_000)
            .expect("stale reserve");

        assert_eq!(transition.outcome, ReservationOutcome::StaleWorkerState);
        assert_eq!(store.worker("worker-a").expect("worker"), worker());
        assert!(store.ledger().expect("ledger").leases.is_empty());
    }

    #[test]
    fn exact_replay_is_idempotent_in_store() {
        let mut store = seeded_store();
        let first = store
            .reserve(&admission(), &request("req-1"), &policy(), 1_000_000)
            .expect("first");
        let replay = store
            .reserve(&admission(), &request("req-1"), &policy(), 1_000_100)
            .expect("replay");

        assert_eq!(first.outcome, ReservationOutcome::Reserved);
        assert_eq!(replay.outcome, ReservationOutcome::IdempotentReplay);
        assert_eq!(store.ledger().expect("ledger").leases.len(), 1);
        assert_eq!(
            store.worker("worker-a").expect("worker").reserved_capacity,
            RunnerShape::new(4_000, 8 * GIB)
        );
    }

    #[test]
    fn conflicting_request_id_fails_closed_without_mutation() {
        let mut store = seeded_store();
        store
            .reserve(&admission(), &request("req-1"), &policy(), 1_000_000)
            .expect("first");
        let before = store.worker("worker-a").expect("before");
        let mut conflict = request("req-1");
        conflict.lease_ttl_ms = 120_000;

        let error = store
            .reserve(&admission(), &conflict, &policy(), 1_000_100)
            .unwrap_err();

        assert!(error.to_string().contains("conflicts"));
        assert_eq!(store.worker("worker-a").expect("after"), before);
        assert_eq!(store.ledger().expect("ledger").leases.len(), 1);
    }

    #[test]
    fn release_restores_capacity_transactionally() {
        let mut store = seeded_store();
        store
            .reserve(&admission(), &request("req-1"), &policy(), 1_000_000)
            .expect("reserve");

        let released = store.release("req-1", 1_001_000).expect("release");

        assert_eq!(released.outcome, ReservationOutcome::Released);
        let worker = store.worker("worker-a").expect("worker");
        assert_eq!(worker.state_revision, 9);
        assert_eq!(worker.reserved_capacity, RunnerShape::new(0, 0));
        assert_eq!(
            store.ledger().expect("ledger").leases[0].status,
            LeaseStatus::Released
        );
    }

    #[test]
    fn expiry_restores_due_capacity_transactionally() {
        let mut store = seeded_store();
        store
            .reserve(&admission(), &request("req-1"), &policy(), 1_000_000)
            .expect("reserve");

        let expired = store.expire_due("worker-a", 1_060_000).expect("expire");

        assert_eq!(expired.outcome, ReservationOutcome::Expired);
        let worker = store.worker("worker-a").expect("worker");
        assert_eq!(worker.state_revision, 9);
        assert_eq!(worker.reserved_capacity, RunnerShape::new(0, 0));
        assert_eq!(
            store.ledger().expect("ledger").leases[0].status,
            LeaseStatus::Expired
        );
    }

    #[test]
    fn failed_lease_insert_rolls_back_worker_cas_mutation() {
        let mut store = seeded_store();
        let first = store
            .reserve(&admission(), &request("req-1"), &policy(), 1_000_000)
            .expect("first reserve");
        assert_eq!(first.outcome, ReservationOutcome::Reserved);

        store
            .connection
            .execute_batch(
                "CREATE UNIQUE INDEX test_one_lease_per_worker
                 ON reservation_leases(worker_id)",
            )
            .expect("test-only uniqueness constraint");

        let before = store.worker("worker-a").expect("before");
        let mut fresh_admission = admission();
        fresh_admission.workers[0].state_revision = before.state_revision;
        let mut second = request("req-2");
        second.expected_worker_state_revision = before.state_revision;

        let error = store
            .reserve(&fresh_admission, &second, &policy(), 1_000_100)
            .unwrap_err();

        assert!(
            error.to_string().contains("Constraint")
                || error.to_string().contains("constraint")
                || error.to_string().contains("unique")
        );
        assert_eq!(store.worker("worker-a").expect("after"), before);
        let ledger = store.ledger().expect("ledger");
        assert_eq!(ledger.leases.len(), 1);
        assert_eq!(ledger.leases[0].request_id, "req-1");
    }

    #[test]
    fn seeding_existing_worker_is_idempotent_but_cannot_overwrite_state() {
        let mut store = seeded_store();
        assert_eq!(store.seed_workers(&snapshot()).expect("idempotent seed"), 0);

        let mut changed = snapshot();
        changed.workers[0].state_revision = 8;
        let error = store.seed_workers(&changed).unwrap_err();

        assert!(error.to_string().contains("different authoritative state"));
        assert_eq!(store.worker("worker-a").expect("worker"), worker());
    }
}
