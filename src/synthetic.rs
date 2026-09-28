use crate::model::{GIB, MIB, RunObservation, RunnerShape};

pub fn oversized_lint(job: &str, runs: usize) -> Vec<RunObservation> {
    let runner = RunnerShape::new(16_000, 64 * GIB);
    let rate_per_minute = 0.016;

    (0..runs)
        .map(|i| {
            let duration_ms = 8_500 + ((i * 173) % 2_900) as u64;
            let cpu_peak_millis = 620 + ((i * 47) % 290) as u32;
            let memory_peak_bytes = (540 + ((i * 29) % 250) as u64) * MIB;
            let duration_seconds = duration_ms as f64 / 1000.0;

            RunObservation {
                job: job.to_string(),
                duration_ms,
                cpu_seconds: duration_seconds * 0.72,
                cpu_peak_millis,
                memory_peak_bytes,
                read_bytes: (18 + ((i * 7) % 18) as u64) * MIB,
                write_bytes: (3 + ((i * 5) % 7) as u64) * MIB,
                runner: runner.clone(),
                provider: "synthetic".into(),
                provider_runner: "huge-demo-runner".into(),
                queue_ms: 800 + ((i * 97) % 1_700) as u64,
                cost_usd: rate_per_minute * duration_seconds / 60.0,
                exit_code: 0,
                sequence: i as u64,
            }
        })
        .collect()
}
