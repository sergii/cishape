use cishape::capacity_scope::{CapacityScope, CapacityScopeKind};
use cishape::catalog::{ProviderCatalog, RepositoryVisibility};
use cishape::economics::CapacitySnapshot;
use cishape::economics_policy::{EconomicsPolicy, select_workflow};
use cishape::workflow::{WorkflowDemand, evaluate};
use std::path::PathBuf;

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
}

#[test]
fn fan_out_fan_in_records_the_deterministic_schedule() {
    let catalog =
        ProviderCatalog::load(&repo_path("catalogs/providers-v1.json")).expect("provider catalog");
    let snapshot =
        CapacitySnapshot::load(&repo_path("examples/workflow-capacity-snapshot-v1.json"))
            .expect("capacity snapshot");
    let workflow = WorkflowDemand::load(&repo_path("examples/workflow-demand-v1.json"))
        .expect("workflow demand");

    let report = evaluate(&catalog, &snapshot, &workflow).expect("workflow economics");
    let depot = report
        .evaluations
        .iter()
        .find(|item| item.provider == "depot")
        .expect("depot evaluation");

    assert_eq!(depot.critical_path_ms, 20_000);
    assert_eq!(depot.time_to_green_ms, 20_000);
    assert!((depot.effective_cost_usd - 0.012).abs() < 1e-12);

    let prepare = depot
        .jobs
        .iter()
        .find(|job| job.id == "prepare")
        .expect("prepare");
    let branch_a = depot
        .jobs
        .iter()
        .find(|job| job.id == "branch-a")
        .expect("branch-a");
    let integrate = depot
        .jobs
        .iter()
        .find(|job| job.id == "integrate")
        .expect("integrate");
    let e2e = depot.jobs.iter().find(|job| job.id == "e2e").expect("e2e");

    assert_eq!((prepare.start_ms, prepare.finish_ms), (0, 2_000));
    assert_eq!((branch_a.start_ms, branch_a.finish_ms), (2_000, 10_000));
    assert_eq!((integrate.start_ms, integrate.finish_ms), (10_000, 16_000));
    assert_eq!((e2e.start_ms, e2e.finish_ms), (16_000, 20_000));
}

#[test]
fn workflow_skips_offer_when_concurrency_scope_is_too_narrow() {
    let catalog =
        ProviderCatalog::load(&repo_path("catalogs/providers-v1.json")).expect("provider catalog");
    let mut snapshot =
        CapacitySnapshot::load(&repo_path("examples/workflow-capacity-snapshot-v1.json"))
            .expect("capacity snapshot");
    let workflow = WorkflowDemand::load(&repo_path("examples/workflow-demand-v1.json"))
        .expect("workflow demand");

    snapshot.repository_visibility = Some(RepositoryVisibility::Public);
    let github = snapshot
        .states
        .iter_mut()
        .find(|state| state.provider == "github-actions")
        .expect("github state");
    github.offer_id = "ubuntu-latest-public-x64".into();
    github.capacity_scope = Some(CapacityScope {
        kind: CapacityScopeKind::Repository,
        key: "sergii/cishape".into(),
    });

    let report = evaluate(&catalog, &snapshot, &workflow).expect("workflow economics");

    assert!(
        report
            .evaluations
            .iter()
            .all(|item| item.offer_id != "ubuntu-latest-public-x64")
    );
    assert!(report.skipped.iter().any(|item| {
        item.offer_id == "ubuntu-latest-public-x64"
            && item
                .reason
                .contains("requires provider_account capacity scope")
    }));
}

#[test]
fn limited_workflow_parallelism_can_flip_policy_selection() {
    let catalog =
        ProviderCatalog::load(&repo_path("catalogs/providers-v1.json")).expect("provider catalog");
    let mut snapshot =
        CapacitySnapshot::load(&repo_path("examples/workflow-capacity-snapshot-v1.json"))
            .expect("capacity snapshot");
    let workflow = WorkflowDemand::load(&repo_path("examples/workflow-demand-v1.json"))
        .expect("workflow demand");
    let policy = EconomicsPolicy::default_v1();

    let constrained = evaluate(&catalog, &snapshot, &workflow).expect("constrained workflow");
    let constrained_selection =
        select_workflow(&constrained, &policy).expect("constrained selection");
    assert_eq!(
        constrained_selection.selected.expect("selected").provider,
        "depot"
    );

    let hetzner = snapshot
        .states
        .iter_mut()
        .find(|state| state.provider == "hetzner")
        .expect("hetzner state");
    hetzner.parallel_slots = 8;

    let expanded = evaluate(&catalog, &snapshot, &workflow).expect("expanded workflow");
    let expanded_selection = select_workflow(&expanded, &policy).expect("expanded selection");
    assert_eq!(
        expanded_selection.selected.expect("selected").provider,
        "hetzner"
    );
}
