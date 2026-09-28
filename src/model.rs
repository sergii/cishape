use serde::{Deserialize, Serialize};
use std::fmt;

pub const MIB: u64 = 1024 * 1024;
pub const GIB: u64 = 1024 * MIB;

pub const RUN_OBSERVATION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerShape {
    pub cpu_millis: u32,
    pub memory_bytes: u64,
}

impl RunnerShape {
    pub const fn new(cpu_millis: u32, memory_bytes: u64) -> Self {
        Self {
            cpu_millis,
            memory_bytes,
        }
    }

    pub fn cpu_cores(&self) -> f64 {
        self.cpu_millis as f64 / 1000.0
    }

    pub fn memory_gib(&self) -> f64 {
        self.memory_bytes as f64 / GIB as f64
    }

    pub fn id(&self) -> String {
        let cores = self.cpu_millis / 1000;
        let memory_gib = self.memory_bytes / GIB;
        format!("cpu{cores}-mem{memory_gib}")
    }

    pub fn display_id(&self) -> String {
        self.id().to_ascii_uppercase()
    }
}

impl fmt::Display for RunnerShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_id())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunObservation {
    pub schema_version: u32,
    pub job: String,
    pub observed_at_unix_ms: u64,
    pub duration_ms: u64,
    pub cpu_seconds: f64,
    pub cpu_peak_millis: u32,
    pub memory_peak_bytes: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub runner: RunnerShape,
    pub provider: Option<String>,
    pub provider_runner: Option<String>,
    pub queue_ms: Option<u64>,
    pub cost_usd: Option<f64>,
    pub exit_code: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobShape {
    pub job: String,
    pub runs: u64,
    pub duration_p50_ms: f64,
    pub duration_p95_ms: f64,
    pub cpu_peak_p95_millis: f64,
    pub memory_peak_p99_bytes: f64,
    pub current_runner: RunnerShape,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerCandidate {
    pub name: String,
    pub shape: RunnerShape,
    pub usd_per_minute: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub job: String,
    pub current: RunnerShape,
    pub recommended: RunnerCandidate,
    pub predicted_p95_ms: f64,
    pub current_estimated_cost_usd: f64,
    pub recommended_estimated_cost_usd: f64,
    pub cost_reduction_percent: f64,
    pub cpu_headroom: f64,
    pub memory_headroom: f64,
    pub algorithm: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runner_shape_has_stable_external_and_display_ids() {
        let shape = RunnerShape::new(8_000, 16 * GIB);

        assert_eq!(shape.id(), "cpu8-mem16");
        assert_eq!(shape.display_id(), "CPU8-MEM16");
    }
}
