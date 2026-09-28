use cishape::model::{GIB, JobShape, MIB, RunnerShape};
use cishape::optimize::{default_catalog, recommend_with_policy};
use cishape::policy::{OptimizationObjective, OptimizationPolicy};
use std::path::PathBuf;

fn default_policy_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("policies/default-v1.json")
}

fn profile() -> JobShape {
    JobShape {
        job: "lint".into(),
        repository: Some("acme/api".into()),
        runs: 20,
        duration_p50_ms: 9_000.0,
        duration_p95_ms: 11_000.0,
        cpu_peak_p95_millis: 850.0,
        memory_peak_p99_bytes: 800.0 * MIB as f64,
        current_runner: RunnerShape::new(16_000, 64 * GIB),
    }
}

#[test]
fn checked_in_default_policy_validates() {
    let policy = OptimizationPolicy::load(&default_policy_path()).expect("policy");

    assert_eq!(policy.policy_id, "default-v1");
    assert_eq!(policy.min_runs, 10);
    assert_eq!(policy.objective, OptimizationObjective::MinimizeCost);
}

#[test]
fn policy_identity_is_embedded_in_recommendation_algorithm() {
    let policy = OptimizationPolicy::load(&default_policy_path()).expect("policy");
    let recommendation =
        recommend_with_policy(&profile(), &default_catalog(), &policy).expect("recommendation");

    assert!(recommendation.algorithm.contains("default-v1"));
    assert!(recommendation.algorithm.contains("policy-schema-v1"));
}
