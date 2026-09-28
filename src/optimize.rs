use crate::model::{GIB, JobShape, Recommendation, RunnerCandidate, RunnerShape};
use crate::policy::{OptimizationObjective, OptimizationPolicy};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeasibleCandidate {
    pub name: String,
    pub shape: RunnerShape,
    pub usd_per_minute: f64,
    pub cpu_headroom: f64,
    pub memory_headroom: f64,
    pub estimated_p95_cost_usd: f64,
}

pub fn default_catalog() -> Vec<RunnerCandidate> {
    vec![
        RunnerCandidate {
            name: "cpu1-mem2".into(),
            shape: RunnerShape::new(1_000, 2 * GIB),
            usd_per_minute: 0.001,
        },
        RunnerCandidate {
            name: "cpu2-mem4".into(),
            shape: RunnerShape::new(2_000, 4 * GIB),
            usd_per_minute: 0.002,
        },
        RunnerCandidate {
            name: "cpu4-mem8".into(),
            shape: RunnerShape::new(4_000, 8 * GIB),
            usd_per_minute: 0.004,
        },
        RunnerCandidate {
            name: "cpu8-mem16".into(),
            shape: RunnerShape::new(8_000, 16 * GIB),
            usd_per_minute: 0.008,
        },
        RunnerCandidate {
            name: "cpu16-mem64".into(),
            shape: RunnerShape::new(16_000, 64 * GIB),
            usd_per_minute: 0.016,
        },
    ]
}

pub fn feasible_candidates(
    profile: &JobShape,
    catalog: &[RunnerCandidate],
) -> Vec<FeasibleCandidate> {
    feasible_candidates_with_policy(profile, catalog, &OptimizationPolicy::default_v1())
}

pub fn feasible_candidates_with_policy(
    profile: &JobShape,
    catalog: &[RunnerCandidate],
    policy: &OptimizationPolicy,
) -> Vec<FeasibleCandidate> {
    if policy.validate().is_err() {
        return Vec::new();
    }

    let required_cpu = profile.cpu_peak_p95_millis * policy.cpu_safety_factor;
    let required_memory = profile.memory_peak_p99_bytes * policy.memory_safety_factor;
    let predicted_p95_ms = profile.duration_p95_ms * policy.latency_penalty;

    if policy
        .max_predicted_p95_ms
        .is_some_and(|limit| predicted_p95_ms > limit as f64)
    {
        return Vec::new();
    }

    let mut candidates = catalog
        .iter()
        .filter(|candidate| {
            candidate.shape.cpu_millis as f64 >= required_cpu
                && candidate.shape.memory_bytes as f64 >= required_memory
        })
        .map(|candidate| FeasibleCandidate {
            name: candidate.name.clone(),
            shape: candidate.shape.clone(),
            usd_per_minute: candidate.usd_per_minute,
            cpu_headroom: candidate.shape.cpu_millis as f64 / profile.cpu_peak_p95_millis,
            memory_headroom: candidate.shape.memory_bytes as f64 / profile.memory_peak_p99_bytes,
            estimated_p95_cost_usd: candidate.usd_per_minute * predicted_p95_ms / 60_000.0,
        })
        .collect::<Vec<_>>();

    match policy.objective {
        OptimizationObjective::MinimizeCost => {
            candidates.sort_by(|left, right| {
                left.estimated_p95_cost_usd
                    .partial_cmp(&right.estimated_p95_cost_usd)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| left.name.cmp(&right.name))
            });
        }
    }

    candidates
}

pub fn recommend(profile: &JobShape, catalog: &[RunnerCandidate]) -> Option<Recommendation> {
    recommend_with_policy(profile, catalog, &OptimizationPolicy::default_v1())
}

pub fn recommend_with_policy(
    profile: &JobShape,
    catalog: &[RunnerCandidate],
    policy: &OptimizationPolicy,
) -> Option<Recommendation> {
    let candidate = feasible_candidates_with_policy(profile, catalog, policy)
        .into_iter()
        .next()?;

    let predicted_p95_ms = profile.duration_p95_ms * policy.latency_penalty;
    let current_rate = catalog
        .iter()
        .find(|entry| entry.shape == profile.current_runner)
        .map(|entry| entry.usd_per_minute)
        .unwrap_or(candidate.usd_per_minute);

    let current_cost = current_rate * profile.duration_p95_ms / 60_000.0;
    let recommended_cost = candidate.estimated_p95_cost_usd;
    let reduction = if current_cost > 0.0 {
        (1.0 - recommended_cost / current_cost) * 100.0
    } else {
        0.0
    };

    Some(Recommendation {
        job: profile.job.clone(),
        repository: profile.repository.clone(),
        current: profile.current_runner.clone(),
        cpu_headroom: candidate.cpu_headroom,
        memory_headroom: candidate.memory_headroom,
        recommended: RunnerCandidate {
            name: candidate.name,
            shape: candidate.shape,
            usd_per_minute: candidate.usd_per_minute,
        },
        predicted_p95_ms,
        current_estimated_cost_usd: current_cost,
        recommended_estimated_cost_usd: recommended_cost,
        cost_reduction_percent: reduction,
        algorithm: policy.algorithm_id(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{JobShape, MIB};

    fn light_profile() -> JobShape {
        JobShape {
            job: "lint".into(),
            repository: None,
            runs: 100,
            duration_p50_ms: 9_000.0,
            duration_p95_ms: 11_000.0,
            cpu_peak_p95_millis: 850.0,
            memory_peak_p99_bytes: 800.0 * MIB as f64,
            current_runner: RunnerShape::new(16_000, 64 * GIB),
        }
    }

    #[test]
    fn recommends_cpu2_mem4_for_default_policy() {
        let recommendation =
            recommend(&light_profile(), &default_catalog()).expect("recommendation");
        assert_eq!(
            recommendation.recommended.shape,
            RunnerShape::new(2_000, 4 * GIB)
        );
        assert!(recommendation.algorithm.contains("default-v1"));
    }

    #[test]
    fn safety_policy_changes_feasible_set() {
        let profile = light_profile();
        let catalog = default_catalog();

        let mut relaxed = OptimizationPolicy::default_v1();
        relaxed.cpu_safety_factor = 1.0;
        relaxed.memory_safety_factor = 1.0;

        let mut strict = OptimizationPolicy::default_v1();
        strict.cpu_safety_factor = 3.0;
        strict.memory_safety_factor = 3.0;

        let relaxed_candidates =
            feasible_candidates_with_policy(&profile, &catalog, &relaxed);
        let strict_candidates =
            feasible_candidates_with_policy(&profile, &catalog, &strict);

        assert_eq!(relaxed_candidates[0].name, "cpu1-mem2");
        assert_eq!(strict_candidates[0].name, "cpu4-mem8");
    }

    #[test]
    fn latency_guard_can_reject_all_candidates() {
        let profile = light_profile();
        let catalog = default_catalog();
        let mut policy = OptimizationPolicy::default_v1();
        policy.max_predicted_p95_ms = Some(10_000);

        assert!(
            feasible_candidates_with_policy(&profile, &catalog, &policy).is_empty()
        );
    }
}
