use serde::{Deserialize, Serialize};
use std::fmt;

pub const MIB: u64 = 1024 * 1024;
pub const GIB: u64 = 1024 * MIB;

pub const RUN_OBSERVATION_SCHEMA_VERSION: u32 = 2;
pub const MIN_SUPPORTED_RUN_OBSERVATION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CiIdentity {
    pub provider: Option<String>,
    pub repository: Option<String>,
    pub workflow: Option<String>,
    pub run_id: Option<String>,
    pub run_attempt: Option<u64>,
    pub workflow_job: Option<String>,
    pub commit_sha: Option<String>,
    pub git_ref: Option<String>,
}

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
    #[serde(default)]
    pub ci: CiIdentity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_runner: Option<String>,
    pub queue_ms: Option<u64>,
    pub cost_usd: Option<f64>,
    pub exit_code: i32,
}

impl RunObservation {
    pub fn canonicalize(mut self) -> Self {
        if self.ci.provider.is_none() {
            self.ci.provider = self.provider.take();
        }
        if self.runner_name.is_none() {
            self.runner_name = self.provider_runner.take();
        }
        self.provider = None;
        self.provider_runner = None;
        if self.schema_version < RUN_OBSERVATION_SCHEMA_VERSION {
            self.schema_version = RUN_OBSERVATION_SCHEMA_VERSION;
        }
        self
    }

    pub fn observation_id(&self) -> String {
        let provider = self
            .ci
            .provider
            .as_deref()
            .or(self.provider.as_deref())
            .unwrap_or("local");
        let repository = self.ci.repository.as_deref().unwrap_or("-");
        let workflow = self.ci.workflow.as_deref().unwrap_or("-");
        let run_id = self.ci.run_id.as_deref().unwrap_or("-");
        let run_attempt = self
            .ci
            .run_attempt
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".into());
        let workflow_job = self.ci.workflow_job.as_deref().unwrap_or("-");
        format!(
            "{provider}:{repository}:{workflow}:{run_id}:{run_attempt}:{workflow_job}:{}:{}",
            self.job, self.observed_at_unix_ms
        )
    }

    pub fn validate_schema(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.schema_version >= MIN_SUPPORTED_RUN_OBSERVATION_SCHEMA_VERSION,
            "unsupported RunObservation schema version {}",
            self.schema_version
        );
        anyhow::ensure!(
            self.schema_version <= RUN_OBSERVATION_SCHEMA_VERSION,
            "RunObservation schema version {} is newer than supported version {}",
            self.schema_version,
            RUN_OBSERVATION_SCHEMA_VERSION
        );
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkloadScope {
    pub repository: Option<String>,
    pub job: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobShape {
    pub job: String,
    pub repository: Option<String>,
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
    pub repository: Option<String>,
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

    #[test]
    fn legacy_provider_fields_canonicalize_to_v2() {
        let observation = RunObservation {
            schema_version: 1,
            job: "test".into(),
            observed_at_unix_ms: 123,
            duration_ms: 10,
            cpu_seconds: 0.1,
            cpu_peak_millis: 100,
            memory_peak_bytes: MIB,
            read_bytes: 0,
            write_bytes: 0,
            runner: RunnerShape::new(2_000, 4 * GIB),
            ci: CiIdentity::default(),
            runner_name: None,
            provider: Some("github-actions".into()),
            provider_runner: Some("runner-1".into()),
            queue_ms: None,
            cost_usd: None,
            exit_code: 0,
        }
        .canonicalize();

        assert_eq!(observation.schema_version, RUN_OBSERVATION_SCHEMA_VERSION);
        assert_eq!(observation.ci.provider.as_deref(), Some("github-actions"));
        assert_eq!(observation.runner_name.as_deref(), Some("runner-1"));
        assert!(observation.provider.is_none());
        assert!(observation.provider_runner.is_none());
    }

    #[test]
    fn observation_id_is_stable_for_ci_identity() {
        let observation = RunObservation {
            schema_version: RUN_OBSERVATION_SCHEMA_VERSION,
            job: "test".into(),
            observed_at_unix_ms: 123,
            duration_ms: 10,
            cpu_seconds: 0.1,
            cpu_peak_millis: 100,
            memory_peak_bytes: MIB,
            read_bytes: 0,
            write_bytes: 0,
            runner: RunnerShape::new(2_000, 4 * GIB),
            ci: CiIdentity {
                provider: Some("github-actions".into()),
                repository: Some("acme/api".into()),
                workflow: Some("CI".into()),
                run_id: Some("42".into()),
                run_attempt: Some(1),
                workflow_job: Some("check".into()),
                commit_sha: Some("abc".into()),
                git_ref: Some("refs/heads/main".into()),
            },
            runner_name: Some("runner".into()),
            provider: None,
            provider_runner: None,
            queue_ms: None,
            cost_usd: None,
            exit_code: 0,
        };
        assert_eq!(
            observation.observation_id(),
            "github-actions:acme/api:CI:42:1:check:test:123"
        );
    }
}
