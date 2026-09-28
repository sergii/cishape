use cishape::catalog::ProviderCatalog;
use cishape::economics::{CapacitySnapshot, CostBasis, evaluate};
use cishape::model::{GIB, RunnerShape};
use std::path::PathBuf;

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
}

#[test]
fn deterministic_fixture_compares_managed_and_persistent_capacity() {
    let catalog =
        ProviderCatalog::load(&repo_path("catalogs/providers-v1.json")).expect("provider catalog");
    let snapshot = CapacitySnapshot::load(&repo_path("examples/capacity-snapshot-v1.json"))
        .expect("capacity snapshot");

    let report = evaluate(
        &catalog,
        &snapshot,
        &RunnerShape::new(2_000, 4 * GIB),
        11_000,
    )
    .expect("economics report");

    assert_eq!(report.evaluations.len(), 3);
    assert!(report.skipped.is_empty());

    let github = report
        .evaluations
        .iter()
        .find(|item| item.provider == "github-actions")
        .expect("github");
    let depot = report
        .evaluations
        .iter()
        .find(|item| item.provider == "depot")
        .expect("depot");
    let hetzner = report
        .evaluations
        .iter()
        .find(|item| item.provider == "hetzner")
        .expect("hetzner");

    assert_eq!(github.effective_runtime_ms, 15_000);
    assert_eq!(github.estimated_queue_ms, 12_000);
    assert_eq!(github.time_to_green_ms, 27_000);
    assert_eq!(github.billed_seconds, Some(60));
    assert!((github.effective_cost_usd - 0.006).abs() < 1e-12);
    assert!(!github.pareto_optimal);

    assert_eq!(depot.time_to_green_ms, 11_000);
    assert_eq!(depot.billed_seconds, Some(11));
    assert!((depot.effective_cost_usd - 0.0011).abs() < 1e-12);
    assert!(depot.pareto_optimal);

    assert_eq!(hetzner.estimated_queue_ms, 22_000);
    assert_eq!(hetzner.time_to_green_ms, 33_000);
    assert_eq!(hetzner.cost_basis, CostBasis::AllocatedFixedCapacity);
    assert!(hetzner.effective_cost_usd < depot.effective_cost_usd);
    assert!(hetzner.pareto_optimal);
}
