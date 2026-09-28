use crate::economics::{CacheState, CapacitySnapshot, CapacityState};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_GITHUB_API_BASE: &str = "https://api.github.com";
pub const GITHUB_API_VERSION: &str = "2026-03-10";

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
        anyhow::ensure!(!self.provider.trim().is_empty(), "provider is required");
        anyhow::ensure!(!self.offer_id.trim().is_empty(), "offer_id is required");
        anyhow::ensure!(
            !self.runner_label.trim().is_empty(),
            "runner_label is required"
        );
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
        anyhow::ensure!(!token.trim().is_empty(), "GitHub token is empty");
        anyhow::ensure!(!self.api_base.is_empty(), "GitHub API base is empty");
        config.validate()?;

        let observed_at = observed_at_now()?;
        let run_ids = self.active_run_ids(token, &config.repository)?;
        let mut queue_depth = 0_u32;
        let mut running_jobs = 0_u32;

        for run_id in run_ids {
            for job in self.jobs_for_run(token, &config.repository, run_id)? {
                let labels = job.labels.as_deref().unwrap_or_default();
                if !labels.iter().any(|label| label == &config.runner_label) {
                    continue;
                }

                match job.status.as_str() {
                    "queued" => {
                        queue_depth = queue_depth
                            .checked_add(1)
                            .ok_or_else(|| anyhow::anyhow!("queued job count overflow"))?;
                    }
                    "in_progress" => {
                        running_jobs = running_jobs
                            .checked_add(1)
                            .ok_or_else(|| anyhow::anyhow!("running job count overflow"))?;
                    }
                    _ => {}
                }
            }
        }

        let snapshot = CapacitySnapshot {
            schema_version: 1,
            observed_at,
            source: format!(
                "github-actions-rest-v{GITHUB_API_VERSION}:{}",
                config.repository
            ),
            states: vec![CapacityState {
                provider: config.provider.clone(),
                offer_id: config.offer_id.clone(),
                queue_depth,
                running_jobs,
                parallel_slots: config.parallel_slots,
                slot_turnover_ms: config.slot_turnover_ms,
                cache_state: config.cache_state.clone(),
                cache_penalty_ms: config.cache_penalty_ms,
                utilization: None,
            }],
        };
        snapshot.validate()?;
        Ok(snapshot)
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

    #[test]
    fn collects_matching_jobs_with_pagination_and_auth() {
        let responses = vec![
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
        assert!(snapshot.source.contains("owner/repo"));

        let requests = captured
            .recv_timeout(Duration::from_secs(2))
            .expect("captures");
        assert_eq!(requests.len(), 6);
        assert!(requests[1].contains("status=queued&per_page=100&page=2"));
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
