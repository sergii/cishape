#![cfg(target_os = "linux")]

use std::process::Command;
use tempfile::tempdir;

#[test]
fn observe_records_a_successful_command() {
    let observation = cishape::observe::command(
        "smoke",
        "sh",
        &["-c".into(), "sleep 0.2".into()],
    )
    .expect("observation");

    assert_eq!(observation.exit_code, 0);
    assert!(observation.duration_ms >= 100);
    assert!(observation.runner.cpu_millis > 0);
    assert!(observation.runner.memory_bytes > 0);
    assert!(observation.memory_peak_bytes > 0);

    let json = serde_json::to_string(&observation).expect("serialize");
    assert!(json.contains("\"schema_version\":1"));
}

#[test]
fn cli_preserves_child_failure_status_and_writes_evidence() {
    let dir = tempdir().expect("tempdir");
    let db = dir.path().join("history.duckdb");
    let output = dir.path().join("failed.json");

    let status = Command::new(env!("CARGO_BIN_EXE_cishape"))
        .args([
            "observe",
            "--job",
            "failure",
            "--db",
            db.to_str().expect("db path"),
            "--output",
            output.to_str().expect("output path"),
            "--",
            "sh",
            "-c",
            "exit 7",
        ])
        .status()
        .expect("run cishape");

    assert_eq!(status.code(), Some(7));
    assert!(output.exists());

    let evidence = std::fs::read_to_string(output).expect("evidence");
    let observation: cishape::model::RunObservation =
        serde_json::from_str(&evidence).expect("parse evidence");
    assert_eq!(observation.exit_code, 7);
}
