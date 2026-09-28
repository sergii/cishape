use cishape::catalog::ProviderCatalog;
use cishape::model::{GIB, RunnerShape};
use std::path::PathBuf;

fn catalog_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("catalogs/providers-v1.json")
}

#[test]
fn checked_in_provider_catalog_validates() {
    let catalog = ProviderCatalog::load(&catalog_path()).expect("catalog");
    assert_eq!(catalog.schema_version, 1);
    assert!(catalog.offers.iter().any(|offer| offer.provider == "github-actions"));
    assert!(catalog.offers.iter().any(|offer| offer.provider == "depot"));
    assert!(catalog.offers.iter().any(|offer| offer.provider == "blacksmith"));
    assert!(catalog.offers.iter().any(|offer| offer.provider == "namespace"));
    assert!(catalog.offers.iter().any(|offer| offer.provider == "hetzner"));
}

#[test]
fn target_fit_uses_only_comparable_managed_offers() {
    let catalog = ProviderCatalog::load(&catalog_path()).expect("catalog");
    let fits = catalog.fit(&RunnerShape::new(2_000, 4 * GIB), 11_000);

    assert!(fits.iter().any(|fit| fit.provider == "github-actions"));
    assert!(fits.iter().any(|fit| fit.provider == "depot"));
    assert!(!fits.iter().any(|fit| fit.provider == "blacksmith"));
    assert!(!fits.iter().any(|fit| fit.provider == "namespace"));
    assert!(!fits.iter().any(|fit| fit.provider == "hetzner"));

    let github = fits
        .iter()
        .find(|fit| fit.offer_id == "ubuntu-latest-private-x64")
        .expect("github fit");
    let depot = fits
        .iter()
        .find(|fit| fit.offer_id == "depot-ubuntu-24.04")
        .expect("depot fit");

    assert_eq!(github.billed_seconds, 60);
    assert_eq!(depot.billed_seconds, 11);
    assert!(depot.estimated_cost_usd < github.estimated_cost_usd);
}
