use crate::model::{JobShape, Recommendation, RunnerShape};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

pub const EXECUTION_REQUIREMENTS_SCHEMA_VERSION: u32 = 1;
pub const EXECUTION_PLAN_SCHEMA_VERSION: u32 = 1;
pub const EXECUTION_PLANNER_VERSION: &str = "execution-plan-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeTrust {
    Trusted,
    Untrusted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MinimumIsolation {
    Process,
    Container,
    Kernel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionEnvironment {
    Process,
    Container,
    Microvm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CpuTenancy {
    Shared,
    Exclusive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostTenancy {
    Shared,
    Dedicated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheRequirement {
    Ephemeral,
    WarmPreferred,
    PersistentRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacementRequirements {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub architecture: Option<String>,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
}

impl PlacementRequirements {
    fn validate(&self) -> Result<()> {
        if let Some(architecture) = &self.architecture {
            anyhow::ensure!(
                !architecture.trim().is_empty(),
                "placement architecture must not be blank"
            );
        }

        let mut seen = BTreeSet::new();
        for capability in &self.required_capabilities {
            anyhow::ensure!(
                !capability.trim().is_empty(),
                "placement capabilities must not contain blank values"
            );
            anyhow::ensure!(
                seen.insert(capability),
                "duplicate placement capability {capability}"
            );
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionRequirements {
    pub schema_version: u32,
    pub requirements_id: String,
    pub code_trust: CodeTrust,
    pub minimum_isolation: MinimumIsolation,
    pub cpu_tenancy: CpuTenancy,
    pub host_tenancy: HostTenancy,
    #[serde(default)]
    pub cache: Option<CacheRequirement>,
    #[serde(default)]
    pub max_parallelism: Option<u32>,
    #[serde(default)]
    pub placement: Option<PlacementRequirements>,
}

impl ExecutionRequirements {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("read execution requirements {}", path.display()))?;
        let requirements: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse execution requirements {}", path.display()))?;
        requirements.validate()?;
        Ok(requirements)
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.schema_version == EXECUTION_REQUIREMENTS_SCHEMA_VERSION,
            "unsupported execution requirements schema version {}",
            self.schema_version
        );
        anyhow::ensure!(
            !self.requirements_id.trim().is_empty(),
            "requirements_id is required"
        );
        if let Some(max_parallelism) = self.max_parallelism {
            anyhow::ensure!(max_parallelism > 0, "max_parallelism must be positive");
        }
        if let Some(placement) = &self.placement {
            placement.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub schema_version: u32,
    pub planner_version: String,
    pub requirements_id: String,
    pub job: String,
    pub repository: Option<String>,
    pub target_runner: RunnerShape,
    pub predicted_p95_ms: f64,
    pub environment: ExecutionEnvironment,
    pub cpu_tenancy: CpuTenancy,
    pub host_tenancy: HostTenancy,
    pub cache: Option<CacheRequirement>,
    pub max_parallelism: Option<u32>,
    pub placement: Option<PlacementRequirements>,
    pub sizing_algorithm: String,
}

pub fn plan(
    profile: &JobShape,
    recommendation: &Recommendation,
    requirements: &ExecutionRequirements,
) -> Result<ExecutionPlan> {
    requirements.validate()?;

    anyhow::ensure!(
        recommendation.job == profile.job,
        "recommendation job does not match JobShape"
    );
    anyhow::ensure!(
        recommendation.repository == profile.repository,
        "recommendation repository does not match JobShape"
    );

    let environment = match (&requirements.code_trust, &requirements.minimum_isolation) {
        (CodeTrust::Untrusted, _) | (_, MinimumIsolation::Kernel) => {
            ExecutionEnvironment::Microvm
        }
        (_, MinimumIsolation::Container) => ExecutionEnvironment::Container,
        (_, MinimumIsolation::Process) => ExecutionEnvironment::Process,
    };

    Ok(ExecutionPlan {
        schema_version: EXECUTION_PLAN_SCHEMA_VERSION,
        planner_version: EXECUTION_PLANNER_VERSION.into(),
        requirements_id: requirements.requirements_id.clone(),
        job: profile.job.clone(),
        repository: profile.repository.clone(),
        target_runner: recommendation.recommended.shape.clone(),
        predicted_p95_ms: recommendation.predicted_p95_ms,
        environment,
        cpu_tenancy: requirements.cpu_tenancy.clone(),
        host_tenancy: requirements.host_tenancy.clone(),
        cache: requirements.cache.clone(),
        max_parallelism: requirements.max_parallelism,
        placement: requirements.placement.clone(),
        sizing_algorithm: recommendation.algorithm.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GIB, MIB, RunnerCandidate};

    fn profile() -> JobShape {
        JobShape {
            job: "test".into(),
            repository: Some("acme/api".into()),
            runs: 20,
            duration_p50_ms: 10_000.0,
            duration_p95_ms: 12_000.0,
            cpu_peak_p95_millis: 1_200.0,
            memory_peak_p99_bytes: 900.0 * MIB as f64,
            current_runner: RunnerShape::new(4_000, 8 * GIB),
        }
    }

    fn recommendation() -> Recommendation {
        Recommendation {
            job: "test".into(),
            repository: Some("acme/api".into()),
            current: RunnerShape::new(4_000, 8 * GIB),
            recommended: RunnerCandidate {
                name: "cpu2-mem4".into(),
                shape: RunnerShape::new(2_000, 4 * GIB),
                usd_per_minute: 0.002,
            },
            predicted_p95_ms: 12_600.0,
            current_estimated_cost_usd: Some(0.0008),
            recommended_estimated_cost_usd: 0.00042,
            cost_reduction_percent: Some(47.5),
            cpu_headroom: 1.66,
            memory_headroom: 4.55,
            algorithm: "deterministic-fit-v1@default-v1:policy-schema-v1".into(),
        }
    }

    fn requirements() -> ExecutionRequirements {
        ExecutionRequirements {
            schema_version: EXECUTION_REQUIREMENTS_SCHEMA_VERSION,
            requirements_id: "trusted-default".into(),
            code_trust: CodeTrust::Trusted,
            minimum_isolation: MinimumIsolation::Process,
            cpu_tenancy: CpuTenancy::Shared,
            host_tenancy: HostTenancy::Shared,
            cache: None,
            max_parallelism: None,
            placement: None,
        }
    }

    #[test]
    fn trusted_workload_can_use_process_environment() {
        let plan = plan(&profile(), &recommendation(), &requirements()).expect("plan");

        assert_eq!(plan.environment, ExecutionEnvironment::Process);
        assert_eq!(plan.target_runner, RunnerShape::new(2_000, 4 * GIB));
    }

    #[test]
    fn explicit_container_isolation_selects_container() {
        let mut requirements = requirements();
        requirements.minimum_isolation = MinimumIsolation::Container;

        let plan = plan(&profile(), &recommendation(), &requirements).expect("plan");

        assert_eq!(plan.environment, ExecutionEnvironment::Container);
    }

    #[test]
    fn untrusted_workload_escalates_to_kernel_isolation() {
        let mut requirements = requirements();
        requirements.code_trust = CodeTrust::Untrusted;

        let plan = plan(&profile(), &recommendation(), &requirements).expect("plan");

        assert_eq!(plan.environment, ExecutionEnvironment::Microvm);
    }

    #[test]
    fn dedicated_host_is_orthogonal_to_microvm_environment() {
        let mut requirements = requirements();
        requirements.code_trust = CodeTrust::Untrusted;
        requirements.cpu_tenancy = CpuTenancy::Exclusive;
        requirements.host_tenancy = HostTenancy::Dedicated;

        let plan = plan(&profile(), &recommendation(), &requirements).expect("plan");

        assert_eq!(plan.environment, ExecutionEnvironment::Microvm);
        assert_eq!(plan.cpu_tenancy, CpuTenancy::Exclusive);
        assert_eq!(plan.host_tenancy, HostTenancy::Dedicated);
    }

    #[test]
    fn invalid_parallelism_fails_closed() {
        let mut requirements = requirements();
        requirements.max_parallelism = Some(0);

        let error = plan(&profile(), &recommendation(), &requirements).unwrap_err();

        assert!(error.to_string().contains("max_parallelism"));
    }

    #[test]
    fn mismatched_recommendation_fails_closed() {
        let mut recommendation = recommendation();
        recommendation.job = "lint".into();

        let error = plan(&profile(), &recommendation, &requirements()).unwrap_err();

        assert!(error.to_string().contains("does not match"));
    }
}
