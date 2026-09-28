use crate::model::{CiIdentity, GIB, MIB, RUN_OBSERVATION_SCHEMA_VERSION, RunObservation, RunnerShape};

const SYNTHETIC_EPOCH_MS: u64 = 1_700_000_000_000;

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
                schema_version: RUN_OBSERVATION_SCHEMA_VERSION,
                job: job.to_string(),
                observed_at_unix_ms: SYNTHETIC_EPOCH_MS + i as u64 * 60_000,
                duration_ms,
                cpu_seconds: duration_seconds * 0.72,
                cpu_peak_millis,
                memory_peak_bytes,
                read_bytes: (18 + ((i * 7) % 18) as u64) * MIB,
                write_bytes: (3 + ((i * 5) % 7) as u64) * MIB,
                runner: runner.clone(),
                ci: CiIdentity {
                    provider: Some("synthetic".into()),
                    repository: None,
                    workflow: None,
                    run_id: Some(i.to_string()),
                    run_attempt: Some(1),
                    workflow_job: Some(job.to_string()),
                    commit_sha: None,
                    git_ref: None,
                },
                runner_name: Some("huge-demo-runner".into()),
                provider: None,
                provider_runner: None,
                queue_ms: Some(800 + ((i * 97) % 1_700) as u64),
                cost_usd: Some(rate_per_minute * duration_seconds / 60.0),
                exit_code: 0,
            }
        })
        .collect()
}
