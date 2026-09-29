use crate::execution::{
    CacheRequirement, CpuTenancy, ExecutionEnvironment, ExecutionPlan, HostTenancy,
};
use crate::model::RunnerShape;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

pub const EXECUTOR_CATALOG_SCHEMA_VERSION: u32 = 1;
pub const BINDING_REPORT_SCHEMA_VERSION: u32 = 2;
pub const BINDING_ALGORITHM_VERSION: &str = "executor-fit-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheMode {
    Ephemeral,
    Persistent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutorTarget {
    pub executor_id: String,
    pub environments: Vec<ExecutionEnvironment>,
    pub max_runner: RunnerShape,
    pub cpu_tenancy: Vec<CpuTenancy>,
    pub host_tenancy: Vec<HostTenancy>,
    pub architectures: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub cache_modes: Vec<CacheMode>,
}

impl ExecutorTarget {
    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            !self.executor_id.trim().is_empty(),
            "executor_id is required"
        );
        anyhow::ensure!(
            self.max_runner.cpu_millis > 0,
            "executor {} max CPU must be positive",
            self.executor_id
        );
        anyhow::ensure!(
            self.max_runner.memory_bytes > 0,
            "executor {} max memory must be positive",
            self.executor_id
        );
        anyhow::ensure!(
            !self.environments.is_empty(),
            "executor {} must support at least one environment",
            self.executor_id
        );
        anyhow::ensure!(
            !self.cpu_tenancy.is_empty(),
            "executor {} must support at least one CPU tenancy mode",
            self.executor_id
        );
        anyhow::ensure!(
            !self.host_tenancy.is_empty(),
            "executor {} must support at least one host tenancy mode",
            self.executor_id
        );
        anyhow::ensure!(
            !self.architectures.is_empty(),
            "executor {} must declare at least one architecture",
            self.executor_id
        );
        anyhow::ensure!(
            !self.cache_modes.is_empty(),
            "executor {} must declare at least one cache mode",
            self.executor_id
        );

        ensure_unique_strings(
            &self.architectures,
            &format!("executor {} architectures", self.executor_id),
        )?;
        ensure_unique_strings(
            &self.capabilities,
            &format!("executor {} capabilities", self.executor_id),
        )?;

        for architecture in &self.architectures {
            anyhow::ensure!(
                !architecture.trim().is_empty(),
                "executor {} architectures must not contain blank values",
                self.executor_id
            );
        }
        for capability in &self.capabilities {
            anyhow::ensure!(
                !capability.trim().is_empty(),
                "executor {} capabilities must not contain blank values",
                self.executor_id
            );
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutorCatalog {
    pub schema_version: u32,
    pub catalog_id: String,
    pub targets: Vec<ExecutorTarget>,
}

impl ExecutorCatalog {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("read executor catalog {}", path.display()))?;
        let catalog: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse executor catalog {}", path.display()))?;
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.schema_version == EXECUTOR_CATALOG_SCHEMA_VERSION,
            "unsupported executor catalog schema version {}",
            self.schema_version
        );
        anyhow::ensure!(!self.catalog_id.trim().is_empty(), "catalog_id is required");
        anyhow::ensure!(
            !self.targets.is_empty(),
            "executor catalog must not be empty"
        );

        let mut ids = BTreeSet::new();
        for target in &self.targets {
            target.validate()?;
            anyhow::ensure!(
                ids.insert(target.executor_id.as_str()),
                "duplicate executor_id {}",
                target.executor_id
            );
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExclusionCode {
    UnsupportedEnvironment,
    InsufficientCpu,
    InsufficientMemory,
    UnsupportedCpuTenancy,
    UnsupportedHostTenancy,
    UnsupportedArchitecture,
    MissingCapability,
    EphemeralCacheUnavailable,
    PersistentCacheUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingExclusion {
    pub code: ExclusionCode,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreferenceCode {
    ReusableCacheCapable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingPreference {
    pub code: PreferenceCode,
    pub satisfied: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutorFit {
    pub executor_id: String,
    pub compatible: bool,
    pub exclusions: Vec<BindingExclusion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preferences: Vec<BindingPreference>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BindingReport {
    pub schema_version: u32,
    pub algorithm: String,
    pub catalog_id: String,
    pub job: String,
    pub repository: Option<String>,
    pub plan: ExecutionPlan,
    pub compatible_executor_ids: Vec<String>,
    pub fits: Vec<ExecutorFit>,
}

impl BindingReport {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("read binding report {}", path.display()))?;
        let report: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse binding report {}", path.display()))?;
        report.validate()?;
        Ok(report)
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.schema_version == BINDING_REPORT_SCHEMA_VERSION,
            "unsupported binding report schema version {}",
            self.schema_version
        );
        anyhow::ensure!(
            !self.algorithm.trim().is_empty(),
            "binding algorithm is required"
        );
        anyhow::ensure!(!self.catalog_id.trim().is_empty(), "catalog_id is required");
        anyhow::ensure!(!self.job.trim().is_empty(), "binding job is required");
        self.plan.validate()?;
        anyhow::ensure!(
            self.job == self.plan.job,
            "binding job does not match embedded ExecutionPlan"
        );
        anyhow::ensure!(
            self.repository == self.plan.repository,
            "binding repository does not match embedded ExecutionPlan"
        );

        let mut fit_ids = BTreeSet::new();
        let mut compatible_ids = Vec::new();
        for fit in &self.fits {
            anyhow::ensure!(
                !fit.executor_id.trim().is_empty(),
                "binding fit executor_id is required"
            );
            anyhow::ensure!(
                fit_ids.insert(fit.executor_id.as_str()),
                "duplicate binding fit executor_id {}",
                fit.executor_id
            );
            anyhow::ensure!(
                fit.compatible == fit.exclusions.is_empty(),
                "binding fit {} compatibility does not match exclusions",
                fit.executor_id
            );
            if fit.compatible {
                compatible_ids.push(fit.executor_id.clone());
            }
        }

        compatible_ids.sort();
        let mut declared = self.compatible_executor_ids.clone();
        declared.sort();
        anyhow::ensure!(
            declared == compatible_ids,
            "compatible_executor_ids do not match compatible fits"
        );
        anyhow::ensure!(
            declared.windows(2).all(|pair| pair[0] != pair[1]),
            "compatible_executor_ids contain duplicates"
        );
        Ok(())
    }
}

pub fn fit(plan: &ExecutionPlan, catalog: &ExecutorCatalog) -> Result<BindingReport> {
    plan.validate()?;
    catalog.validate()?;

    let mut targets = catalog.targets.iter().collect::<Vec<_>>();
    targets.sort_by(|left, right| left.executor_id.cmp(&right.executor_id));

    let fits = targets
        .into_iter()
        .map(|target| fit_target(plan, target))
        .collect::<Vec<_>>();
    let compatible_executor_ids = fits
        .iter()
        .filter(|fit| fit.compatible)
        .map(|fit| fit.executor_id.clone())
        .collect();

    let report = BindingReport {
        schema_version: BINDING_REPORT_SCHEMA_VERSION,
        algorithm: BINDING_ALGORITHM_VERSION.into(),
        catalog_id: catalog.catalog_id.clone(),
        job: plan.job.clone(),
        repository: plan.repository.clone(),
        plan: plan.clone(),
        compatible_executor_ids,
        fits,
    };
    report.validate()?;
    Ok(report)
}

fn fit_target(plan: &ExecutionPlan, target: &ExecutorTarget) -> ExecutorFit {
    let mut exclusions = Vec::new();
    let mut preferences = Vec::new();

    if !target.environments.contains(&plan.environment) {
        exclusions.push(BindingExclusion {
            code: ExclusionCode::UnsupportedEnvironment,
            detail: format!("executor does not support {:?}", plan.environment),
        });
    }

    if target.max_runner.cpu_millis < plan.target_runner.cpu_millis {
        exclusions.push(BindingExclusion {
            code: ExclusionCode::InsufficientCpu,
            detail: format!(
                "requires {}m CPU but executor supports at most {}m",
                plan.target_runner.cpu_millis, target.max_runner.cpu_millis
            ),
        });
    }

    if target.max_runner.memory_bytes < plan.target_runner.memory_bytes {
        exclusions.push(BindingExclusion {
            code: ExclusionCode::InsufficientMemory,
            detail: format!(
                "requires {} bytes memory but executor supports at most {}",
                plan.target_runner.memory_bytes, target.max_runner.memory_bytes
            ),
        });
    }

    if !target.cpu_tenancy.contains(&plan.cpu_tenancy) {
        exclusions.push(BindingExclusion {
            code: ExclusionCode::UnsupportedCpuTenancy,
            detail: format!(
                "executor does not support {:?} CPU tenancy",
                plan.cpu_tenancy
            ),
        });
    }

    if !target.host_tenancy.contains(&plan.host_tenancy) {
        exclusions.push(BindingExclusion {
            code: ExclusionCode::UnsupportedHostTenancy,
            detail: format!(
                "executor does not support {:?} host tenancy",
                plan.host_tenancy
            ),
        });
    }

    if let Some(placement) = &plan.placement {
        if let Some(architecture) = &placement.architecture
            && !target.architectures.contains(architecture)
        {
            exclusions.push(BindingExclusion {
                code: ExclusionCode::UnsupportedArchitecture,
                detail: format!("executor does not support architecture {architecture}"),
            });
        }

        for capability in &placement.required_capabilities {
            if !target.capabilities.contains(capability) {
                exclusions.push(BindingExclusion {
                    code: ExclusionCode::MissingCapability,
                    detail: format!("executor is missing required capability {capability}"),
                });
            }
        }
    }

    match &plan.cache {
        Some(CacheRequirement::Ephemeral) => {
            if !target.cache_modes.contains(&CacheMode::Ephemeral) {
                exclusions.push(BindingExclusion {
                    code: ExclusionCode::EphemeralCacheUnavailable,
                    detail: "executor does not support ephemeral cache/workspace mode".into(),
                });
            }
        }
        Some(CacheRequirement::PersistentRequired) => {
            if !target.cache_modes.contains(&CacheMode::Persistent) {
                exclusions.push(BindingExclusion {
                    code: ExclusionCode::PersistentCacheUnavailable,
                    detail: "executor does not support persistent reusable cache".into(),
                });
            }
        }
        Some(CacheRequirement::WarmPreferred) => {
            let reusable = target.cache_modes.contains(&CacheMode::Persistent);
            preferences.push(BindingPreference {
                code: PreferenceCode::ReusableCacheCapable,
                satisfied: reusable,
                detail: if reusable {
                    "executor can preserve reusable cache state; live warmth is not asserted".into()
                } else {
                    "executor has no persistent cache capability; warm cache preference is unmet"
                        .into()
                },
            });
        }
        None => {}
    }

    ExecutorFit {
        executor_id: target.executor_id.clone(),
        compatible: exclusions.is_empty(),
        exclusions,
        preferences,
    }
}

fn ensure_unique_strings(values: &[String], field: &str) -> Result<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        anyhow::ensure!(seen.insert(value), "duplicate value {value} in {field}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::{EXECUTION_PLAN_SCHEMA_VERSION, PlacementRequirements};
    use crate::model::GIB;

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
            placement: Some(PlacementRequirements {
                architecture: Some("x86_64".into()),
                required_capabilities: vec!["kvm".into()],
            }),
            sizing_algorithm: "deterministic-fit-v1@default-v1:policy-schema-v1".into(),
        }
    }

    fn target(id: &str) -> ExecutorTarget {
        ExecutorTarget {
            executor_id: id.into(),
            environments: vec![ExecutionEnvironment::Microvm],
            max_runner: RunnerShape::new(16_000, 64 * GIB),
            cpu_tenancy: vec![CpuTenancy::Shared, CpuTenancy::Exclusive],
            host_tenancy: vec![HostTenancy::Shared],
            architectures: vec!["x86_64".into()],
            capabilities: vec!["kvm".into()],
            cache_modes: vec![CacheMode::Ephemeral, CacheMode::Persistent],
        }
    }

    #[test]
    fn compatible_targets_are_reported_in_stable_id_order() {
        let catalog = ExecutorCatalog {
            schema_version: EXECUTOR_CATALOG_SCHEMA_VERSION,
            catalog_id: "test-v1".into(),
            targets: vec![target("z-worker"), target("a-worker")],
        };

        let report = fit(&plan(), &catalog).expect("binding report");

        assert_eq!(
            report.compatible_executor_ids,
            vec!["a-worker".to_string(), "z-worker".to_string()]
        );
        assert!(report.fits.iter().all(|fit| fit.compatible));
    }

    #[test]
    fn insufficient_capacity_and_capability_are_explicit() {
        let mut small = target("small");
        small.max_runner = RunnerShape::new(2_000, 4 * GIB);
        small.capabilities.clear();
        let catalog = ExecutorCatalog {
            schema_version: EXECUTOR_CATALOG_SCHEMA_VERSION,
            catalog_id: "test-v1".into(),
            targets: vec![small],
        };

        let report = fit(&plan(), &catalog).expect("binding report");
        let exclusions = &report.fits[0].exclusions;

        assert!(!report.fits[0].compatible);
        assert!(
            exclusions
                .iter()
                .any(|reason| reason.code == ExclusionCode::InsufficientCpu)
        );
        assert!(
            exclusions
                .iter()
                .any(|reason| reason.code == ExclusionCode::InsufficientMemory)
        );
        assert!(
            exclusions
                .iter()
                .any(|reason| reason.code == ExclusionCode::MissingCapability)
        );
    }

    #[test]
    fn warm_cache_is_a_preference_not_a_compatibility_requirement() {
        let mut no_persistent_cache = target("ephemeral-only");
        no_persistent_cache.cache_modes = vec![CacheMode::Ephemeral];
        let catalog = ExecutorCatalog {
            schema_version: EXECUTOR_CATALOG_SCHEMA_VERSION,
            catalog_id: "test-v1".into(),
            targets: vec![no_persistent_cache],
        };

        let report = fit(&plan(), &catalog).expect("binding report");
        let fit = &report.fits[0];

        assert!(fit.compatible);
        assert_eq!(fit.preferences.len(), 1);
        assert!(!fit.preferences[0].satisfied);
    }

    #[test]
    fn persistent_cache_requirement_is_hard() {
        let mut execution_plan = plan();
        execution_plan.cache = Some(CacheRequirement::PersistentRequired);
        let mut no_persistent_cache = target("ephemeral-only");
        no_persistent_cache.cache_modes = vec![CacheMode::Ephemeral];
        let catalog = ExecutorCatalog {
            schema_version: EXECUTOR_CATALOG_SCHEMA_VERSION,
            catalog_id: "test-v1".into(),
            targets: vec![no_persistent_cache],
        };

        let report = fit(&execution_plan, &catalog).expect("binding report");

        assert!(!report.fits[0].compatible);
        assert_eq!(
            report.fits[0].exclusions[0].code,
            ExclusionCode::PersistentCacheUnavailable
        );
    }

    #[test]
    fn dedicated_host_requirement_rejects_shared_only_target() {
        let mut execution_plan = plan();
        execution_plan.host_tenancy = HostTenancy::Dedicated;
        let catalog = ExecutorCatalog {
            schema_version: EXECUTOR_CATALOG_SCHEMA_VERSION,
            catalog_id: "test-v1".into(),
            targets: vec![target("shared-host")],
        };

        let report = fit(&execution_plan, &catalog).expect("binding report");

        assert!(!report.fits[0].compatible);
        assert!(
            report.fits[0]
                .exclusions
                .iter()
                .any(|reason| reason.code == ExclusionCode::UnsupportedHostTenancy)
        );
    }
}
