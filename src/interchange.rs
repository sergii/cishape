use crate::model::RunObservation;
use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

pub fn read_observations(paths: &[PathBuf]) -> Result<Vec<RunObservation>> {
    let mut observations = Vec::new();
    for path in paths {
        observations.extend(read_path(path)?);
    }
    Ok(observations)
}

pub fn write_jsonl<W: Write>(runs: &[RunObservation], mut writer: W) -> Result<()> {
    for run in runs {
        let canonical = run.clone().canonicalize();
        serde_json::to_writer(&mut writer, &canonical).context("serialize RunObservation")?;
        writer.write_all(b"\n").context("write JSONL newline")?;
    }
    Ok(())
}

fn read_path(path: &Path) -> Result<Vec<RunObservation>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("read observation file {}", path.display()))?;
    let trimmed = content.trim();
    anyhow::ensure!(!trimmed.is_empty(), "observation file {} is empty", path.display());

    if let Ok(run) = serde_json::from_str::<RunObservation>(trimmed) {
        return canonicalize_all(vec![run], path);
    }

    if let Ok(runs) = serde_json::from_str::<Vec<RunObservation>>(trimmed) {
        return canonicalize_all(runs, path);
    }

    let mut runs = Vec::new();
    for (index, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let run: RunObservation = serde_json::from_str(line).with_context(|| {
            format!("parse JSONL observation {}:{}", path.display(), index + 1)
        })?;
        runs.push(run);
    }

    anyhow::ensure!(
        !runs.is_empty(),
        "observation file {} did not contain a RunObservation",
        path.display()
    );
    canonicalize_all(runs, path)
}

fn canonicalize_all(runs: Vec<RunObservation>, path: &Path) -> Result<Vec<RunObservation>> {
    runs.into_iter()
        .map(|run| {
            run.validate_schema()
                .with_context(|| format!("validate observation from {}", path.display()))?;
            Ok(run.canonicalize())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CiIdentity, GIB, RUN_OBSERVATION_SCHEMA_VERSION, RunnerShape};

    fn run(at: u64) -> RunObservation {
        RunObservation {
            schema_version: RUN_OBSERVATION_SCHEMA_VERSION,
            job: "test".into(),
            observed_at_unix_ms: at,
            duration_ms: 100,
            cpu_seconds: 0.1,
            cpu_peak_millis: 100,
            memory_peak_bytes: GIB,
            read_bytes: 0,
            write_bytes: 0,
            runner: RunnerShape::new(2_000, 4 * GIB),
            ci: CiIdentity::default(),
            runner_name: None,
            provider: None,
            provider_runner: None,
            queue_ms: None,
            cost_usd: None,
            exit_code: 0,
        }
    }

    #[test]
    fn jsonl_round_trip() {
        let mut output = Vec::new();
        write_jsonl(&[run(1), run(2)], &mut output).expect("write JSONL");
        let lines = String::from_utf8(output).expect("utf8");
        assert_eq!(lines.lines().count(), 2);
        for line in lines.lines() {
            let parsed: RunObservation = serde_json::from_str(line).expect("parse JSONL line");
            assert_eq!(parsed.schema_version, RUN_OBSERVATION_SCHEMA_VERSION);
        }
    }
}
