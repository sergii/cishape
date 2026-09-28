use crate::decision::DecisionRecord;
use crate::model::{CiIdentity, JobShape, RunObservation, RunnerShape, WorkloadScope};
use anyhow::{Context, Result};
use duckdb::{Connection, params};
use std::path::Path;

pub struct Store {
    connection: Connection,
}

impl Store {
    pub fn memory() -> Result<Self> {
        let connection = Connection::open_in_memory().context("open in-memory DuckDB")?;
        let store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    pub fn open(path: &Path) -> Result<Self> {
        let connection =
            Connection::open(path).with_context(|| format!("open DuckDB at {}", path.display()))?;
        let store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        self.connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS runs (
                observation_id VARCHAR PRIMARY KEY,
                schema_version INTEGER NOT NULL,
                job VARCHAR NOT NULL,
                observed_at_unix_ms BIGINT NOT NULL,
                duration_ms BIGINT NOT NULL,
                cpu_seconds DOUBLE NOT NULL,
                cpu_peak_millis INTEGER NOT NULL,
                memory_peak_bytes BIGINT NOT NULL,
                read_bytes BIGINT NOT NULL,
                write_bytes BIGINT NOT NULL,
                runner_cpu_millis INTEGER NOT NULL,
                runner_memory_bytes BIGINT NOT NULL,
                ci_provider VARCHAR,
                ci_repository VARCHAR,
                ci_workflow VARCHAR,
                ci_run_id VARCHAR,
                ci_run_attempt BIGINT,
                ci_workflow_job VARCHAR,
                commit_sha VARCHAR,
                git_ref VARCHAR,
                runner_name VARCHAR,
                queue_ms BIGINT,
                cost_usd DOUBLE,
                exit_code INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS decisions (
                request_id VARCHAR PRIMARY KEY,
                schema_version INTEGER NOT NULL,
                recorded_at_unix_ms BIGINT NOT NULL,
                provider VARCHAR NOT NULL,
                mode VARCHAR NOT NULL,
                ci_repository VARCHAR,
                job VARCHAR NOT NULL,
                evidence_runs BIGINT NOT NULL,
                deterministic_baseline VARCHAR NOT NULL,
                selected_candidate VARCHAR NOT NULL,
                confidence DOUBLE NOT NULL,
                probabilities_json VARCHAR NOT NULL,
                model VARCHAR NOT NULL,
                input_tokens BIGINT NOT NULL,
                output_tokens BIGINT NOT NULL,
                agrees_with_baseline BOOLEAN NOT NULL,
                record_json VARCHAR NOT NULL
            );
            "#,
        )?;

        let portable_columns: i64 = self.connection.query_row(
            "SELECT count(*) FROM information_schema.columns WHERE table_name = 'runs' AND column_name = 'observation_id'",
            [],
            |row| row.get(0),
        )?;

        anyhow::ensure!(
            portable_columns == 1,
            "existing CIShape DuckDB uses the pre-PORTABLE1 schema; move or remove the local database before continuing"
        );

        Ok(())
    }

    pub fn insert_runs(&mut self, runs: &[RunObservation]) -> Result<usize> {
        let tx = self.connection.transaction()?;
        let mut inserted = 0_usize;
        {
            let mut statement = tx.prepare(
                r#"
                INSERT OR IGNORE INTO runs (
                    observation_id, schema_version, job, observed_at_unix_ms, duration_ms,
                    cpu_seconds, cpu_peak_millis, memory_peak_bytes, read_bytes, write_bytes,
                    runner_cpu_millis, runner_memory_bytes, ci_provider, ci_repository,
                    ci_workflow, ci_run_id, ci_run_attempt, ci_workflow_job, commit_sha,
                    git_ref, runner_name, queue_ms, cost_usd, exit_code
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )?;

            for run in runs {
                run.validate_schema()?;
                let canonical = run.clone().canonicalize();
                inserted += statement.execute(params![
                    canonical.observation_id(),
                    canonical.schema_version as i64,
                    canonical.job,
                    canonical.observed_at_unix_ms as i64,
                    canonical.duration_ms as i64,
                    canonical.cpu_seconds,
                    canonical.cpu_peak_millis as i64,
                    canonical.memory_peak_bytes as i64,
                    canonical.read_bytes as i64,
                    canonical.write_bytes as i64,
                    canonical.runner.cpu_millis as i64,
                    canonical.runner.memory_bytes as i64,
                    canonical.ci.provider,
                    canonical.ci.repository,
                    canonical.ci.workflow,
                    canonical.ci.run_id,
                    canonical.ci.run_attempt.map(|value| value as i64),
                    canonical.ci.workflow_job,
                    canonical.ci.commit_sha,
                    canonical.ci.git_ref,
                    canonical.runner_name,
                    canonical.queue_ms.map(|value| value as i64),
                    canonical.cost_usd,
                    canonical.exit_code,
                ])?;
            }
        }
        tx.commit()?;
        Ok(inserted)
    }

    pub fn insert_decision(&self, record: &DecisionRecord) -> Result<bool> {
        let probabilities_json = serde_json::to_string(&record.probabilities)
            .context("serialize decision probabilities")?;
        let record_json = serde_json::to_string(record).context("serialize DecisionRecord")?;

        let inserted = self.connection.execute(
            r#"
            INSERT OR IGNORE INTO decisions (
                request_id, schema_version, recorded_at_unix_ms, provider, mode,
                ci_repository, job, evidence_runs, deterministic_baseline,
                selected_candidate, confidence, probabilities_json, model,
                input_tokens, output_tokens, agrees_with_baseline, record_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            params![
                record.request_id,
                record.schema_version as i64,
                record.recorded_at_unix_ms as i64,
                record.provider.as_str(),
                record.mode.as_str(),
                record.repository,
                record.job,
                record.evidence_runs as i64,
                record.deterministic_baseline,
                record.selected_candidate,
                record.confidence,
                probabilities_json,
                record.model,
                record.usage.input_tokens as i64,
                record.usage.output_tokens as i64,
                record.agrees_with_baseline,
                record_json,
            ],
        )?;

        Ok(inserted == 1)
    }

    pub fn all_runs(&self) -> Result<Vec<RunObservation>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT
                schema_version, job, observed_at_unix_ms, duration_ms, cpu_seconds,
                cpu_peak_millis, memory_peak_bytes, read_bytes, write_bytes,
                runner_cpu_millis, runner_memory_bytes, ci_provider, ci_repository,
                ci_workflow, ci_run_id, ci_run_attempt, ci_workflow_job, commit_sha,
                git_ref, runner_name, queue_ms, cost_usd, exit_code
            FROM runs
            ORDER BY observed_at_unix_ms, job
            "#,
        )?;

        let rows = statement.query_map([], |row| {
            Ok(RunObservation {
                schema_version: row.get::<_, i64>(0)? as u32,
                job: row.get(1)?,
                observed_at_unix_ms: row.get::<_, i64>(2)? as u64,
                duration_ms: row.get::<_, i64>(3)? as u64,
                cpu_seconds: row.get(4)?,
                cpu_peak_millis: row.get::<_, i64>(5)? as u32,
                memory_peak_bytes: row.get::<_, i64>(6)? as u64,
                read_bytes: row.get::<_, i64>(7)? as u64,
                write_bytes: row.get::<_, i64>(8)? as u64,
                runner: RunnerShape::new(
                    row.get::<_, i64>(9)? as u32,
                    row.get::<_, i64>(10)? as u64,
                ),
                ci: CiIdentity {
                    provider: row.get(11)?,
                    repository: row.get(12)?,
                    workflow: row.get(13)?,
                    run_id: row.get(14)?,
                    run_attempt: row.get::<_, Option<i64>>(15)?.map(|value| value as u64),
                    workflow_job: row.get(16)?,
                    commit_sha: row.get(17)?,
                    git_ref: row.get(18)?,
                },
                runner_name: row.get(19)?,
                provider: None,
                provider_runner: None,
                queue_ms: row.get::<_, Option<i64>>(20)?.map(|value| value as u64),
                cost_usd: row.get(21)?,
                exit_code: row.get(22)?,
            })
        })?;

        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn workload_scopes(&self, repository: Option<&str>) -> Result<Vec<WorkloadScope>> {
        let mut scopes = Vec::new();

        if let Some(repository) = repository {
            let mut statement = self.connection.prepare(
                r#"
                SELECT DISTINCT ci_repository, job
                FROM runs
                WHERE ci_repository = ?
                ORDER BY job
                "#,
            )?;
            let rows = statement.query_map(params![repository], |row| {
                Ok(WorkloadScope {
                    repository: row.get(0)?,
                    job: row.get(1)?,
                })
            })?;
            for row in rows {
                scopes.push(row?);
            }
        } else {
            let mut statement = self.connection.prepare(
                r#"
                SELECT DISTINCT ci_repository, job
                FROM runs
                ORDER BY coalesce(ci_repository, ''), job
                "#,
            )?;
            let rows = statement.query_map([], |row| {
                Ok(WorkloadScope {
                    repository: row.get(0)?,
                    job: row.get(1)?,
                })
            })?;
            for row in rows {
                scopes.push(row?);
            }
        }

        Ok(scopes)
    }

    pub fn profile(&self, job: &str) -> Result<JobShape> {
        self.profile_for(job, None)
    }

    pub fn profile_for(&self, job: &str, repository: Option<&str>) -> Result<JobShape> {
        let repository = self.resolve_repository_scope(job, repository)?;
        self.profile_scope(job, repository.as_deref())
    }

    pub fn profile_scope(&self, job: &str, repository: Option<&str>) -> Result<JobShape> {
        let aggregate = if let Some(repository) = repository {
            self.connection.query_row(
                r#"
                SELECT
                    count(*),
                    quantile_cont(duration_ms, 0.50),
                    quantile_cont(duration_ms, 0.95),
                    quantile_cont(cpu_peak_millis, 0.95),
                    quantile_cont(memory_peak_bytes, 0.99)
                FROM runs
                WHERE job = ? AND ci_repository = ?
                "#,
                params![job, repository],
                aggregate_row,
            )?
        } else {
            self.connection.query_row(
                r#"
                SELECT
                    count(*),
                    quantile_cont(duration_ms, 0.50),
                    quantile_cont(duration_ms, 0.95),
                    quantile_cont(cpu_peak_millis, 0.95),
                    quantile_cont(memory_peak_bytes, 0.99)
                FROM runs
                WHERE job = ? AND ci_repository IS NULL
                "#,
                params![job],
                aggregate_row,
            )?
        };

        anyhow::ensure!(aggregate.0 > 0, "no observations for workload scope");

        let current = if let Some(repository) = repository {
            self.connection.query_row(
                r#"
                SELECT runner_cpu_millis, runner_memory_bytes
                FROM runs
                WHERE job = ? AND ci_repository = ?
                ORDER BY observed_at_unix_ms DESC
                LIMIT 1
                "#,
                params![job, repository],
                runner_row,
            )?
        } else {
            self.connection.query_row(
                r#"
                SELECT runner_cpu_millis, runner_memory_bytes
                FROM runs
                WHERE job = ? AND ci_repository IS NULL
                ORDER BY observed_at_unix_ms DESC
                LIMIT 1
                "#,
                params![job],
                runner_row,
            )?
        };

        Ok(JobShape {
            job: job.to_string(),
            repository: repository.map(str::to_string),
            runs: aggregate.0 as u64,
            duration_p50_ms: aggregate.1,
            duration_p95_ms: aggregate.2,
            cpu_peak_p95_millis: aggregate.3,
            memory_peak_p99_bytes: aggregate.4,
            current_runner: current,
        })
    }

    fn resolve_repository_scope(
        &self,
        job: &str,
        requested_repository: Option<&str>,
    ) -> Result<Option<String>> {
        if let Some(repository) = requested_repository {
            let count: i64 = self.connection.query_row(
                "SELECT count(*) FROM runs WHERE job = ? AND ci_repository = ?",
                params![job, repository],
                |row| row.get(0),
            )?;
            anyhow::ensure!(
                count > 0,
                "no observations for job {job} in repository {repository}"
            );
            return Ok(Some(repository.to_string()));
        }

        let (rows, scopes, repository): (i64, i64, Option<String>) = self.connection.query_row(
            r#"
                SELECT
                    count(*),
                    count(DISTINCT coalesce(ci_repository, '<local>')),
                    max(ci_repository)
                FROM runs
                WHERE job = ?
                "#,
            params![job],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;

        anyhow::ensure!(rows > 0, "no observations for job {job}");
        anyhow::ensure!(
            scopes <= 1,
            "job {job} exists in multiple repositories; pass --repository to select one"
        );

        Ok(repository)
    }
}

fn aggregate_row(row: &duckdb::Row<'_>) -> duckdb::Result<(i64, f64, f64, f64, f64)> {
    Ok((
        row.get::<_, i64>(0)?,
        row.get::<_, f64>(1)?,
        row.get::<_, f64>(2)?,
        row.get::<_, f64>(3)?,
        row.get::<_, f64>(4)?,
    ))
}

fn runner_row(row: &duckdb::Row<'_>) -> duckdb::Result<RunnerShape> {
    Ok(RunnerShape::new(
        row.get::<_, i64>(0)? as u32,
        row.get::<_, i64>(1)? as u64,
    ))
}
