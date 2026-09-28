use serde::{Deserialize, Serialize};
use std::fmt;

pub const MIB: u64 = 1024 * 1024;
pub const GIB: u64 = 1024 * MIB;

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
        format!("C{cores}-M{memory_gib}")
    }
}

impl fmt::Display for RunnerShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunObservation {
    pub job: String,
    pub duration_ms: u64,
    pub cpu_seconds: f64,
    pub cpu_peak_millis: u32,
    pub memory_peak_bytes: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub runner: RunnerShape,
    pub provider: String,
    pub provider_runner: String,
    pub queue_ms: u64,
    pub cost_usd: f64,
    pub exit_code: i32,
    pub sequence: u64,
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
