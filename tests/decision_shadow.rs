use cishape::decision::{
    JevChoiceAnswer, JevSystemOneResponse, JevUsage, prepare_jev_request, record_jev_response,
};
use cishape::optimize::{default_catalog, feasible_candidates, recommend};
use cishape::store::Store;
use cishape::synthetic;
use std::collections::BTreeMap;

#[test]
fn jev_shadow_decision_is_persisted_idempotently() {
    let mut store = Store::memory().expect("store");
    let runs = synthetic::oversized_lint("lint", 20);
    store.insert_runs(&runs).expect("runs");

    let profile = store.profile("lint").expect("profile");
    let catalog = default_catalog();
    let recommendation = recommend(&profile, &catalog).expect("recommendation");
    let feasible = feasible_candidates(&profile, &catalog);
    let bundle = prepare_jev_request(&profile, &recommendation, &feasible, None).expect("prepare");

    let selected = recommendation.recommended.name.clone();
    let probabilities = feasible
        .iter()
        .map(|candidate| {
            (
                candidate.name.clone(),
                if candidate.name == selected { 1.0 } else { 0.0 },
            )
        })
        .collect();

    let response = JevSystemOneResponse {
        model: "jev-1.13.0".into(),
        answers: BTreeMap::from([(
            "runner_choice".into(),
            JevChoiceAnswer {
                answer_type: "choice".into(),
                choice: selected,
                confidence: 0.91,
                probabilities,
            },
        )]),
        usage: JevUsage {
            input_tokens: 120,
            output_tokens: 8,
        },
    };

    let record = record_jev_response(&bundle, response).expect("record");
    assert!(store.insert_decision(&record).expect("first insert"));
    assert!(!store.insert_decision(&record).expect("duplicate insert"));
}

#[test]
fn wire_request_uses_typed_choice_question() {
    let mut store = Store::memory().expect("store");
    let runs = synthetic::oversized_lint("lint", 20);
    store.insert_runs(&runs).expect("runs");

    let profile = store.profile("lint").expect("profile");
    let catalog = default_catalog();
    let recommendation = recommend(&profile, &catalog).expect("recommendation");
    let feasible = feasible_candidates(&profile, &catalog);
    let bundle =
        prepare_jev_request(&profile, &recommendation, &feasible, None).expect("prepare");

    let json = serde_json::to_value(&bundle.wire).expect("serialize");
    assert_eq!(json["model"], "jev-latest");
    assert_eq!(json["questions"]["runner_choice"]["type"], "choice");
    assert!(json["questions"]["runner_choice"]["criteria"].is_object());
}
