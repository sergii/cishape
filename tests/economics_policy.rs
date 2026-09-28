use cishape::catalog::ProviderCatalog;
use cishape::economics::{CapacitySnapshot, evaluate};
use cishape::economics_policy::{EconomicsObjective, EconomicsPolicy, select};
use cishape::model::{GIB, RunnerShape};
use std::path::PathBuf;

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn report(snapshot: &CapacitySnapshot) -> cishape::economics::EconomicsReport {
    let catalog =
        ProviderCatalog::load(&repo_path("catalogs/providers-v1.json")).expect("provider catalog");
    evaluate(
        &catalog,
        snapshot,
        &RunnerShape::new(2_000, 4 * GIB),
        11_000,
    )
    .expect("economics report")
}

#[test]
fn checked_in_economics_policy_selects_depot_under_30_second_sla() {
    let snapshot = CapacitySnapshot::load(&repo_path("examples/capacity-snapshot-v1.json"))
        .expect("capacity snapshot");
    let policy = EconomicsPolicy::load(&repo_path("policies/economics-default-v1.json"))
        .expect("economics policy");

    let selection = select(&report(&snapshot), &policy).expect("selection");
    let selected = selection.selected.expect("selected offer");

    assert_eq!(selected.provider, "depot");
    assert_eq!(selected.offer_id, "depot-ubuntu-24.04");
    assert_eq!(selection.eligible_candidates, 2);
    assert!(
        selection
            .excluded
            .iter()
            .any(|item| item.provider == "hetzner")
    );
}

#[test]
fn queue_change_can_flip_selection_to_persistent_capacity() {
    let mut snapshot = CapacitySnapshot::load(&repo_path("examples/capacity-snapshot-v1.json"))
        .expect("capacity snapshot");
    let hetzner = snapshot
        .states
        .iter_mut()
        .find(|state| state.provider == "hetzner")
        .expect("hetzner state");
    hetzner.queue_depth = 0;
    hetzner.running_jobs = 0;

    let policy = EconomicsPolicy::load(&repo_path("policies/economics-default-v1.json"))
        .expect("economics policy");
    let selection = select(&report(&snapshot), &policy).expect("selection");

    assert_eq!(
        selection.selected.expect("selected offer").provider,
        "hetzner"
    );
}

#[test]
fn utilization_change_can_flip_cost_selection() {
    let snapshot = CapacitySnapshot::load(&repo_path("examples/capacity-snapshot-v1.json"))
        .expect("capacity snapshot");
    let policy = EconomicsPolicy {
        schema_version: 1,
        policy_id: "wide-sla".into(),
        objective: EconomicsObjective::MinimizeEffectiveCost,
        max_time_to_green_ms: Some(45_000),
        max_effective_cost_usd: None,
    };

    let selection = select(&report(&snapshot), &policy).expect("selection");
    assert_eq!(
        selection.selected.expect("selected offer").provider,
        "hetzner"
    );

    let mut low_utilization = snapshot;
    low_utilization
        .states
        .iter_mut()
        .find(|state| state.provider == "hetzner")
        .expect("hetzner state")
        .utilization = Some(0.01);

    let selection = select(&report(&low_utilization), &policy).expect("selection");
    assert_eq!(
        selection.selected.expect("selected offer").provider,
        "depot"
    );
}
