use crate::model::{JobShape, RunObservation, RunnerShape};
use anyhow::{Context, Result};
use duckdb::{params, Connection};
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
        let connection = Connection::open(path).with_context(|| format!("open DuckDB at {}", path.display()))?;
        let store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        self.connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS runs (
                job VARCHAR NOT NULL,
                duration_ms BIGINT NOT NULL,
                cpu_seconds DOUBLE NOT NULL,
                cpu_peak_millis INTEGER NOT NULL,
                memory_peak_bytes BIGINT NOT NULL,
                read_bytes BIGINT NOT NULL,
                write_bytes BIGINT NOT NULL,
                runner_cpu_millis INTEGER NOT NULL,
                runner_memory_bytes BIGINT NOT NULL,
                provider VARCHAR NOT NULL,
                provider_runner VARCHAR NOT NULL,
                queue_ms BIGINT NOT NULL,
                cost_usd DOUBLE NOT NULL,
                exit_code INTEGER NOT NULL,
                sequence BIGINT NOT NULL
            );
            "#,
        )?;
        Ok(())
    }

    pub fn insert_runs(&mut self, runs: &[RunObservation]) -> Result<()> {
        let tx = self.connection.transaction()?;
        {
            let mut statement = tx.prepare(
                r#"
                INSERT INTO runs (
                    job, duration_ms, cpu_seconds, cpu_peak_millis, memory_peak_bytes,
                    read_bytes, write_bytes, runner_cpu_millis, runner_memory_bytes,
                    provider, provider_runner, queue_ms, cost_usd, exit_code, sequence
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )?;

            for run in runs {
                statement.execute(params![
                    run.job,
                    run.duration_ms as i64,
                    run.cpu_seconds,
                    run.cpu_peak_millis as i64,
                    run.memory_peak_bytes as i64,
                    run.read_bytes as i64,
                    run.write_bytes as i64,
                    run.runner.cpu_millis as i64,
                    run.runner.memory_bytes as i64,
                    run.provider,
                    run.provider_runner,
                    run.queue_ms as i64,
                    run.cost_usd,
                    run.exit_code,
                    run.sequence as i64,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn profile(&self, job: &str) -> Result<JobShape> {
        let aggregate = self.connection.query_row(
            r#"
            SELECT
                count(*),
                quantile_cont(duration_ms, 0.50),
                quantile_cont(duration_ms, 0.95),
                quantile_cont(cpu_peak_millis, 0.95),
                quantile_cont(memory_peak_bytes, 0.99)
            FROM runs
            WHERE job = ?
            "#,
            params![job],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, f64>(4)?,
                ))
            },
        )?;

        anyhow::ensure!(aggregate.0 > 0, "no observations for job {job}");

        let current = self.connection.query_row(
            r#"
            SELECT runner_cpu_millis, runner_memory_bytes
            FROM runs
            WHERE job = ?
            ORDER BY sequence DESC
            LIMIT 1
            "#,
            params![job],
            |row| {
                Ok(RunnerShape::new(
                    row.get::<_, i64>(0)? as u32,
                    row.get::<_, i64>(1)? as u64,
                ))
            },
        )?;

        Ok(JobShape {
            job: job.to_string(),
            runs: aggregate.0 as u64,
            duration_p50_ms: aggregate.1,
            duration_p95_ms: aggregate.2,
            cpu_peak_p95_millis: aggregate.3,
            memory_peak_p99_bytes: aggregate.4,
            current_runner: current,
        })
    }
}
