use cishape::advisory::{AdvisoryAction, AdvisoryReport, advisory_item, to_markdown};
use cishape::optimize::{default_catalog, recommend};
use cishape::store::Store;
use cishape::synthetic;

#[test]
fn low_history_is_non_actionable() {
    let mut store = Store::memory().expect("store");
    store
        .insert_runs(&synthetic::oversized_lint("lint", 3))
        .expect("runs");

    let profile = store.profile("lint").expect("profile");
    let catalog = default_catalog();
    let recommendation = recommend(&profile, &catalog).expect("recommendation");
    let item = advisory_item(&profile, Some(&recommendation), 10);

    assert_eq!(item.action, AdvisoryAction::InsufficientEvidence);
    assert_eq!(item.runs, 3);
}

#[test]
fn markdown_contains_action_and_target() {
    let mut store = Store::memory().expect("store");
    store
        .insert_runs(&synthetic::oversized_lint("lint", 20))
        .expect("runs");

    let profile = store.profile("lint").expect("profile");
    let catalog = default_catalog();
    let recommendation = recommend(&profile, &catalog).expect("recommendation");
    let item = advisory_item(&profile, Some(&recommendation), 10);

    let markdown = to_markdown(&AdvisoryReport {
        schema_version: 1,
        min_runs: 10,
        items: vec![item],
    });

    assert!(markdown.contains("downsize"));
    assert!(markdown.contains("CPU2-MEM4"));
    assert!(markdown.contains("Advisory only"));
}
