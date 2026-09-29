use crate::execution::{CpuTenancy, ExecutionEnvironment, ExecutionPlan, HostTenancy};
use crate::model::{GIB, RunnerShape};
use crate::reservation::{LeaseStatus, ReservationLease};
use crate::worker_state::WorkerState;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const EXECUTION_RECORD_SCHEMA_VERSION: u32 = 1;
pub const EXECUTION_TRANSITION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Starting,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupStatus {
    NotAttempted,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionRecord {
    pub schema_version: u32,
    pub execution_id: String,
    pub lease_id: String,
    pub worker_id: String,
    pub executor_id: String,
    pub backend: String,
    pub plan: ExecutionPlan,
    pub command: Vec<String>,
    pub status: ExecutionStatus,
    pub created_at_unix_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running_at_unix_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at_unix_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_resource_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stdout: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stderr: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub cleanup: CleanupStatus,
}

impl ExecutionRecord {
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.schema_version == EXECUTION_RECORD_SCHEMA_VERSION,
            "unsupported execution record schema version {}",
            self.schema_version
        );
        anyhow::ensure!(
            !self.execution_id.trim().is_empty(),
            "execution_id is required"
        );
        anyhow::ensure!(!self.lease_id.trim().is_empty(), "lease_id is required");
        anyhow::ensure!(!self.worker_id.trim().is_empty(), "worker_id is required");
        anyhow::ensure!(
            !self.executor_id.trim().is_empty(),
            "executor_id is required"
        );
        anyhow::ensure!(!self.backend.trim().is_empty(), "backend is required");
        self.plan.validate()?;
        anyhow::ensure!(!self.command.is_empty(), "execution command is required");
        anyhow::ensure!(
            self.command.iter().all(|part| !part.is_empty()),
            "execution command arguments must not be empty"
        );

        match self.status {
            ExecutionStatus::Starting => {
                anyhow::ensure!(
                    self.running_at_unix_ms.is_none(),
                    "starting execution cannot have running timestamp"
                );
                anyhow::ensure!(
                    self.completed_at_unix_ms.is_none(),
                    "starting execution cannot have completion timestamp"
                );
                anyhow::ensure!(
                    self.exit_code.is_none() && self.error.is_none(),
                    "starting execution cannot have terminal result"
                );
            }
            ExecutionStatus::Running => {
                let running = self
                    .running_at_unix_ms
                    .context("running execution must have running timestamp")?;
                anyhow::ensure!(
                    running >= self.created_at_unix_ms,
                    "running timestamp cannot precede creation"
                );
                anyhow::ensure!(
                    self.backend_resource_id.is_some(),
                    "running execution requires backend_resource_id"
                );
                anyhow::ensure!(
                    self.completed_at_unix_ms.is_none(),
                    "running execution cannot have completion timestamp"
                );
                anyhow::ensure!(
                    self.exit_code.is_none() && self.error.is_none(),
                    "running execution cannot have terminal result"
                );
            }
            ExecutionStatus::Succeeded => {
                let completed = self
                    .completed_at_unix_ms
                    .context("succeeded execution must have completion timestamp")?;
                anyhow::ensure!(
                    completed >= self.created_at_unix_ms,
                    "completion timestamp cannot precede creation"
                );
                anyhow::ensure!(
                    self.exit_code == Some(0),
                    "succeeded execution requires exit_code 0"
                );
                anyhow::ensure!(
                    self.error.is_none(),
                    "succeeded execution cannot contain error"
                );
                anyhow::ensure!(
                    self.cleanup == CleanupStatus::Succeeded,
                    "succeeded execution requires successful cleanup"
                );
            }
            ExecutionStatus::Failed => {
                let completed = self
                    .completed_at_unix_ms
                    .context("failed execution must have completion timestamp")?;
                anyhow::ensure!(
                    completed >= self.created_at_unix_ms,
                    "completion timestamp cannot precede creation"
                );
                anyhow::ensure!(
                    self.exit_code.is_some() || self.error.is_some(),
                    "failed execution requires exit code or error"
                );
            }
        }

        Ok(())
    }

    pub fn terminal(&self) -> bool {
        matches!(
            self.status,
            ExecutionStatus::Succeeded | ExecutionStatus::Failed
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionClaimTransition {
    pub schema_version: u32,
    pub worker_before: WorkerState,
    pub worker_after: WorkerState,
    pub lease_before: ReservationLease,
    pub lease_after: ReservationLease,
    pub execution: ExecutionRecord,
}

pub fn claim_execution(
    worker: &WorkerState,
    lease: &ReservationLease,
    execution_id: &str,
    backend: &str,
    command: &[String],
    now_unix_ms: u64,
) -> Result<ExecutionClaimTransition> {
    worker.validate()?;
    anyhow::ensure!(!execution_id.trim().is_empty(), "execution_id is required");
    anyhow::ensure!(!backend.trim().is_empty(), "backend is required");
    anyhow::ensure!(!command.is_empty(), "execution command is required");
    anyhow::ensure!(
        lease.status == LeaseStatus::Active,
        "lease {} is not active",
        lease.lease_id
    );
    anyhow::ensure!(
        now_unix_ms < lease.expires_at_unix_ms,
        "lease {} expired before execution claim",
        lease.lease_id
    );
    anyhow::ensure!(
        lease.worker_id == worker.worker_id,
        "lease worker does not match current worker"
    );
    anyhow::ensure!(
        lease.executor_id == worker.executor_id,
        "lease executor does not match current worker"
    );
    anyhow::ensure!(
        worker.reserved_capacity.cpu_millis >= lease.reserved_capacity.cpu_millis,
        "worker reserved CPU is smaller than claimed lease"
    );
    anyhow::ensure!(
        worker.reserved_capacity.memory_bytes >= lease.reserved_capacity.memory_bytes,
        "worker reserved memory is smaller than claimed lease"
    );
    anyhow::ensure!(
        worker.reserved_allocations > 0,
        "worker has no reserved allocation to claim"
    );

    let next_revision = worker
        .state_revision
        .checked_add(1)
        .context("worker state revision overflow")?;

    let mut worker_after = worker.clone();
    worker_after.reserved_capacity = RunnerShape::new(
        worker_after.reserved_capacity.cpu_millis - lease.reserved_capacity.cpu_millis,
        worker_after.reserved_capacity.memory_bytes - lease.reserved_capacity.memory_bytes,
    );
    worker_after.reserved_allocations -= 1;
    worker_after.allocated_capacity = RunnerShape::new(
        worker_after
            .allocated_capacity
            .cpu_millis
            .checked_add(lease.reserved_capacity.cpu_millis)
            .context("allocated CPU overflow")?,
        worker_after
            .allocated_capacity
            .memory_bytes
            .checked_add(lease.reserved_capacity.memory_bytes)
            .context("allocated memory overflow")?,
    );
    worker_after.running_allocations = worker_after
        .running_allocations
        .checked_add(1)
        .context("running allocation count overflow")?;
    worker_after.state_revision = next_revision;
    worker_after.validate()?;

    let mut lease_after = lease.clone();
    lease_after.status = LeaseStatus::Claimed;
    lease_after.terminal_at_unix_ms = Some(now_unix_ms);

    let execution = ExecutionRecord {
        schema_version: EXECUTION_RECORD_SCHEMA_VERSION,
        execution_id: execution_id.into(),
        lease_id: lease.lease_id.clone(),
        worker_id: worker.worker_id.clone(),
        executor_id: worker.executor_id.clone(),
        backend: backend.into(),
        plan: lease.plan.clone(),
        command: command.to_vec(),
        status: ExecutionStatus::Starting,
        created_at_unix_ms: now_unix_ms,
        running_at_unix_ms: None,
        completed_at_unix_ms: None,
        backend_resource_id: None,
        exit_code: None,
        stdout: String::new(),
        stderr: String::new(),
        error: None,
        cleanup: CleanupStatus::NotAttempted,
    };
    execution.validate()?;

    Ok(ExecutionClaimTransition {
        schema_version: EXECUTION_TRANSITION_SCHEMA_VERSION,
        worker_before: worker.clone(),
        worker_after,
        lease_before: lease.clone(),
        lease_after,
        execution,
    })
}

pub fn mark_running(
    execution: &ExecutionRecord,
    backend_resource_id: &str,
    now_unix_ms: u64,
) -> Result<ExecutionRecord> {
    execution.validate()?;
    anyhow::ensure!(
        execution.status == ExecutionStatus::Starting,
        "execution {} is not starting",
        execution.execution_id
    );
    anyhow::ensure!(
        !backend_resource_id.trim().is_empty(),
        "backend_resource_id is required"
    );
    anyhow::ensure!(
        now_unix_ms >= execution.created_at_unix_ms,
        "running timestamp cannot precede execution creation"
    );

    let mut next = execution.clone();
    next.status = ExecutionStatus::Running;
    next.running_at_unix_ms = Some(now_unix_ms);
    next.backend_resource_id = Some(backend_resource_id.into());
    next.validate()?;
    Ok(next)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionCompletion {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub cleanup: CleanupStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionFinishTransition {
    pub schema_version: u32,
    pub worker_before: WorkerState,
    pub worker_after: WorkerState,
    pub execution_before: ExecutionRecord,
    pub execution_after: ExecutionRecord,
}

pub fn finish_execution(
    worker: &WorkerState,
    execution: &ExecutionRecord,
    completion: &ExecutionCompletion,
    now_unix_ms: u64,
) -> Result<ExecutionFinishTransition> {
    worker.validate()?;
    execution.validate()?;
    anyhow::ensure!(
        matches!(
            execution.status,
            ExecutionStatus::Starting | ExecutionStatus::Running
        ),
        "execution {} is already terminal",
        execution.execution_id
    );
    anyhow::ensure!(
        execution.worker_id == worker.worker_id,
        "execution worker does not match current worker"
    );
    anyhow::ensure!(
        worker.allocated_capacity.cpu_millis >= execution.plan.target_runner.cpu_millis,
        "worker allocated CPU is smaller than execution allocation"
    );
    anyhow::ensure!(
        worker.allocated_capacity.memory_bytes >= execution.plan.target_runner.memory_bytes,
        "worker allocated memory is smaller than execution allocation"
    );
    anyhow::ensure!(
        worker.running_allocations > 0,
        "worker has no running allocation to finish"
    );
    anyhow::ensure!(
        now_unix_ms >= execution.created_at_unix_ms,
        "completion timestamp cannot precede execution creation"
    );

    let success = completion.exit_code == Some(0)
        && completion.error.is_none()
        && completion.cleanup == CleanupStatus::Succeeded;

    let mut execution_after = execution.clone();
    execution_after.status = if success {
        ExecutionStatus::Succeeded
    } else {
        ExecutionStatus::Failed
    };
    execution_after.completed_at_unix_ms = Some(now_unix_ms);
    execution_after.exit_code = completion.exit_code;
    execution_after.stdout = completion.stdout.clone();
    execution_after.stderr = completion.stderr.clone();
    execution_after.error = completion.error.clone();
    execution_after.cleanup = completion.cleanup.clone();
    execution_after.validate()?;

    let mut worker_after = worker.clone();
    worker_after.allocated_capacity = RunnerShape::new(
        worker_after.allocated_capacity.cpu_millis - execution.plan.target_runner.cpu_millis,
        worker_after.allocated_capacity.memory_bytes - execution.plan.target_runner.memory_bytes,
    );
    worker_after.running_allocations -= 1;
    worker_after.state_revision = worker_after
        .state_revision
        .checked_add(1)
        .context("worker state revision overflow")?;
    worker_after.validate()?;

    Ok(ExecutionFinishTransition {
        schema_version: EXECUTION_TRANSITION_SCHEMA_VERSION,
        worker_before: worker.clone(),
        worker_after,
        execution_before: execution.clone(),
        execution_after,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendHandle {
    pub resource_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendCommandResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub trait ExecutorBackend {
    fn kind(&self) -> &'static str;
    fn validate_plan(&self, plan: &ExecutionPlan) -> Result<()>;
    fn create(&self, execution_id: &str, plan: &ExecutionPlan) -> Result<BackendHandle>;
    fn exec(&self, handle: &BackendHandle, command: &[String]) -> Result<BackendCommandResult>;
    fn destroy(&self, handle: &BackendHandle) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct BoxdCliBackend {
    binary: PathBuf,
    exact_shape: RunnerShape,
}

impl BoxdCliBackend {
    pub fn public_default() -> Self {
        Self {
            binary: PathBuf::from("boxd"),
            exact_shape: RunnerShape::new(2_000, 8 * GIB),
        }
    }

    pub fn with_binary(binary: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
            ..Self::public_default()
        }
    }

    pub fn binary(&self) -> &Path {
        &self.binary
    }

    fn machine_name(execution_id: &str) -> String {
        format!("cishape-{:016x}", fnv1a64(execution_id.as_bytes()))
    }

    fn run_boxd(&self, args: &[String]) -> Result<std::process::Output> {
        Command::new(&self.binary)
            .args(args)
            .output()
            .with_context(|| format!("run {}", self.binary.display()))
    }

    fn require_success(
        &self,
        action: &str,
        output: std::process::Output,
    ) -> Result<std::process::Output> {
        anyhow::ensure!(
            output.status.success(),
            "boxd {action} failed with status {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
        Ok(output)
    }
}

impl ExecutorBackend for BoxdCliBackend {
    fn kind(&self) -> &'static str {
        "boxd-cli"
    }

    fn validate_plan(&self, plan: &ExecutionPlan) -> Result<()> {
        plan.validate()?;
        anyhow::ensure!(
            plan.environment == ExecutionEnvironment::Microvm,
            "Boxd backend requires microvm execution environment"
        );
        anyhow::ensure!(
            plan.cpu_tenancy == CpuTenancy::Shared,
            "Boxd public backend does not prove exclusive CPU tenancy"
        );
        anyhow::ensure!(
            plan.host_tenancy == HostTenancy::Shared,
            "Boxd public backend does not prove dedicated host tenancy"
        );
        anyhow::ensure!(
            plan.target_runner == self.exact_shape,
            "Boxd public default proof requires exact runner shape {}; got {}",
            self.exact_shape.id(),
            plan.target_runner.id()
        );
        Ok(())
    }

    fn create(&self, execution_id: &str, plan: &ExecutionPlan) -> Result<BackendHandle> {
        self.validate_plan(plan)?;
        let resource_id = Self::machine_name(execution_id);
        let args = vec![
            "machine".into(),
            "new".into(),
            resource_id.clone(),
            "--isolated".into(),
        ];
        let output = self.run_boxd(&args)?;
        self.require_success("machine new", output)?;

        Ok(BackendHandle { resource_id })
    }

    fn exec(&self, handle: &BackendHandle, command: &[String]) -> Result<BackendCommandResult> {
        anyhow::ensure!(!command.is_empty(), "backend command is required");

        let mut args = vec!["machine".into(), "exec".into(), handle.resource_id.clone(), "--".into()];
        args.extend(command.iter().cloned());
        let output = self.run_boxd(&args)?;

        Ok(BackendCommandResult {
            exit_code: output.status.code().unwrap_or(255),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    fn destroy(&self, handle: &BackendHandle) -> Result<()> {
        let args = vec![
            "machine".into(),
            "remove".into(),
            handle.resource_id.clone(),
            "-y".into(),
        ];
        let output = self.run_boxd(&args)?;
        self.require_success("machine remove", output)?;
        Ok(())
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::{CacheRequirement, EXECUTION_PLAN_SCHEMA_VERSION};
    use crate::reservation::RESERVATION_LEASE_SCHEMA_VERSION;
    use crate::worker_state::WorkerLifecycle;

    fn plan() -> ExecutionPlan {
        ExecutionPlan {
            schema_version: EXECUTION_PLAN_SCHEMA_VERSION,
            planner_version: "execution-plan-v1".into(),
            requirements_id: "executor-proof-v1".into(),
            job: "tiny".into(),
            repository: Some("acme/api".into()),
            target_runner: RunnerShape::new(2_000, 8 * GIB),
            predicted_p95_ms: 1_000.0,
            environment: ExecutionEnvironment::Microvm,
            cpu_tenancy: CpuTenancy::Shared,
            host_tenancy: HostTenancy::Shared,
            cache: Some(CacheRequirement::Ephemeral),
            max_parallelism: Some(1),
            placement: None,
            sizing_algorithm: "fixture".into(),
        }
    }

    fn worker() -> WorkerState {
        WorkerState {
            worker_id: "worker-a".into(),
            executor_id: "boxd-default".into(),
            state_revision: 8,
            lifecycle: WorkerLifecycle::Ready,
            physical_capacity: RunnerShape::new(2_000, 8 * GIB),
            allocation_limit: RunnerShape::new(2_000, 8 * GIB),
            allocated_capacity: RunnerShape::new(0, 0),
            reserved_capacity: RunnerShape::new(2_000, 8 * GIB),
            running_allocations: 0,
            reserved_allocations: 1,
            max_allocations: 1,
            pressure: None,
        }
    }

    fn lease() -> ReservationLease {
        ReservationLease {
            schema_version: RESERVATION_LEASE_SCHEMA_VERSION,
            lease_id: "req-1".into(),
            request_id: "req-1".into(),
            worker_id: "worker-a".into(),
            executor_id: "boxd-default".into(),
            plan: plan(),
            reserved_capacity: RunnerShape::new(2_000, 8 * GIB),
            created_at_unix_ms: 1_000,
            expires_at_unix_ms: 61_000,
            requested_ttl_ms: 60_000,
            requested_worker_state_revision: 7,
            worker_state_revision_before: 7,
            worker_state_revision_after: 8,
            status: LeaseStatus::Active,
            terminal_at_unix_ms: None,
        }
    }

    #[test]
    fn claim_moves_reserved_capacity_to_running_capacity() {
        let transition = claim_execution(
            &worker(),
            &lease(),
            "exec-1",
            "boxd-cli",
            &["/bin/echo".into(), "cishape".into()],
            2_000,
        )
        .expect("claim");

        assert_eq!(transition.worker_after.state_revision, 9);
        assert_eq!(transition.worker_after.reserved_capacity, RunnerShape::new(0, 0));
        assert_eq!(
            transition.worker_after.allocated_capacity,
            RunnerShape::new(2_000, 8 * GIB)
        );
        assert_eq!(transition.worker_after.reserved_allocations, 0);
        assert_eq!(transition.worker_after.running_allocations, 1);
        assert_eq!(transition.lease_after.status, LeaseStatus::Claimed);
        assert_eq!(transition.execution.status, ExecutionStatus::Starting);
    }

    #[test]
    fn expired_lease_cannot_be_claimed() {
        let error = claim_execution(
            &worker(),
            &lease(),
            "exec-1",
            "boxd-cli",
            &["true".into()],
            61_000,
        )
        .unwrap_err();

        assert!(error.to_string().contains("expired before execution claim"));
    }

    #[test]
    fn finish_returns_running_capacity() {
        let claim = claim_execution(
            &worker(),
            &lease(),
            "exec-1",
            "boxd-cli",
            &["true".into()],
            2_000,
        )
        .expect("claim");
        let running = mark_running(&claim.execution, "machine-1", 2_100).expect("running");
        let completion = ExecutionCompletion {
            exit_code: Some(0),
            stdout: "ok\n".into(),
            stderr: String::new(),
            error: None,
            cleanup: CleanupStatus::Succeeded,
        };

        let finished =
            finish_execution(&claim.worker_after, &running, &completion, 2_200).expect("finish");

        assert_eq!(finished.execution_after.status, ExecutionStatus::Succeeded);
        assert_eq!(finished.worker_after.allocated_capacity, RunnerShape::new(0, 0));
        assert_eq!(finished.worker_after.running_allocations, 0);
        assert_eq!(finished.worker_after.state_revision, 10);
    }

    #[test]
    fn cleanup_failure_makes_execution_failed() {
        let claim = claim_execution(
            &worker(),
            &lease(),
            "exec-1",
            "boxd-cli",
            &["true".into()],
            2_000,
        )
        .expect("claim");
        let running = mark_running(&claim.execution, "machine-1", 2_100).expect("running");
        let completion = ExecutionCompletion {
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            error: Some("teardown failed".into()),
            cleanup: CleanupStatus::Failed,
        };

        let finished =
            finish_execution(&claim.worker_after, &running, &completion, 2_200).expect("finish");

        assert_eq!(finished.execution_after.status, ExecutionStatus::Failed);
    }

    #[test]
    fn boxd_public_backend_requires_exact_default_shape() {
        let backend = BoxdCliBackend::public_default();
        backend.validate_plan(&plan()).expect("supported");

        let mut too_large = plan();
        too_large.target_runner = RunnerShape::new(4_000, 8 * GIB);
        let error = backend.validate_plan(&too_large).unwrap_err();

        assert!(error.to_string().contains("exact runner shape"));
    }

    #[test]
    fn boxd_machine_name_is_deterministic_and_opaque() {
        assert_eq!(
            BoxdCliBackend::machine_name("exec-1"),
            BoxdCliBackend::machine_name("exec-1")
        );
        assert_ne!(
            BoxdCliBackend::machine_name("exec-1"),
            BoxdCliBackend::machine_name("exec-2")
        );
        assert!(BoxdCliBackend::machine_name("secret/repo").starts_with("cishape-"));
        assert!(!BoxdCliBackend::machine_name("secret/repo").contains("secret"));
    }

    #[cfg(unix)]
    #[test]
    fn boxd_cli_backend_executes_create_exec_remove_without_shell_interpolation() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        use tempfile::tempdir;

        let dir = tempdir().expect("tempdir");
        let binary = dir.path().join("boxd");
        let log = dir.path().join("calls.log");
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nif [ \"$1 $2\" = \"machine exec\" ]; then\n  printf 'cishape-from-guest\\n'\n  exit 0\nfi\nexit 0\n",
            log.display()
        );
        fs::write(&binary, script).expect("script");
        let mut permissions = fs::metadata(&binary).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&binary, permissions).expect("chmod");

        let backend = BoxdCliBackend::with_binary(&binary);
        let handle = backend.create("exec-1", &plan()).expect("create");
        let result = backend
            .exec(
                &handle,
                &["/bin/echo".into(), "hello;not-a-shell".into()],
            )
            .expect("exec");
        backend.destroy(&handle).expect("destroy");

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "cishape-from-guest\n");

        let calls = fs::read_to_string(log).expect("calls");
        assert!(calls.contains("machine new cishape-"));
        assert!(calls.contains("--isolated"));
        assert!(calls.contains("machine exec cishape-"));
        assert!(calls.contains("/bin/echo hello;not-a-shell"));
        assert!(calls.contains("machine remove cishape-"));
    }
}
