use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const OPTIMIZATION_POLICY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OptimizationObjective {
    MinimizeCost,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationPolicy {
    pub schema_version: u32,
    pub policy_id: String,
    pub min_runs: u64,
    pub cpu_safety_factor: f64,
    pub memory_safety_factor: f64,
    pub latency_penalty: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_predicted_p95_ms: Option<u64>,
    pub objective: OptimizationObjective,
}

impl OptimizationPolicy {
    pub fn default_v1() -> Self {
        Self {
            schema_version: OPTIMIZATION_POLICY_SCHEMA_VERSION,
            policy_id: "default-v1".into(),
            min_runs: 10,
            cpu_safety_factor: 1.5,
            memory_safety_factor: 1.5,
            latency_penalty: 1.05,
            max_predicted_p95_ms: None,
            objective: OptimizationObjective::MinimizeCost,
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("read optimization policy {}", path.display()))?;
        let policy: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse optimization policy {}", path.display()))?;
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.schema_version == OPTIMIZATION_POLICY_SCHEMA_VERSION,
            "unsupported optimization policy schema version {}",
            self.schema_version
        );
        anyhow::ensure!(!self.policy_id.trim().is_empty(), "policy_id is required");
        anyhow::ensure!(self.min_runs > 0, "min_runs must be positive");
        anyhow::ensure!(
            self.cpu_safety_factor >= 1.0 && self.cpu_safety_factor.is_finite(),
            "cpu_safety_factor must be finite and >= 1.0"
        );
        anyhow::ensure!(
            self.memory_safety_factor >= 1.0 && self.memory_safety_factor.is_finite(),
            "memory_safety_factor must be finite and >= 1.0"
        );
        anyhow::ensure!(
            self.latency_penalty >= 1.0 && self.latency_penalty.is_finite(),
            "latency_penalty must be finite and >= 1.0"
        );
        if let Some(max_predicted_p95_ms) = self.max_predicted_p95_ms {
            anyhow::ensure!(
                max_predicted_p95_ms > 0,
                "max_predicted_p95_ms must be positive when present"
            );
        }
        Ok(())
    }

    pub fn algorithm_id(&self) -> String {
        format!(
            "deterministic-fit-v1@{}:policy-schema-v{}",
            self.policy_id, self.schema_version
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_matches_pre_policy_behavior() {
        let policy = OptimizationPolicy::default_v1();

        assert_eq!(policy.min_runs, 10);
        assert_eq!(policy.cpu_safety_factor, 1.5);
        assert_eq!(policy.memory_safety_factor, 1.5);
        assert_eq!(policy.latency_penalty, 1.05);
        assert_eq!(policy.objective, OptimizationObjective::MinimizeCost);
    }

    #[test]
    fn invalid_safety_factor_is_rejected() {
        let mut policy = OptimizationPolicy::default_v1();
        policy.cpu_safety_factor = 0.9;

        assert!(policy.validate().is_err());
    }
}
