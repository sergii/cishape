use crate::model::{GIB, JobShape, Recommendation, RunnerCandidate, RunnerShape};
use serde::{Deserialize, Serialize};

const CPU_SAFETY_FACTOR: f64 = 1.5;
const MEMORY_SAFETY_FACTOR: f64 = 1.5;
const LATENCY_PENALTY: f64 = 1.05;

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
    let required_cpu = profile.cpu_peak_p95_millis * CPU_SAFETY_FACTOR;
    let required_memory = profile.memory_peak_p99_bytes * MEMORY_SAFETY_FACTOR;
    let predicted_p95_ms = profile.duration_p95_ms * LATENCY_PENALTY;

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

    candidates.sort_by(|left, right| {
        left.usd_per_minute
            .partial_cmp(&right.usd_per_minute)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.name.cmp(&right.name))
    });

    candidates
}

pub fn recommend(profile: &JobShape, catalog: &[RunnerCandidate]) -> Option<Recommendation> {
    let candidate = feasible_candidates(profile, catalog).into_iter().next()?;

    let predicted_p95_ms = profile.duration_p95_ms * LATENCY_PENALTY;
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
        algorithm: "deterministic-fit-v0".into(),
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
    fn recommends_cpu2_mem4_for_light_job() {
        let recommendation =
            recommend(&light_profile(), &default_catalog()).expect("recommendation");
        assert_eq!(
            recommendation.recommended.shape,
            RunnerShape::new(2_000, 4 * GIB)
        );
    }

    #[test]
    fn feasible_candidates_are_filtered_and_cost_ordered() {
        let candidates = feasible_candidates(&light_profile(), &default_catalog());

        assert_eq!(candidates[0].name, "cpu2-mem4");
        assert_eq!(candidates[1].name, "cpu4-mem8");
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.cpu_headroom >= 1.5)
        );
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.memory_headroom >= 1.5)
        );
    }
}
