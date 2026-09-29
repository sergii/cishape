use crate::model::RunnerShape;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

pub const WORKER_STATE_SNAPSHOT_SCHEMA_VERSION: u32 = 1;
pub const WORKER_STATE_REPORT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerLifecycle {
    Ready,
    Draining,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkerPressure {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_utilization_ratio: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_utilization_ratio: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub io_pressure_ratio: Option<f64>,
}

impl WorkerPressure {
    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.cpu_utilization_ratio.is_some()
                || self.memory_utilization_ratio.is_some()
                || self.io_pressure_ratio.is_some(),
            "worker pressure must contain at least one metric"
        );

        validate_ratio("cpu_utilization_ratio", self.cpu_utilization_ratio)?;
        validate_ratio(
            "memory_utilization_ratio",
            self.memory_utilization_ratio,
        )?;
        validate_ratio("io_pressure_ratio", self.io_pressure_ratio)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkerState {
    pub worker_id: String,
    pub executor_id: String,
    pub lifecycle: WorkerLifecycle,
    pub physical_capacity: RunnerShape,
    pub allocation_limit: RunnerShape,
    pub allocated_capacity: RunnerShape,
    pub running_allocations: u32,
    pub max_allocations: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure: Option<WorkerPressure>,
}

impl WorkerState {
    fn validate(&self) -> Result<()> {
        anyhow::ensure!(!self.worker_id.trim().is_empty(), "worker_id is required");
        anyhow::ensure!(
            !self.executor_id.trim().is_empty(),
            "executor_id is required"
        );
        validate_positive_shape("physical_capacity", &self.physical_capacity)?;
        validate_positive_shape("allocation_limit", &self.allocation_limit)?;

        anyhow::ensure!(
            self.allocated_capacity.cpu_millis <= self.allocation_limit.cpu_millis,
            "worker {} allocated CPU exceeds allocation limit",
            self.worker_id
        );
        anyhow::ensure!(
            self.allocated_capacity.memory_bytes <= self.allocation_limit.memory_bytes,
            "worker {} allocated memory exceeds allocation limit",
            self.worker_id
        );
        anyhow::ensure!(
            self.max_allocations > 0,
            "worker {} max_allocations must be positive",
            self.worker_id
        );
        anyhow::ensure!(
            self.running_allocations <= self.max_allocations,
            "worker {} running_allocations cannot exceed max_allocations",
            self.worker_id
        );

        if let Some(pressure) = &self.pressure {
            pressure.validate()?;
        }

        Ok(())
    }

    pub fn available_capacity(&self) -> RunnerShape {
        RunnerShape::new(
            self.allocation_limit.cpu_millis - self.allocated_capacity.cpu_millis,
            self.allocation_limit.memory_bytes - self.allocated_capacity.memory_bytes,
        )
    }

    pub fn accepting_new_work(&self) -> bool {
        let available = self.available_capacity();
        self.lifecycle == WorkerLifecycle::Ready
            && self.running_allocations < self.max_allocations
            && available.cpu_millis > 0
            && available.memory_bytes > 0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkerStateSnapshot {
    pub schema_version: u32,
    pub observed_at: String,
    pub source: String,
    pub workers: Vec<WorkerState>,
}

impl WorkerStateSnapshot {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("read worker-state snapshot {}", path.display()))?;
        let snapshot: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse worker-state snapshot {}", path.display()))?;
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.schema_version == WORKER_STATE_SNAPSHOT_SCHEMA_VERSION,
            "unsupported worker-state snapshot schema version {}",
            self.schema_version
        );
        anyhow::ensure!(
            !self.observed_at.trim().is_empty(),
            "worker-state snapshot observed_at is required"
        );
        anyhow::ensure!(
            !self.source.trim().is_empty(),
            "worker-state snapshot source is required"
        );

        let mut worker_ids = BTreeSet::new();
        for worker in &self.workers {
            worker.validate()?;
            anyhow::ensure!(
                worker_ids.insert(worker.worker_id.as_str()),
                "duplicate worker_id {}",
                worker.worker_id
            );
        }

        Ok(())
    }

    pub fn report(&self) -> WorkerStateReport {
        let mut workers = self
            .workers
            .iter()
            .map(|worker| WorkerStateSummary {
                worker_id: worker.worker_id.clone(),
                executor_id: worker.executor_id.clone(),
                lifecycle: worker.lifecycle.clone(),
                physical_capacity: worker.physical_capacity.clone(),
                allocation_limit: worker.allocation_limit.clone(),
                allocated_capacity: worker.allocated_capacity.clone(),
                available_capacity: worker.available_capacity(),
                running_allocations: worker.running_allocations,
                max_allocations: worker.max_allocations,
                accepting_new_work: worker.accepting_new_work(),
                pressure: worker.pressure.clone(),
            })
            .collect::<Vec<_>>();

        workers.sort_by(|left, right| left.worker_id.cmp(&right.worker_id));

        WorkerStateReport {
            schema_version: WORKER_STATE_REPORT_SCHEMA_VERSION,
            snapshot_observed_at: self.observed_at.clone(),
            snapshot_source: self.source.clone(),
            workers,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkerStateSummary {
    pub worker_id: String,
    pub executor_id: String,
    pub lifecycle: WorkerLifecycle,
    pub physical_capacity: RunnerShape,
    pub allocation_limit: RunnerShape,
    pub allocated_capacity: RunnerShape,
    pub available_capacity: RunnerShape,
    pub running_allocations: u32,
    pub max_allocations: u32,
    pub accepting_new_work: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure: Option<WorkerPressure>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkerStateReport {
    pub schema_version: u32,
    pub snapshot_observed_at: String,
    pub snapshot_source: String,
    pub workers: Vec<WorkerStateSummary>,
}

fn validate_positive_shape(field: &str, shape: &RunnerShape) -> Result<()> {
    anyhow::ensure!(
        shape.cpu_millis > 0,
        "{field} CPU must be positive"
    );
    anyhow::ensure!(
        shape.memory_bytes > 0,
        "{field} memory must be positive"
    );
    Ok(())
}

fn validate_ratio(field: &str, value: Option<f64>) -> Result<()> {
    if let Some(value) = value {
        anyhow::ensure!(
            value.is_finite() && (0.0..=1.0).contains(&value),
            "{field} must be finite and in [0, 1]"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::GIB;

    fn ready_worker(id: &str) -> WorkerState {
        WorkerState {
            worker_id: id.into(),
            executor_id: "microvm-x86_64".into(),
            lifecycle: WorkerLifecycle::Ready,
            physical_capacity: RunnerShape::new(32_000, 128 * GIB),
            allocation_limit: RunnerShape::new(48_000, 120 * GIB),
            allocated_capacity: RunnerShape::new(20_000, 40 * GIB),
            running_allocations: 6,
            max_allocations: 16,
            pressure: Some(WorkerPressure {
                cpu_utilization_ratio: Some(0.42),
                memory_utilization_ratio: Some(0.31),
                io_pressure_ratio: Some(0.08),
            }),
        }
    }

    #[test]
    fn derives_available_capacity_from_explicit_allocation_limit() {
        let worker = ready_worker("worker-a");

        assert_eq!(
            worker.available_capacity(),
            RunnerShape::new(28_000, 80 * GIB)
        );
        assert!(worker.accepting_new_work());
    }

    #[test]
    fn explicit_cpu_oversubscription_limit_is_allowed() {
        let worker = ready_worker("worker-a");

        assert!(worker.allocation_limit.cpu_millis > worker.physical_capacity.cpu_millis);
        assert!(worker.validate().is_ok());
    }

    #[test]
    fn draining_worker_does_not_accept_new_work() {
        let mut worker = ready_worker("worker-a");
        worker.lifecycle = WorkerLifecycle::Draining;

        assert!(!worker.accepting_new_work());
    }

    #[test]
    fn full_allocation_count_does_not_accept_new_work() {
        let mut worker = ready_worker("worker-a");
        worker.running_allocations = worker.max_allocations;

        assert!(!worker.accepting_new_work());
    }

    #[test]
    fn allocated_capacity_cannot_exceed_explicit_limit() {
        let mut worker = ready_worker("worker-a");
        worker.allocated_capacity.cpu_millis = worker.allocation_limit.cpu_millis + 1;

        let error = worker.validate().unwrap_err();

        assert!(error.to_string().contains("allocated CPU exceeds"));
    }

    #[test]
    fn report_is_stably_sorted_by_worker_id() {
        let snapshot = WorkerStateSnapshot {
            schema_version: WORKER_STATE_SNAPSHOT_SCHEMA_VERSION,
            observed_at: "2026-09-29T15:00:00Z".into(),
            source: "fixture".into(),
            workers: vec![ready_worker("worker-z"), ready_worker("worker-a")],
        };

        snapshot.validate().expect("snapshot");
        let report = snapshot.report();

        assert_eq!(report.workers[0].worker_id, "worker-a");
        assert_eq!(report.workers[1].worker_id, "worker-z");
    }

    #[test]
    fn invalid_pressure_ratio_fails_closed() {
        let mut worker = ready_worker("worker-a");
        worker.pressure = Some(WorkerPressure {
            cpu_utilization_ratio: Some(1.1),
            memory_utilization_ratio: None,
            io_pressure_ratio: None,
        });

        let error = worker.validate().unwrap_err();

        assert!(error.to_string().contains("cpu_utilization_ratio"));
    }
}
