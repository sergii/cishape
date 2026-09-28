use crate::model::{GIB, JobShape, Recommendation, RunnerCandidate, RunnerShape};

const CPU_SAFETY_FACTOR: f64 = 1.5;
const MEMORY_SAFETY_FACTOR: f64 = 1.5;
const LATENCY_PENALTY: f64 = 1.05;

pub fn default_catalog() -> Vec<RunnerCandidate> {
    vec![
        RunnerCandidate {
            name: "c1-m2".into(),
            shape: RunnerShape::new(1_000, 2 * GIB),
            usd_per_minute: 0.001,
        },
        RunnerCandidate {
            name: "c2-m4".into(),
            shape: RunnerShape::new(2_000, 4 * GIB),
            usd_per_minute: 0.002,
        },
        RunnerCandidate {
            name: "c4-m8".into(),
            shape: RunnerShape::new(4_000, 8 * GIB),
            usd_per_minute: 0.004,
        },
        RunnerCandidate {
            name: "c8-m16".into(),
            shape: RunnerShape::new(8_000, 16 * GIB),
            usd_per_minute: 0.008,
        },
        RunnerCandidate {
            name: "c16-m64".into(),
            shape: RunnerShape::new(16_000, 64 * GIB),
            usd_per_minute: 0.016,
        },
    ]
}

pub fn recommend(profile: &JobShape, catalog: &[RunnerCandidate]) -> Option<Recommendation> {
    let required_cpu = profile.cpu_peak_p95_millis * CPU_SAFETY_FACTOR;
    let required_memory = profile.memory_peak_p99_bytes * MEMORY_SAFETY_FACTOR;

    let candidate = catalog
        .iter()
        .filter(|candidate| {
            candidate.shape.cpu_millis as f64 >= required_cpu
                && candidate.shape.memory_bytes as f64 >= required_memory
        })
        .min_by(|left, right| {
            left.usd_per_minute
                .partial_cmp(&right.usd_per_minute)
                .unwrap_or(std::cmp::Ordering::Equal)
        })?
        .clone();

    let predicted_p95_ms = profile.duration_p95_ms * LATENCY_PENALTY;
    let current_rate = catalog
        .iter()
        .find(|entry| entry.shape == profile.current_runner)
        .map(|entry| entry.usd_per_minute)
        .unwrap_or(candidate.usd_per_minute);

    let current_cost = current_rate * profile.duration_p95_ms / 60_000.0;
    let recommended_cost = candidate.usd_per_minute * predicted_p95_ms / 60_000.0;
    let reduction = if current_cost > 0.0 {
        (1.0 - recommended_cost / current_cost) * 100.0
    } else {
        0.0
    };

    Some(Recommendation {
        job: profile.job.clone(),
        current: profile.current_runner.clone(),
        cpu_headroom: candidate.shape.cpu_millis as f64 / profile.cpu_peak_p95_millis,
        memory_headroom: candidate.shape.memory_bytes as f64 / profile.memory_peak_p99_bytes,
        recommended: candidate,
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

    #[test]
    fn recommends_c2_m4_for_light_job() {
        let profile = JobShape {
            job: "lint".into(),
            runs: 100,
            duration_p50_ms: 9_000.0,
            duration_p95_ms: 11_000.0,
            cpu_peak_p95_millis: 850.0,
            memory_peak_p99_bytes: 800.0 * MIB as f64,
            current_runner: RunnerShape::new(16_000, 64 * GIB),
        };

        let recommendation = recommend(&profile, &default_catalog()).expect("recommendation");
        assert_eq!(
            recommendation.recommended.shape,
            RunnerShape::new(2_000, 4 * GIB)
        );
    }
}
