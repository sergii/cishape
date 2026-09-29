use cishape::capacity_scope::{CapacityScope, CapacityScopeKind};
use cishape::catalog::{ProviderCatalog, RepositoryVisibility};
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

#[test]
fn account_scoped_offer_rejects_repository_scoped_concurrency_evidence() {
    let catalog =
        ProviderCatalog::load(&repo_path("catalogs/providers-v1.json")).expect("provider catalog");
    let mut snapshot = CapacitySnapshot::load(&repo_path("examples/capacity-snapshot-v1.json"))
        .expect("capacity snapshot");
    let github = snapshot
        .states
        .iter_mut()
        .find(|state| state.provider == "github-actions")
        .expect("github state");
    github.capacity_scope = Some(CapacityScope {
        kind: CapacityScopeKind::Repository,
        key: "sergii/cishape".into(),
    });

    let report = evaluate(
        &catalog,
        &snapshot,
        &RunnerShape::new(2_000, 4 * GIB),
        11_000,
    )
    .expect("economics report");

    assert!(
        report
            .evaluations
            .iter()
            .all(|item| item.offer_id != "ubuntu-latest-private-x64")
    );
    assert!(report.skipped.iter().any(|item| {
        item.offer_id == "ubuntu-latest-private-x64"
            && item.reason.contains("requires provider_account capacity scope")
            && item.reason.contains("repository")
    }));

    let github = snapshot
        .states
        .iter_mut()
        .find(|state| state.provider == "github-actions")
        .expect("github state");
    github.capacity_scope = None;

    let unknown = evaluate(
        &catalog,
        &snapshot,
        &RunnerShape::new(2_000, 4 * GIB),
        11_000,
    )
    .expect("unknown-scope economics");
    assert!(unknown.skipped.iter().any(|item| {
        item.offer_id == "ubuntu-latest-private-x64"
            && item.reason.contains("snapshot state scope is unknown")
    }));
}

#[test]
fn contextual_offer_requires_matching_snapshot_visibility() {
    let catalog =
        ProviderCatalog::load(&repo_path("catalogs/providers-v1.json")).expect("provider catalog");
    let mut snapshot = CapacitySnapshot::load(&repo_path("examples/capacity-snapshot-v1.json"))
        .expect("capacity snapshot");
    let github = snapshot
        .states
        .iter_mut()
        .find(|state| state.provider == "github-actions")
        .expect("github state");
    github.offer_id = "ubuntu-latest-public-x64".into();

    snapshot.repository_visibility = Some(RepositoryVisibility::Public);
    let public = evaluate(
        &catalog,
        &snapshot,
        &RunnerShape::new(2_000, 4 * GIB),
        11_000,
    )
    .expect("public economics");
    let public_github = public
        .evaluations
        .iter()
        .find(|item| item.offer_id == "ubuntu-latest-public-x64")
        .expect("public GitHub offer");
    assert_eq!(public_github.offer_shape, RunnerShape::new(4_000, 16 * GIB));
    assert_eq!(public_github.effective_cost_usd, 0.0);

    snapshot.repository_visibility = Some(RepositoryVisibility::Private);
    let mismatched = evaluate(
        &catalog,
        &snapshot,
        &RunnerShape::new(2_000, 4 * GIB),
        11_000,
    )
    .expect("mismatched economics");
    assert!(mismatched.skipped.iter().any(|item| {
        item.offer_id == "ubuntu-latest-public-x64"
            && item
                .reason
                .contains("requires public repository visibility")
    }));

    snapshot.repository_visibility = None;
    let unknown = evaluate(
        &catalog,
        &snapshot,
        &RunnerShape::new(2_000, 4 * GIB),
        11_000,
    )
    .expect("unknown-context economics");
    assert!(
        unknown
            .skipped
            .iter()
            .any(|item| item.offer_id == "ubuntu-latest-public-x64"
                && item.reason.contains("visibility is unknown"))
    );
}
