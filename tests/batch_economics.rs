use cishape::batch::evaluate as evaluate_batch;
use cishape::catalog::ProviderCatalog;
use cishape::economics::{CapacitySnapshot, evaluate};
use cishape::model::{GIB, RunnerShape};
use std::path::PathBuf;

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
}

#[test]
fn thirty_jobs_expose_provider_parallelism_in_time_to_green() {
    let catalog =
        ProviderCatalog::load(&repo_path("catalogs/providers-v1.json")).expect("provider catalog");
    let snapshot = CapacitySnapshot::load(&repo_path("examples/capacity-snapshot-v1.json"))
        .expect("capacity snapshot");
    let economics = evaluate(
        &catalog,
        &snapshot,
        &RunnerShape::new(2_000, 4 * GIB),
        11_000,
    )
    .expect("economics");

    let batch = evaluate_batch(&economics, &snapshot, 30).expect("batch economics");

    let depot = batch
        .evaluations
        .iter()
        .find(|item| item.provider == "depot")
        .expect("depot");
    let github = batch
        .evaluations
        .iter()
        .find(|item| item.provider == "github-actions")
        .expect("github");
    let hetzner = batch
        .evaluations
        .iter()
        .find(|item| item.provider == "hetzner")
        .expect("hetzner");

    assert_eq!(depot.first_job_start_ms, 0);
    assert_eq!(depot.last_job_start_ms, 11_000);
    assert_eq!(depot.time_to_green_ms, 22_000);
    assert!((depot.effective_cost_usd - 0.033).abs() < 1e-12);

    assert_eq!(github.first_job_start_ms, 12_000);
    assert_eq!(github.time_to_green_ms, 42_000);
    assert!((github.effective_cost_usd - 0.18).abs() < 1e-12);

    assert!(hetzner.time_to_green_ms > depot.time_to_green_ms);
    assert!(hetzner.effective_cost_usd < depot.effective_cost_usd);
}
