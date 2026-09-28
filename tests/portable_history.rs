use cishape::model::{
    CiIdentity, GIB, RUN_OBSERVATION_SCHEMA_VERSION, RunObservation, RunnerShape,
};
use cishape::store::Store;
use std::process::Command;
use tempfile::tempdir;

fn observation() -> RunObservation {
    RunObservation {
        schema_version: RUN_OBSERVATION_SCHEMA_VERSION,
        job: "test".into(),
        observed_at_unix_ms: 1_700_000_000_000,
        duration_ms: 2_500,
        cpu_seconds: 3.9,
        cpu_peak_millis: 3_600,
        memory_peak_bytes: 2 * GIB,
        read_bytes: 1_024,
        write_bytes: 2_048,
        runner: RunnerShape::new(4_000, 16 * GIB),
        ci: CiIdentity {
            provider: Some("github-actions".into()),
            repository: Some("sergii/cishape".into()),
            workflow: Some("CI".into()),
            run_id: Some("123".into()),
            run_attempt: Some(1),
            workflow_job: Some("check".into()),
            commit_sha: Some("deadbeef".into()),
            git_ref: Some("refs/heads/main".into()),
        },
        runner_name: Some("GitHub Actions 1".into()),
        provider: None,
        provider_runner: None,
        queue_ms: None,
        cost_usd: None,
        exit_code: 0,
    }
}

#[test]
fn import_is_idempotent_and_export_is_portable_jsonl() {
    let dir = tempdir().expect("tempdir");
    let db = dir.path().join("history.duckdb");
    let input = dir.path().join("run.json");
    let output = dir.path().join("history.jsonl");

    std::fs::write(
        &input,
        serde_json::to_vec_pretty(&observation()).expect("serialize input"),
    )
    .expect("write input");

    let status = Command::new(env!("CARGO_BIN_EXE_cishape"))
        .args([
            "import",
            "--db",
            db.to_str().expect("db path"),
            input.to_str().expect("input path"),
            input.to_str().expect("input path"),
        ])
        .status()
        .expect("run import");
    assert!(status.success());

    let store = Store::open(&db).expect("open imported history");
    assert_eq!(store.profile("test").expect("profile").runs, 1);
    drop(store);

    let status = Command::new(env!("CARGO_BIN_EXE_cishape"))
        .args([
            "export",
            "--db",
            db.to_str().expect("db path"),
            "--format",
            "jsonl",
            "--output",
            output.to_str().expect("output path"),
        ])
        .status()
        .expect("run export");
    assert!(status.success());

    let exported = std::fs::read_to_string(output).expect("read export");
    assert_eq!(exported.lines().count(), 1);
    let parsed: RunObservation =
        serde_json::from_str(exported.lines().next().expect("JSONL line")).expect("parse export");
    assert_eq!(parsed.ci.repository.as_deref(), Some("sergii/cishape"));
    assert_eq!(parsed.job, "test");
}

#[test]
fn profiles_require_repository_scope_when_job_names_overlap() {
    let mut left = observation();
    left.ci.repository = Some("acme/api".into());
    left.ci.run_id = Some("1".into());
    left.observed_at_unix_ms = 1;

    let mut right = observation();
    right.ci.repository = Some("acme/web".into());
    right.ci.run_id = Some("2".into());
    right.observed_at_unix_ms = 2;

    let mut store = Store::memory().expect("memory store");
    store.insert_runs(&[left, right]).expect("insert runs");

    let error = store
        .profile("test")
        .expect_err("ambiguous repository scope");
    assert!(error.to_string().contains("multiple repositories"));

    let profile = store
        .profile_for("test", Some("acme/api"))
        .expect("scoped profile");
    assert_eq!(profile.repository.as_deref(), Some("acme/api"));
    assert_eq!(profile.runs, 1);
}

#[test]
fn report_summarizes_repository_workloads_as_markdown() {
    let dir = tempdir().expect("tempdir");
    let db = dir.path().join("history.duckdb");

    let mut first = observation();
    first.observed_at_unix_ms = 1;
    first.ci.run_id = Some("1".into());

    let mut second = observation();
    second.observed_at_unix_ms = 2;
    second.ci.run_id = Some("2".into());
    second.duration_ms = 3_000;

    let mut store = Store::open(&db).expect("open store");
    store.insert_runs(&[first, second]).expect("insert history");
    drop(store);

    let output = Command::new(env!("CARGO_BIN_EXE_cishape"))
        .args([
            "report",
            "--db",
            db.to_str().expect("db path"),
            "--repository",
            "sergii/cishape",
            "--format",
            "markdown",
        ])
        .output()
        .expect("run report");

    assert!(output.status.success());
    let report = String::from_utf8(output.stdout).expect("utf8 report");
    assert!(report.contains("| Repository | Job | Runs |"));
    assert!(report.contains("| sergii/cishape | test | 2 |"));
}
