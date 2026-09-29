use crate::model::{GIB, RunnerShape};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::path::Path;

pub const PROVIDER_CATALOG_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCatalog {
    pub schema_version: u32,
    pub observed_at: String,
    pub offers: Vec<RunnerOffer>,
}

impl ProviderCatalog {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("read provider catalog {}", path.display()))?;
        let catalog: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse provider catalog {}", path.display()))?;
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.schema_version == PROVIDER_CATALOG_SCHEMA_VERSION,
            "unsupported provider catalog schema version {}",
            self.schema_version
        );
        anyhow::ensure!(
            !self.observed_at.trim().is_empty(),
            "catalog observed_at is required"
        );

        let mut identities = std::collections::BTreeSet::new();
        for offer in &self.offers {
            offer.validate()?;
            let identity = format!("{}:{}", offer.provider, offer.offer_id);
            anyhow::ensure!(
                identities.insert(identity.clone()),
                "duplicate offer {identity}"
            );
        }

        Ok(())
    }

    pub fn fit(&self, target: &RunnerShape, predicted_duration_ms: u64) -> Vec<OfferFit> {
        self.fit_with_visibility(target, predicted_duration_ms, None)
    }

    pub fn fit_with_visibility(
        &self,
        target: &RunnerShape,
        predicted_duration_ms: u64,
        repository_visibility: Option<RepositoryVisibility>,
    ) -> Vec<OfferFit> {
        let mut matches = self
            .offers
            .iter()
            .filter_map(|offer| {
                offer.fit(
                    target,
                    predicted_duration_ms,
                    repository_visibility.as_ref(),
                )
            })
            .collect::<Vec<_>>();

        matches.sort_by(|left, right| {
            left.estimated_cost_usd
                .partial_cmp(&right.estimated_cost_usd)
                .unwrap_or(Ordering::Equal)
                .then_with(|| left.provider.cmp(&right.provider))
                .then_with(|| left.offer_id.cmp(&right.offer_id))
        });

        matches
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerCapacity {
    pub cpu_millis: Option<u32>,
    pub memory_bytes: Option<u64>,
}

impl RunnerCapacity {
    pub fn complete_shape(&self) -> Option<RunnerShape> {
        Some(RunnerShape::new(self.cpu_millis?, self.memory_bytes?))
    }

    pub fn display(&self) -> String {
        match self.complete_shape() {
            Some(shape) => shape.display_id(),
            None => {
                let cpu = self
                    .cpu_millis
                    .map(|value| format!("{:.0} CPU", value as f64 / 1000.0))
                    .unwrap_or_else(|| "? CPU".into());
                let memory = self
                    .memory_bytes
                    .map(|value| format!("{:.0} GiB", value as f64 / GIB as f64))
                    .unwrap_or_else(|| "? GiB".into());
                format!("{cpu}/{memory}")
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryVisibility {
    Public,
    Private,
}

impl std::fmt::Display for RepositoryVisibility {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Public => write!(formatter, "public"),
            Self::Private => write!(formatter, "private"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapacityScopeKind {
    Repository,
    ProviderAccount,
    Pool,
}

impl std::fmt::Display for CapacityScopeKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Repository => write!(formatter, "repository"),
            Self::ProviderAccount => write!(formatter, "provider_account"),
            Self::Pool => write!(formatter, "pool"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionModel {
    ManagedEphemeral,
    ManagedConfigurable,
    SelfHostedVm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OfferPricing {
    PerMinute {
        usd_per_minute: f64,
        billing_increment_seconds: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context: Option<String>,
    },
    FixedServer {
        usd_per_hour: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        monthly_cap_usd: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context: Option<String>,
    },
    Unknown {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

impl OfferPricing {
    pub fn display(&self) -> String {
        match self {
            Self::PerMinute {
                usd_per_minute,
                billing_increment_seconds,
                ..
            } => match billing_increment_seconds {
                Some(seconds) => format!("${usd_per_minute:.4}/min, {seconds}s increment"),
                None => format!("${usd_per_minute:.4}/min, billing increment unknown"),
            },
            Self::FixedServer {
                usd_per_hour,
                monthly_cap_usd,
                ..
            } => match monthly_cap_usd {
                Some(cap) => format!("${usd_per_hour:.4}/hour, ${cap:.2}/month cap"),
                None => format!("${usd_per_hour:.4}/hour"),
            },
            Self::Unknown { .. } => "unknown".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerOffer {
    pub provider: String,
    pub offer_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_label: Option<String>,
    pub capacity: RunnerCapacity,
    pub os: String,
    pub architecture: String,
    pub execution_model: ExecutionModel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_visibility: Option<RepositoryVisibility>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_capacity_scope: Option<CapacityScopeKind>,
    pub pricing: OfferPricing,
    pub source_url: String,
    pub observed_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl RunnerOffer {
    fn validate(&self) -> Result<()> {
        anyhow::ensure!(!self.provider.trim().is_empty(), "provider is required");
        anyhow::ensure!(!self.offer_id.trim().is_empty(), "offer_id is required");
        anyhow::ensure!(!self.os.trim().is_empty(), "os is required");
        anyhow::ensure!(
            !self.architecture.trim().is_empty(),
            "architecture is required"
        );
        anyhow::ensure!(!self.source_url.trim().is_empty(), "source_url is required");
        anyhow::ensure!(
            !self.observed_at.trim().is_empty(),
            "offer observed_at is required"
        );

        if let Some(cpu_millis) = self.capacity.cpu_millis {
            anyhow::ensure!(cpu_millis > 0, "offer CPU capacity must be positive");
        }
        if let Some(memory_bytes) = self.capacity.memory_bytes {
            anyhow::ensure!(memory_bytes > 0, "offer memory capacity must be positive");
        }

        match &self.pricing {
            OfferPricing::PerMinute {
                usd_per_minute,
                billing_increment_seconds,
                ..
            } => {
                anyhow::ensure!(
                    *usd_per_minute >= 0.0,
                    "per-minute price cannot be negative"
                );
                if let Some(seconds) = billing_increment_seconds {
                    anyhow::ensure!(*seconds > 0, "billing increment must be positive");
                }
            }
            OfferPricing::FixedServer {
                usd_per_hour,
                monthly_cap_usd,
                ..
            } => {
                anyhow::ensure!(*usd_per_hour >= 0.0, "hourly price cannot be negative");
                if let Some(cap) = monthly_cap_usd {
                    anyhow::ensure!(*cap >= 0.0, "monthly cap cannot be negative");
                }
            }
            OfferPricing::Unknown { .. } => {}
        }

        Ok(())
    }

    fn fit(
        &self,
        target: &RunnerShape,
        predicted_duration_ms: u64,
        repository_visibility: Option<&RepositoryVisibility>,
    ) -> Option<OfferFit> {
        if self.execution_model != ExecutionModel::ManagedEphemeral {
            return None;
        }

        if let Some(required_visibility) = &self.repository_visibility
            && repository_visibility != Some(required_visibility)
        {
            return None;
        }

        let shape = self.capacity.complete_shape()?;
        if shape.cpu_millis < target.cpu_millis || shape.memory_bytes < target.memory_bytes {
            return None;
        }

        let OfferPricing::PerMinute {
            usd_per_minute,
            billing_increment_seconds: Some(increment_seconds),
            ..
        } = &self.pricing
        else {
            return None;
        };

        let actual_seconds = predicted_duration_ms.max(1).div_ceil(1000);
        let billed_seconds = actual_seconds.div_ceil(*increment_seconds) * *increment_seconds;
        let estimated_cost_usd = *usd_per_minute * billed_seconds as f64 / 60.0;

        Some(OfferFit {
            provider: self.provider.clone(),
            offer_id: self.offer_id.clone(),
            runner_label: self.runner_label.clone(),
            shape,
            billed_seconds,
            estimated_cost_usd,
            source_url: self.source_url.clone(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfferFit {
    pub provider: String,
    pub offer_id: String,
    pub runner_label: Option<String>,
    pub shape: RunnerShape,
    pub billed_seconds: u64,
    pub estimated_cost_usd: f64,
    pub source_url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer(
        provider: &str,
        cpu: u32,
        memory_gib: u64,
        usd_per_minute: f64,
        billing_increment_seconds: Option<u64>,
    ) -> RunnerOffer {
        RunnerOffer {
            provider: provider.into(),
            offer_id: format!("{provider}-offer"),
            runner_label: None,
            capacity: RunnerCapacity {
                cpu_millis: Some(cpu * 1000),
                memory_bytes: Some(memory_gib * GIB),
            },
            os: "linux".into(),
            architecture: "x86_64".into(),
            execution_model: ExecutionModel::ManagedEphemeral,
            repository_visibility: None,
            required_capacity_scope: None,
            pricing: OfferPricing::PerMinute {
                usd_per_minute,
                billing_increment_seconds,
                context: None,
            },
            source_url: "https://example.com".into(),
            observed_at: "2026-09-28".into(),
            region: None,
            notes: None,
        }
    }

    #[test]
    fn fit_requires_shape_and_known_billing_increment() {
        let target = RunnerShape::new(2_000, 4 * GIB);
        let complete = offer("complete", 2, 8, 0.006, Some(1));
        let unknown_billing = offer("unknown-billing", 2, 8, 0.004, None);

        assert!(complete.fit(&target, 11_000, None).is_some());
        assert!(unknown_billing.fit(&target, 11_000, None).is_none());
    }

    #[test]
    fn billing_increment_changes_short_job_cost() {
        let target = RunnerShape::new(2_000, 4 * GIB);
        let per_minute = offer("minute", 2, 8, 0.006, Some(60));
        let per_second = offer("second", 2, 8, 0.006, Some(1));

        let minute = per_minute.fit(&target, 11_000, None).expect("fit");
        let second = per_second.fit(&target, 11_000, None).expect("fit");

        assert_eq!(minute.billed_seconds, 60);
        assert_eq!(second.billed_seconds, 11);
        assert!(minute.estimated_cost_usd > second.estimated_cost_usd);
    }

    #[test]
    fn context_specific_offer_requires_matching_repository_visibility() {
        let mut contextual = offer("contextual", 4, 16, 0.0, Some(60));
        contextual.repository_visibility = Some(RepositoryVisibility::Public);
        let target = RunnerShape::new(4_000, 8 * GIB);

        assert!(contextual.fit(&target, 11_000, None).is_none());
        assert!(
            contextual
                .fit(&target, 11_000, Some(&RepositoryVisibility::Private))
                .is_none()
        );
        assert!(
            contextual
                .fit(&target, 11_000, Some(&RepositoryVisibility::Public))
                .is_some()
        );
    }

    #[test]
    fn fixed_server_is_not_compared_as_per_job_runner() {
        let offer = RunnerOffer {
            provider: "host".into(),
            offer_id: "vm".into(),
            runner_label: None,
            capacity: RunnerCapacity {
                cpu_millis: Some(4_000),
                memory_bytes: Some(8 * GIB),
            },
            os: "linux".into(),
            architecture: "x86_64".into(),
            execution_model: ExecutionModel::SelfHostedVm,
            repository_visibility: None,
            required_capacity_scope: None,
            pricing: OfferPricing::FixedServer {
                usd_per_hour: 0.016,
                monthly_cap_usd: Some(9.99),
                context: None,
            },
            source_url: "https://example.com".into(),
            observed_at: "2026-09-28".into(),
            region: None,
            notes: None,
        };

        assert!(
            offer
                .fit(&RunnerShape::new(2_000, 4 * GIB), 10_000, None)
                .is_none()
        );
    }
}
