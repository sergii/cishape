use cishape::decision::{DecisionProvider, record_deterministic_decision};
use cishape::optimize::{default_catalog, feasible_candidates, recommend};
use cishape::store::Store;
use cishape::synthetic;

#[test]
fn deterministic_decision_persists_idempotently() {
    let mut store = Store::memory().expect("store");
    let runs = synthetic::oversized_lint("lint", 20);
    store.insert_runs(&runs).expect("runs");

    let profile = store.profile("lint").expect("profile");
    let catalog = default_catalog();
    let recommendation = recommend(&profile, &catalog).expect("recommendation");
    let feasible = feasible_candidates(&profile, &catalog);
    let record =
        record_deterministic_decision(&profile, &recommendation, &feasible).expect("decision");

    assert_eq!(record.provider, DecisionProvider::Deterministic);
    assert_eq!(record.selected_candidate, recommendation.recommended.name);
    assert!(record.agrees_with_baseline);

    assert!(store.insert_decision(&record).expect("first insert"));
    assert!(!store.insert_decision(&record).expect("duplicate insert"));
}
