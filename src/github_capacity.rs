use crate::capacity_scope::{CapacityScope, CapacityScopeKind};
use crate::catalog::RepositoryVisibility;
use crate::economics::{CacheState, CapacitySnapshot, CapacityState};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_GITHUB_API_BASE: &str = "https://api.github.com";
pub const GITHUB_API_VERSION: &str = "2026-03-10";
pub const GITHUB_CAPACITY_PLAN_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct GitHubCapacityConfig {
    pub repository: String,
    pub provider: String,
    pub offer_id: String,
    pub runner_label: String,
    pub parallel_slots: u32,
    pub slot_turnover_ms: u64,
    pub cache_state: CacheState,
    pub cache_penalty_ms: u64,
}

impl GitHubCapacityConfig {
    pub fn validate(&self) -> Result<()> {
        validate_repository(&self.repository)?;
        self.pool().validate()
    }

    fn pool(&self) -> GitHubCapacityPool {
        GitHubCapacityPool {
            provider: self.provider.clone(),
            offer_id: self.offer_id.clone(),
            required_labels: vec![self.runner_label.clone()],
            parallel_slots: self.parallel_slots,
            slot_turnover_ms: self.slot_turnover_ms,
            cache_state: self.cache_state.clone(),
            cache_penalty_ms: self.cache_penalty_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubCapacityPlan {
    pub schema_version: u32,
    pub pools: Vec<GitHubCapacityPool>,
}

impl GitHubCapacityPlan {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("read GitHub capacity plan {}", path.display()))?;
        let plan: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse GitHub capacity plan {}", path.display()))?;
        plan.validate()?;
        Ok(plan)
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.schema_version == GITHUB_CAPACITY_PLAN_SCHEMA_VERSION,
            "unsupported GitHub capacity plan schema version {}",
            self.schema_version
        );
        anyhow::ensure!(
            !self.pools.is_empty(),
            "GitHub capacity plan must contain at least one pool"
        );

        let mut identities = BTreeSet::new();
        let mut selectors = BTreeSet::new();
        for pool in &self.pools {
            pool.validate()?;
            anyhow::ensure!(
                identities.insert((pool.provider.clone(), pool.offer_id.clone())),
                "duplicate GitHub capacity pool {}/{}",
                pool.provider,
                pool.offer_id
            );

            let mut selector = pool.required_labels.clone();
            selector.sort();
            anyhow::ensure!(
                selectors.insert(selector),
                "duplicate GitHub capacity runner-label selector"
            );
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubCapacityPool {
    pub provider: String,
    pub offer_id: String,
    pub required_labels: Vec<String>,
    pub parallel_slots: u32,
    pub slot_turnover_ms: u64,
    pub cache_state: CacheState,
    pub cache_penalty_ms: u64,
}

impl GitHubCapacityPool {
    fn validate(&self) -> Result<()> {
        anyhow::ensure!(!self.provider.trim().is_empty(), "provider is required");
        anyhow::ensure!(!self.offer_id.trim().is_empty(), "offer_id is required");
        anyhow::ensure!(
            !self.required_labels.is_empty(),
            "required_labels must contain at least one label"
        );

        let mut labels = BTreeSet::new();
        for label in &self.required_labels {
            anyhow::ensure!(!label.trim().is_empty(), "runner label is required");
            anyhow::ensure!(
                labels.insert(label),
                "duplicate required runner label {label}"
            );
        }

        anyhow::ensure!(self.parallel_slots > 0, "parallel_slots must be positive");
        anyhow::ensure!(
            self.slot_turnover_ms > 0,
            "slot_turnover_ms must be positive"
        );
        if self.cache_state == CacheState::Warm {
            anyhow::ensure!(
                self.cache_penalty_ms == 0,
                "warm cache requires cache_penalty_ms = 0"
            );
        }
        Ok(())
    }

    fn matches(&self, labels: &[String]) -> bool {
        self.required_labels
            .iter()
            .all(|required| labels.iter().any(|label| label == required))
    }
}

#[derive(Debug, Clone)]
pub struct GitHubCapacityClient {
    api_base: String,
}

impl Default for GitHubCapacityClient {
    fn default() -> Self {
        Self::new(DEFAULT_GITHUB_API_BASE)
    }
}

impl GitHubCapacityClient {
    pub fn new(api_base: impl Into<String>) -> Self {
        Self {
            api_base: api_base.into().trim_end_matches('/').to_string(),
        }
    }

    pub fn collect(&self, token: &str, config: &GitHubCapacityConfig) -> Result<CapacitySnapshot> {
        config.validate()?;
        let plan = GitHubCapacityPlan {
            schema_version: GITHUB_CAPACITY_PLAN_SCHEMA_VERSION,
            pools: vec![config.pool()],
        };
        self.collect_plan(token, &config.repository, &plan)
    }

    pub fn collect_plan(
        &self,
        token: &str,
        repository: &str,
        plan: &GitHubCapacityPlan,
    ) -> Result<CapacitySnapshot> {
        anyhow::ensure!(!token.trim().is_empty(), "GitHub token is empty");
        anyhow::ensure!(!self.api_base.is_empty(), "GitHub API base is empty");
        validate_repository(repository)?;
        plan.validate()?;

        let observed_at = observed_at_now()?;
        let repository_visibility = self.repository_visibility(token, repository)?;
        let run_ids = self.active_run_ids(token, repository)?;
        let mut jobs = Vec::new();
        for run_id in run_ids {
            jobs.extend(self.jobs_for_run(token, repository, run_id)?);
        }

        let counts = count_active_jobs(&jobs, plan)?;
        let states = plan
            .pools
            .iter()
            .zip(counts)
            .map(|(pool, (queue_depth, running_jobs))| CapacityState {
                provider: pool.provider.clone(),
                offer_id: pool.offer_id.clone(),
                capacity_scope: Some(CapacityScope::repository(repository)),
                queue_depth,
                running_jobs,
                parallel_slots: pool.parallel_slots,
                slot_turnover_ms: pool.slot_turnover_ms,
                cache_state: pool.cache_state.clone(),
                cache_penalty_ms: pool.cache_penalty_ms,
                utilization: None,
            })
            .collect();

        let snapshot = CapacitySnapshot {
            schema_version: 1,
            observed_at,
            source: format!("github-actions-rest-v{GITHUB_API_VERSION}:{repository}"),
            repository_visibility: Some(repository_visibility),
            states,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    fn repository_visibility(&self, token: &str, repository: &str) -> Result<RepositoryVisibility> {
        let url = format!("{}/repos/{repository}", self.api_base);
        let metadata: RepositoryMetadata = self.get_json(token, &url)?;
        Ok(if metadata.is_private {
            RepositoryVisibility::Private
        } else {
            RepositoryVisibility::Public
        })
    }

    fn active_run_ids(&self, token: &str, repository: &str) -> Result<BTreeSet<u64>> {
        let mut ids = BTreeSet::new();

        for status in ["queued", "in_progress"] {
            let mut page = 1_u32;
            let mut received = 0_usize;

            loop {
                let url = format!(
                    "{}/repos/{repository}/actions/runs?status={status}&per_page=100&page={page}",
                    self.api_base
                );
                let response: WorkflowRunsResponse = self.get_json(token, &url)?;
                anyhow::ensure!(
                    response.total_count <= 1000,
                    "GitHub filtered workflow-run search reported {} results for status {}; API search is capped at 1000",
                    response.total_count,
                    status
                );

                let page_count = response.workflow_runs.len();
                received = received.saturating_add(page_count);
                ids.extend(response.workflow_runs.into_iter().map(|run| run.id));

                if page_count == 0 || received >= response.total_count {
                    break;
                }
                page = page
                    .checked_add(1)
                    .ok_or_else(|| anyhow::anyhow!("workflow-run page overflow"))?;
            }
        }

        Ok(ids)
    }

    fn jobs_for_run(
        &self,
        token: &str,
        repository: &str,
        run_id: u64,
    ) -> Result<Vec<WorkflowJobSummary>> {
        let mut jobs = Vec::new();
        let mut page = 1_u32;
        let mut received = 0_usize;

        loop {
            let url = format!(
                "{}/repos/{repository}/actions/runs/{run_id}/jobs?filter=latest&per_page=100&page={page}",
                self.api_base
            );
            let response: WorkflowJobsResponse = self.get_json(token, &url)?;
            let page_count = response.jobs.len();
            received = received.saturating_add(page_count);
            jobs.extend(response.jobs);

            if page_count == 0 || received >= response.total_count {
                break;
            }
            page = page
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("workflow-job page overflow"))?;
        }

        Ok(jobs)
    }

    fn get_json<T: for<'de> Deserialize<'de>>(&self, token: &str, url: &str) -> Result<T> {
        let authorization = format!("Bearer {token}");
        let mut response = ureq::get(url)
            .header("Authorization", &authorization)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", GITHUB_API_VERSION)
            .header("User-Agent", "cishape/0.1")
            .call()
            .with_context(|| format!("GET {url}"))?;

        response
            .body_mut()
            .read_json::<T>()
            .with_context(|| format!("parse GitHub response from {url}"))
    }
}

fn count_active_jobs(
    jobs: &[WorkflowJobSummary],
    plan: &GitHubCapacityPlan,
) -> Result<Vec<(u32, u32)>> {
    let mut counts = vec![(0_u32, 0_u32); plan.pools.len()];

    for job in jobs {
        if !matches!(job.status.as_str(), "queued" | "in_progress") {
            continue;
        }

        let labels = job.labels.as_deref().unwrap_or_default();
        let matching = plan
            .pools
            .iter()
            .enumerate()
            .filter(|(_, pool)| pool.matches(labels))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();

        if matching.len() > 1 {
            let identities = matching
                .iter()
                .map(|index| {
                    let pool = &plan.pools[*index];
                    format!("{}/{}", pool.provider, pool.offer_id)
                })
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::bail!(
                "active GitHub job matches multiple capacity pools: {identities}; make runner-label selectors disjoint"
            );
        }

        let Some(index) = matching.first().copied() else {
            continue;
        };

        let (queue_depth, running_jobs) = &mut counts[index];
        match job.status.as_str() {
            "queued" => {
                *queue_depth = queue_depth
                    .checked_add(1)
                    .ok_or_else(|| anyhow::anyhow!("queued job count overflow"))?;
            }
            "in_progress" => {
                *running_jobs = running_jobs
                    .checked_add(1)
                    .ok_or_else(|| anyhow::anyhow!("running job count overflow"))?;
            }
            _ => unreachable!("active statuses filtered above"),
        }
    }

    Ok(counts)
}

#[derive(Debug, Deserialize)]
struct RepositoryMetadata {
    #[serde(rename = "private")]
    is_private: bool,
}

#[derive(Debug, Deserialize)]
struct WorkflowRunsResponse {
    total_count: usize,
    workflow_runs: Vec<WorkflowRunSummary>,
}

#[derive(Debug, Deserialize)]
struct WorkflowRunSummary {
    id: u64,
}

#[derive(Debug, Deserialize)]
struct WorkflowJobsResponse {
    total_count: usize,
    jobs: Vec<WorkflowJobSummary>,
}

#[derive(Debug, Deserialize)]
struct WorkflowJobSummary {
    status: String,
    labels: Option<Vec<String>>,
}

fn validate_repository(repository: &str) -> Result<()> {
    let Some((owner, name)) = repository.split_once('/') else {
        anyhow::bail!("repository must be in owner/name form");
    };
    anyhow::ensure!(
        !owner.trim().is_empty() && !name.trim().is_empty() && !name.contains('/'),
        "repository must be in owner/name form"
    );
    anyhow::ensure!(
        owner.chars().all(is_url_safe_repository_char)
            && name.chars().all(is_url_safe_repository_char),
        "repository contains unsupported URL characters"
    );
    Ok(())
}

fn is_url_safe_repository_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
}

fn observed_at_now() -> Result<String> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before Unix epoch")?;
    Ok(format!("unix-ms:{}", elapsed.as_millis()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    fn spawn_server(responses: Vec<&'static str>) -> (String, mpsc::Receiver<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            let mut captured = Vec::new();

            for body in responses {
                let (mut stream, _) = listener.accept().expect("accept");
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .expect("timeout");

                let mut data = Vec::new();
                let mut buffer = [0_u8; 4096];
                loop {
                    match stream.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(count) => {
                            data.extend_from_slice(&buffer[..count]);
                            if data.windows(4).any(|window| window == b"\r\n\r\n") {
                                break;
                            }
                        }
                        Err(error)
                            if matches!(
                                error.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                            ) =>
                        {
                            break;
                        }
                        Err(error) => panic!("read request: {error}"),
                    }
                }

                captured.push(String::from_utf8_lossy(&data).into_owned());

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("write response");
            }

            tx.send(captured).expect("send captures");
        });

        (format!("http://{address}"), rx)
    }

    fn config() -> GitHubCapacityConfig {
        GitHubCapacityConfig {
            repository: "owner/repo".into(),
            provider: "github-actions".into(),
            offer_id: "ubuntu-latest-private-x64".into(),
            runner_label: "ubuntu-latest".into(),
            parallel_slots: 4,
            slot_turnover_ms: 12_000,
            cache_state: CacheState::Warm,
            cache_penalty_ms: 0,
        }
    }

    fn pool(
        provider: &str,
        offer_id: &str,
        required_labels: &[&str],
        parallel_slots: u32,
    ) -> GitHubCapacityPool {
        GitHubCapacityPool {
            provider: provider.into(),
            offer_id: offer_id.into(),
            required_labels: required_labels
                .iter()
                .map(|label| (*label).into())
                .collect(),
            parallel_slots,
            slot_turnover_ms: 12_000,
            cache_state: CacheState::Warm,
            cache_penalty_ms: 0,
        }
    }

    #[test]
    fn collects_matching_jobs_with_pagination_and_auth() {
        let responses = vec![
            r#"{"private":true}"#,
            r#"{"total_count":2,"workflow_runs":[{"id":10}]}"#,
            r#"{"total_count":2,"workflow_runs":[{"id":12}]}"#,
            r#"{"total_count":1,"workflow_runs":[{"id":11}]}"#,
            r#"{"total_count":1,"jobs":[{"status":"queued","labels":["ubuntu-latest"]}]}"#,
            r#"{"total_count":2,"jobs":[{"status":"in_progress","labels":["ubuntu-latest"]},{"status":"completed","labels":["ubuntu-latest"]}]}"#,
            r#"{"total_count":1,"jobs":[{"status":"queued","labels":["windows-latest"]}]}"#,
        ];
        let (api_base, captured) = spawn_server(responses);
        let client = GitHubCapacityClient::new(api_base);

        let snapshot = client.collect("secret-token", &config()).expect("snapshot");
        let state = snapshot.states.first().expect("capacity state");

        assert_eq!(state.queue_depth, 1);
        assert_eq!(state.running_jobs, 1);
        assert_eq!(state.parallel_slots, 4);
        assert_eq!(state.slot_turnover_ms, 12_000);
        assert_eq!(
            state.capacity_scope,
            Some(CapacityScope {
                kind: CapacityScopeKind::Repository,
                key: "owner/repo".into(),
            })
        );
        assert_eq!(
            snapshot.repository_visibility,
            Some(RepositoryVisibility::Private)
        );
        assert!(snapshot.source.contains("owner/repo"));

        let requests = captured
            .recv_timeout(Duration::from_secs(2))
            .expect("captures");
        assert_eq!(requests.len(), 7);
        assert!(requests[0].contains("GET /repos/owner/repo "));
        assert!(requests[2].contains("status=queued&per_page=100&page=2"));
        assert!(requests.iter().all(|request| {
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer secret-token")
        }));
        assert!(requests.iter().all(|request| {
            request
                .to_ascii_lowercase()
                .contains(&format!("x-github-api-version: {GITHUB_API_VERSION}"))
        }));
    }

    #[test]
    fn one_scan_populates_multiple_provider_pools() {
        let responses = vec![
            r#"{"private":true}"#,
            r#"{"total_count":1,"workflow_runs":[{"id":10}]}"#,
            r#"{"total_count":1,"workflow_runs":[{"id":11}]}"#,
            r#"{"total_count":2,"jobs":[{"status":"queued","labels":["ubuntu-latest"]},{"status":"queued","labels":["self-hosted","depot-linux"]}]}"#,
            r#"{"total_count":2,"jobs":[{"status":"in_progress","labels":["ubuntu-latest"]},{"status":"in_progress","labels":["self-hosted","depot-linux"]}]}"#,
        ];
        let (api_base, captured) = spawn_server(responses);
        let client = GitHubCapacityClient::new(api_base);
        let plan = GitHubCapacityPlan {
            schema_version: 1,
            pools: vec![
                pool(
                    "github-actions",
                    "ubuntu-latest-private-x64",
                    &["ubuntu-latest"],
                    4,
                ),
                pool(
                    "depot",
                    "depot-ubuntu-24.04",
                    &["self-hosted", "depot-linux"],
                    8,
                ),
            ],
        };

        let snapshot = client
            .collect_plan("secret-token", "owner/repo", &plan)
            .expect("snapshot");
        assert_eq!(snapshot.states.len(), 2);
        assert_eq!(
            snapshot.repository_visibility,
            Some(RepositoryVisibility::Private)
        );

        let github = snapshot
            .states
            .iter()
            .find(|state| state.provider == "github-actions")
            .expect("GitHub state");
        let depot = snapshot
            .states
            .iter()
            .find(|state| state.provider == "depot")
            .expect("Depot state");

        assert_eq!((github.queue_depth, github.running_jobs), (1, 1));
        assert_eq!((depot.queue_depth, depot.running_jobs), (1, 1));
        assert_eq!(
            github.capacity_scope.as_ref().map(|scope| &scope.kind),
            Some(&CapacityScopeKind::Repository)
        );
        assert_eq!(
            depot.capacity_scope.as_ref().map(|scope| &scope.key),
            Some(&"owner/repo".to_string())
        );

        let requests = captured
            .recv_timeout(Duration::from_secs(2))
            .expect("captures");
        assert_eq!(requests.len(), 5);
    }

    #[test]
    fn ambiguous_pool_match_fails_closed() {
        let plan = GitHubCapacityPlan {
            schema_version: 1,
            pools: vec![
                pool("pool-a", "offer-a", &["self-hosted"], 2),
                pool("pool-b", "offer-b", &["self-hosted", "linux"], 2),
            ],
        };
        let jobs = vec![WorkflowJobSummary {
            status: "queued".into(),
            labels: Some(vec!["self-hosted".into(), "linux".into()]),
        }];

        let error = count_active_jobs(&jobs, &plan).expect_err("ambiguous match must fail");
        assert!(
            error
                .to_string()
                .contains("matches multiple capacity pools")
        );
    }

    #[test]
    fn rejects_implicit_or_inconsistent_capacity_facts() {
        let mut input = config();
        input.parallel_slots = 0;
        assert!(input.validate().is_err());

        input.parallel_slots = 4;
        input.cache_state = CacheState::Warm;
        input.cache_penalty_ms = 1;
        assert!(input.validate().is_err());
    }
}
