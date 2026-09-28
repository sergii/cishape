use anyhow::{Context, Result};
use cishape::advisory::{ADVISORY_SCHEMA_VERSION, AdvisoryReport, advisory_item, to_markdown};
use cishape::batch::evaluate as evaluate_batch_economics;
use cishape::catalog::ProviderCatalog;
use cishape::decision::{
    JevRequestBundle, JevSystemOneResponse, prepare_jev_request, record_deterministic_decision,
    record_jev_response,
};
use cishape::economics::{CapacitySnapshot, evaluate as evaluate_economics};
use cishape::economics_policy::{
    EconomicsDecisionReport, EconomicsPolicy, WorkflowEconomicsDecisionReport,
    select as select_economics, select_batch as select_batch_economics,
    select_workflow as select_workflow_economics, to_markdown as economics_decision_to_markdown,
    workflow_decision_to_markdown,
};
use cishape::interchange;
use cishape::jev_http::{DEFAULT_JEV_ENDPOINT, JevHttpClient};
use cishape::model::{GIB, JobShape, Recommendation, RunObservation, RunnerShape};
use cishape::observe;
use cishape::optimize::{
    default_catalog, feasible_candidates_with_policy, recommend, recommend_with_policy,
};
use cishape::outcome::{evaluate as evaluate_outcome, to_markdown as outcome_to_markdown};
use cishape::policy::OptimizationPolicy;
use cishape::store::Store;
use cishape::synthetic;
use cishape::workflow::{WorkflowDemand, evaluate as evaluate_workflow_economics};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(name = "cishape")]
#[command(about = "Observe CI workload shapes and fit them to better runner shapes")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Debug, ValueEnum)]
enum ExportFormat {
    Jsonl,
}

#[derive(Clone, Debug, ValueEnum)]
enum ReportFormat {
    Text,
    Markdown,
}

#[derive(Clone, Debug, ValueEnum)]
enum AdvisoryFormat {
    Json,
    Markdown,
}

#[derive(Clone, Debug, ValueEnum)]
enum OutcomeFormat {
    Json,
    Markdown,
}

#[derive(Clone, Debug, ValueEnum)]
enum EconomicsFormat {
    Json,
    Markdown,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the complete synthetic proof-of-concept without writing local state.
    Demo,
    /// Generate deterministic synthetic history in a local DuckDB database.
    Synth {
        #[arg(long, default_value = ".cishape/cishape.duckdb")]
        db: PathBuf,
        #[arg(long, default_value = "lint")]
        job: String,
        #[arg(long, default_value_t = 100)]
        runs: usize,
    },
    /// Observe a real child process and persist one normalized run.
    Observe {
        #[arg(long)]
        job: String,
        #[arg(long, default_value = ".cishape/cishape.duckdb")]
        db: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(last = true, required = true, num_args = 1..)]
        command: Vec<String>,
    },
    /// Import one or more portable RunObservation JSON/JSONL files.
    Import {
        #[arg(long, default_value = ".cishape/cishape.duckdb")]
        db: PathBuf,
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Export local history in a portable format.
    Export {
        #[arg(long, default_value = ".cishape/cishape.duckdb")]
        db: PathBuf,
        #[arg(long, value_enum, default_value = "jsonl")]
        format: ExportFormat,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Inspect a versioned provider runner catalog.
    Catalog {
        #[arg(long, default_value = "catalogs/providers-v1.json")]
        path: PathBuf,
    },
    /// Fit a canonical target shape to comparable managed provider offers.
    CatalogFit {
        #[arg(long, default_value = "catalogs/providers-v1.json")]
        path: PathBuf,
        #[arg(long)]
        cpu: u32,
        #[arg(long)]
        memory_gib: u64,
        #[arg(long)]
        duration_ms: u64,
    },
    /// Evaluate provider offers against a runtime queue/capacity snapshot.
    Economics {
        #[arg(long, default_value = "catalogs/providers-v1.json")]
        catalog: PathBuf,
        #[arg(long)]
        snapshot: PathBuf,
        #[arg(long, default_value = "policies/economics-default-v1.json")]
        policy: PathBuf,
        #[arg(long)]
        cpu: u32,
        #[arg(long)]
        memory_gib: u64,
        #[arg(long)]
        duration_ms: u64,
        #[arg(long, default_value_t = 1)]
        jobs: u32,
        #[arg(long, value_enum, default_value = "markdown")]
        format: EconomicsFormat,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Evaluate a heterogeneous workflow DAG against provider capacity.
    WorkflowEconomics {
        #[arg(long, default_value = "catalogs/providers-v1.json")]
        catalog: PathBuf,
        #[arg(long)]
        snapshot: PathBuf,
        #[arg(long)]
        workflow: PathBuf,
        #[arg(long, default_value = "policies/economics-default-v1.json")]
        policy: PathBuf,
        #[arg(long, value_enum, default_value = "markdown")]
        format: EconomicsFormat,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Build a deterministic advisory report over all workload scopes.
    Advisory {
        #[arg(long, default_value = ".cishape/cishape.duckdb")]
        db: PathBuf,
        #[arg(long)]
        repository: Option<String>,
        #[arg(long, default_value = "policies/default-v1.json")]
        policy: PathBuf,
        #[arg(long, value_enum, default_value = "markdown")]
        format: AdvisoryFormat,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Evaluate one recorded decision against later matching observations.
    Outcome {
        #[arg(long, default_value = ".cishape/cishape.duckdb")]
        db: PathBuf,
        #[arg(long)]
        decision: PathBuf,
        #[arg(long, default_value = "policies/default-v1.json")]
        policy: PathBuf,
        #[arg(long, value_enum, default_value = "markdown")]
        format: OutcomeFormat,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Summarize all workload profiles in local history.
    Report {
        #[arg(long, default_value = ".cishape/cishape.duckdb")]
        db: PathBuf,
        #[arg(long)]
        repository: Option<String>,
        #[arg(long, value_enum, default_value = "text")]
        format: ReportFormat,
    },
    /// Produce and persist a deterministic DecisionRecord from historical evidence.
    Decide {
        #[arg(long, default_value = ".cishape/cishape.duckdb")]
        db: PathBuf,
        #[arg(long)]
        repository: Option<String>,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long, default_value = "policies/default-v1.json")]
        policy: PathBuf,
        job: String,
    },
    /// Prepare an offline Jev shadow decision request from historical evidence.
    DecisionPrepare {
        #[arg(long, default_value = ".cishape/cishape.duckdb")]
        db: PathBuf,
        #[arg(long)]
        repository: Option<String>,
        #[arg(long, default_value = "jev-latest")]
        model: String,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long, default_value = "policies/default-v1.json")]
        policy: PathBuf,
        job: String,
    },
    /// Send a prepared Jev request, validate the response, and record a shadow decision.
    DecisionRun {
        #[arg(long, default_value = ".cishape/cishape.duckdb")]
        db: PathBuf,
        #[arg(long)]
        request: PathBuf,
        #[arg(long, default_value = DEFAULT_JEV_ENDPOINT)]
        endpoint: String,
        #[arg(long, default_value = "JEV_API_KEY")]
        api_key_env: String,
        #[arg(long)]
        response_output: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Record an offline Jev response as a shadow DecisionRecord.
    DecisionRecord {
        #[arg(long, default_value = ".cishape/cishape.duckdb")]
        db: PathBuf,
        #[arg(long)]
        request: PathBuf,
        #[arg(long)]
        response: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Build a historical JobShape from stored runs.
    Profile {
        #[arg(long, default_value = ".cishape/cishape.duckdb")]
        db: PathBuf,
        #[arg(long)]
        repository: Option<String>,
        job: String,
    },
    /// Recommend the cheapest runner shape that satisfies v0 safety constraints.
    Recommend {
        #[arg(long, default_value = ".cishape/cishape.duckdb")]
        db: PathBuf,
        #[arg(long)]
        repository: Option<String>,
        #[arg(long, default_value = "policies/default-v1.json")]
        policy: PathBuf,
        job: String,
    },
    /// Explain the deterministic recommendation and its safety margins.
    Explain {
        #[arg(long, default_value = ".cishape/cishape.duckdb")]
        db: PathBuf,
        #[arg(long)]
        repository: Option<String>,
        #[arg(long, default_value = "policies/default-v1.json")]
        policy: PathBuf,
        job: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Demo => demo(),
        Command::Synth { db, job, runs } => synth(&db, &job, runs),
        Command::Observe {
            job,
            db,
            output,
            command,
        } => observe_command(&job, &db, output.as_deref(), &command),
        Command::Import { db, files } => import_command(&db, &files),
        Command::Export { db, format, output } => export_command(&db, format, output.as_deref()),
        Command::Catalog { path } => catalog_command(&path),
        Command::CatalogFit {
            path,
            cpu,
            memory_gib,
            duration_ms,
        } => catalog_fit_command(&path, cpu, memory_gib, duration_ms),
        Command::Economics {
            catalog,
            snapshot,
            policy,
            cpu,
            memory_gib,
            duration_ms,
            jobs,
            format,
            output,
        } => economics_command(
            &catalog,
            &snapshot,
            &policy,
            (cpu, memory_gib, jobs),
            duration_ms,
            format,
            output.as_deref(),
        ),
        Command::WorkflowEconomics {
            catalog,
            snapshot,
            workflow,
            policy,
            format,
            output,
        } => workflow_economics_command(
            &catalog,
            &snapshot,
            &workflow,
            &policy,
            format,
            output.as_deref(),
        ),
        Command::Advisory {
            db,
            repository,
            policy,
            format,
            output,
        } => advisory_command(
            &db,
            repository.as_deref(),
            &policy,
            format,
            output.as_deref(),
        ),
        Command::Outcome {
            db,
            decision,
            policy,
            format,
            output,
        } => outcome_command(&db, &decision, &policy, format, output.as_deref()),
        Command::Report {
            db,
            repository,
            format,
        } => report_command(&db, repository.as_deref(), format),
        Command::Decide {
            db,
            repository,
            output,
            policy,
            job,
        } => decide_command(&db, repository.as_deref(), &job, &policy, output.as_deref()),
        Command::DecisionPrepare {
            db,
            repository,
            model,
            output,
            policy,
            job,
        } => decision_prepare_command(
            &db,
            repository.as_deref(),
            &job,
            &model,
            &policy,
            output.as_deref(),
        ),
        Command::DecisionRun {
            db,
            request,
            endpoint,
            api_key_env,
            response_output,
            output,
        } => decision_run_command(
            &db,
            &request,
            &endpoint,
            &api_key_env,
            response_output.as_deref(),
            output.as_deref(),
        ),
        Command::DecisionRecord {
            db,
            request,
            response,
            output,
        } => decision_record_command(&db, &request, &response, output.as_deref()),
        Command::Profile {
            db,
            repository,
            job,
        } => {
            let store = Store::open(&db)?;
            print_profile(&store.profile_for(&job, repository.as_deref())?);
            Ok(())
        }
        Command::Recommend {
            db,
            repository,
            policy,
            job,
        } => {
            let store = Store::open(&db)?;
            let profile = store.profile_for(&job, repository.as_deref())?;
            let policy = OptimizationPolicy::load(&policy)?;
            let recommendation = recommend_with_policy(&profile, &default_catalog(), &policy)
                .with_context(|| format!("no feasible runner candidate for {}", profile.job))?;
            print_recommendation(&recommendation);
            Ok(())
        }
        Command::Explain {
            db,
            repository,
            policy,
            job,
        } => {
            let store = Store::open(&db)?;
            let profile = store.profile_for(&job, repository.as_deref())?;
            let policy = OptimizationPolicy::load(&policy)?;
            let recommendation = recommend_with_policy(&profile, &default_catalog(), &policy)
                .with_context(|| format!("no feasible runner candidate for {}", profile.job))?;
            print_explanation(&profile, &recommendation, &policy);
            Ok(())
        }
    }
}

fn demo() -> Result<()> {
    let mut store = Store::memory()?;
    let runs = synthetic::oversized_lint("lint", 100);
    store.insert_runs(&runs)?;

    let profile = store.profile("lint")?;
    let recommendation = recommendation_for(&profile)?;

    println!("CIShape POC0 - synthetic oversized lint");
    println!();
    print_profile(&profile);
    println!();
    print_recommendation(&recommendation);
    println!();
    print_explanation(&profile, &recommendation, &OptimizationPolicy::default_v1());

    Ok(())
}

fn synth(path: &Path, job: &str, runs: usize) -> Result<()> {
    ensure_parent(path)?;

    let mut store = Store::open(path)?;
    let observations = synthetic::oversized_lint(job, runs);
    store.insert_runs(&observations)?;

    println!(
        "stored {runs} synthetic runs for {job} in {}",
        path.display()
    );
    Ok(())
}

fn observe_command(job: &str, db: &Path, output: Option<&Path>, command: &[String]) -> Result<()> {
    let (program, args) = command
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("observed command is required"))?;

    let observation = observe::command(job, program, args)?;

    let output_path = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_observation_path(&observation));

    ensure_parent(&output_path)?;
    let payload = serde_json::to_vec_pretty(&observation).context("serialize RunObservation")?;
    std::fs::write(&output_path, payload)
        .with_context(|| format!("write {}", output_path.display()))?;

    ensure_parent(db)?;
    let mut store = Store::open(db)?;
    store.insert_runs(std::slice::from_ref(&observation))?;
    drop(store);

    print_observation(&observation, &output_path);

    if observation.exit_code != 0 {
        std::process::exit(observation.exit_code);
    }

    Ok(())
}

fn import_command(db: &Path, files: &[PathBuf]) -> Result<()> {
    ensure_parent(db)?;
    let observations = interchange::read_observations(files)?;
    let total = observations.len();
    let mut store = Store::open(db)?;
    let inserted = store.insert_runs(&observations)?;

    println!(
        "imported {inserted} new observations; skipped {} duplicates",
        total.saturating_sub(inserted)
    );
    Ok(())
}

fn export_command(db: &Path, format: ExportFormat, output: Option<&Path>) -> Result<()> {
    let store = Store::open(db)?;
    let runs = store.all_runs()?;

    match (format, output) {
        (ExportFormat::Jsonl, Some(path)) => {
            ensure_parent(path)?;
            let file = std::fs::File::create(path)
                .with_context(|| format!("create {}", path.display()))?;
            interchange::write_jsonl(&runs, file)?;
            println!("exported {} observations to {}", runs.len(), path.display());
        }
        (ExportFormat::Jsonl, None) => {
            let stdout = std::io::stdout();
            interchange::write_jsonl(&runs, stdout.lock())?;
        }
    }

    Ok(())
}

fn catalog_command(path: &Path) -> Result<()> {
    let catalog = ProviderCatalog::load(path)?;

    println!(
        "Provider catalog v{} observed {}",
        catalog.schema_version, catalog.observed_at
    );
    println!();
    println!("provider\toffer\tcapacity\texecution\tpricing");

    for offer in catalog.offers {
        println!(
            "{}\t{}\t{}\t{:?}\t{}",
            offer.provider,
            offer.offer_id,
            offer.capacity.display(),
            offer.execution_model,
            offer.pricing.display()
        );
    }

    Ok(())
}

fn catalog_fit_command(path: &Path, cpu: u32, memory_gib: u64, duration_ms: u64) -> Result<()> {
    anyhow::ensure!(cpu > 0, "CPU must be positive");
    anyhow::ensure!(memory_gib > 0, "memory-gib must be positive");
    anyhow::ensure!(duration_ms > 0, "duration-ms must be positive");

    let catalog = ProviderCatalog::load(path)?;
    let target = RunnerShape::new(cpu * 1000, memory_gib * GIB);
    let matches = catalog.fit(&target, duration_ms);

    println!(
        "Provider fits for {} at {:.2}s",
        target.display_id(),
        duration_ms as f64 / 1000.0
    );

    if matches.is_empty() {
        println!("  no comparable managed offers");
        return Ok(());
    }

    for offer in matches {
        let label = offer.runner_label.as_deref().unwrap_or("-");
        println!(
            "  {} / {} / {} / {} / billed {}s / ${:.6}",
            offer.provider,
            offer.offer_id,
            label,
            offer.shape.display_id(),
            offer.billed_seconds,
            offer.estimated_cost_usd
        );
    }

    Ok(())
}

fn economics_command(
    catalog_path: &Path,
    snapshot_path: &Path,
    policy_path: &Path,
    workload: (u32, u64, u32),
    duration_ms: u64,
    format: EconomicsFormat,
    output: Option<&Path>,
) -> Result<()> {
    let (cpu, memory_gib, jobs) = workload;
    anyhow::ensure!(cpu > 0, "CPU must be positive");
    anyhow::ensure!(memory_gib > 0, "memory-gib must be positive");
    anyhow::ensure!(jobs > 0, "jobs must be positive");
    anyhow::ensure!(duration_ms > 0, "duration-ms must be positive");

    let catalog = ProviderCatalog::load(catalog_path)?;
    let snapshot = CapacitySnapshot::load(snapshot_path)?;
    let policy = EconomicsPolicy::load(policy_path)?;
    let target = RunnerShape::new(cpu * 1000, memory_gib * GIB);
    let economics = evaluate_economics(&catalog, &snapshot, &target, duration_ms)?;
    let (batch, selection) = if jobs == 1 {
        (None, select_economics(&economics, &policy)?)
    } else {
        let batch = evaluate_batch_economics(&economics, &snapshot, jobs)?;
        let selection = select_batch_economics(&batch, &policy)?;
        (Some(batch), selection)
    };
    let report = EconomicsDecisionReport {
        economics,
        batch,
        selection,
    };

    let payload = match format {
        EconomicsFormat::Json => {
            serde_json::to_string_pretty(&report).context("serialize economics decision report")?
        }
        EconomicsFormat::Markdown => economics_decision_to_markdown(&report),
    };

    if let Some(path) = output {
        ensure_parent(path)?;
        std::fs::write(path, payload.as_bytes())
            .with_context(|| format!("write {}", path.display()))?;
        println!("wrote capacity economics report to {}", path.display());
    } else {
        print!("{payload}");
        if !payload.ends_with('\n') {
            println!();
        }
    }

    Ok(())
}

fn workflow_economics_command(
    catalog_path: &Path,
    snapshot_path: &Path,
    workflow_path: &Path,
    policy_path: &Path,
    format: EconomicsFormat,
    output: Option<&Path>,
) -> Result<()> {
    let catalog = ProviderCatalog::load(catalog_path)?;
    let snapshot = CapacitySnapshot::load(snapshot_path)?;
    let workflow = WorkflowDemand::load(workflow_path)?;
    let policy = EconomicsPolicy::load(policy_path)?;
    let workflow = evaluate_workflow_economics(&catalog, &snapshot, &workflow)?;
    let selection = select_workflow_economics(&workflow, &policy)?;
    let report = WorkflowEconomicsDecisionReport {
        workflow,
        selection,
    };

    let payload = match format {
        EconomicsFormat::Json => serde_json::to_string_pretty(&report)
            .context("serialize workflow economics decision report")?,
        EconomicsFormat::Markdown => workflow_decision_to_markdown(&report),
    };

    if let Some(path) = output {
        ensure_parent(path)?;
        std::fs::write(path, payload.as_bytes())
            .with_context(|| format!("write {}", path.display()))?;
        println!("wrote workflow economics report to {}", path.display());
    } else {
        print!("{payload}");
        if !payload.ends_with('\n') {
            println!();
        }
    }

    Ok(())
}

fn advisory_command(
    db: &Path,
    repository: Option<&str>,
    policy_path: &Path,
    format: AdvisoryFormat,
    output: Option<&Path>,
) -> Result<()> {
    let policy = OptimizationPolicy::load(policy_path)?;
    let store = Store::open(db)?;
    let scopes = store.workload_scopes(repository)?;
    anyhow::ensure!(!scopes.is_empty(), "history is empty");

    let catalog = default_catalog();
    let mut items = Vec::with_capacity(scopes.len());

    for scope in scopes {
        let profile = store.profile_scope(&scope.job, scope.repository.as_deref())?;
        let recommendation = recommend_with_policy(&profile, &catalog, &policy);
        items.push(advisory_item(
            &profile,
            recommendation.as_ref(),
            policy.min_runs,
        ));
    }

    let report = AdvisoryReport {
        schema_version: ADVISORY_SCHEMA_VERSION,
        policy_id: policy.policy_id.clone(),
        policy_schema_version: policy.schema_version,
        min_runs: policy.min_runs,
        items,
    };

    let payload = match format {
        AdvisoryFormat::Json => {
            serde_json::to_string_pretty(&report).context("serialize advisory report")?
        }
        AdvisoryFormat::Markdown => to_markdown(&report),
    };

    if let Some(path) = output {
        ensure_parent(path)?;
        std::fs::write(path, payload.as_bytes())
            .with_context(|| format!("write {}", path.display()))?;
        println!("wrote deterministic advisory to {}", path.display());
    } else {
        print!("{payload}");
        if !payload.ends_with('\n') {
            println!();
        }
    }

    Ok(())
}

fn outcome_command(
    db: &Path,
    decision_path: &Path,
    policy_path: &Path,
    format: OutcomeFormat,
    output: Option<&Path>,
) -> Result<()> {
    let policy = OptimizationPolicy::load(policy_path)?;
    let decision: cishape::decision::DecisionRecord = serde_json::from_slice(
        &std::fs::read(decision_path)
            .with_context(|| format!("read {}", decision_path.display()))?,
    )
    .with_context(|| format!("parse {}", decision_path.display()))?;

    let store = Store::open(db)?;
    let observations = store.all_runs()?;
    let outcome = evaluate_outcome(&decision, &observations, policy.min_runs)?;

    let payload = match format {
        OutcomeFormat::Json => {
            serde_json::to_string_pretty(&outcome).context("serialize outcome")?
        }
        OutcomeFormat::Markdown => outcome_to_markdown(&outcome),
    };

    if let Some(path) = output {
        ensure_parent(path)?;
        std::fs::write(path, payload.as_bytes())
            .with_context(|| format!("write {}", path.display()))?;
        println!("wrote outcome evidence to {}", path.display());
    } else {
        print!("{payload}");
        if !payload.ends_with('\n') {
            println!();
        }
    }

    Ok(())
}

fn report_command(db: &Path, repository: Option<&str>, format: ReportFormat) -> Result<()> {
    let store = Store::open(db)?;
    let scopes = store.workload_scopes(repository)?;

    anyhow::ensure!(!scopes.is_empty(), "history is empty");

    match format {
        ReportFormat::Text => {
            for (index, scope) in scopes.iter().enumerate() {
                if index > 0 {
                    println!();
                }
                let profile = store.profile_scope(&scope.job, scope.repository.as_deref())?;
                print_profile(&profile);
            }
        }
        ReportFormat::Markdown => {
            println!("| Repository | Job | Runs | p50 | p95 | CPU p95 | RAM p99 | Runner |");
            println!("| --- | --- | ---: | ---: | ---: | ---: | ---: | --- |");

            for scope in scopes {
                let profile = store.profile_scope(&scope.job, scope.repository.as_deref())?;
                let repository = profile.repository.as_deref().unwrap_or("local");
                println!(
                    "| {repository} | {} | {} | {:.2}s | {:.2}s | {:.2} cores | {:.0} MiB | {} |",
                    profile.job,
                    profile.runs,
                    profile.duration_p50_ms / 1000.0,
                    profile.duration_p95_ms / 1000.0,
                    profile.cpu_peak_p95_millis / 1000.0,
                    profile.memory_peak_p99_bytes / (1024.0 * 1024.0),
                    profile.current_runner
                );
            }
        }
    }

    Ok(())
}

fn decide_command(
    db: &Path,
    repository: Option<&str>,
    job: &str,
    policy_path: &Path,
    output: Option<&Path>,
) -> Result<()> {
    let policy = OptimizationPolicy::load(policy_path)?;
    let store = Store::open(db)?;
    let profile = store.profile_for(job, repository)?;
    let catalog = default_catalog();
    let recommendation = recommend_with_policy(&profile, &catalog, &policy)
        .with_context(|| format!("no deterministic recommendation for {job}"))?;
    let feasible = feasible_candidates_with_policy(&profile, &catalog, &policy);
    let record = record_deterministic_decision(&profile, &recommendation, &feasible)?;

    persist_decision_record(db, &record, output)
}

fn decision_prepare_command(
    db: &Path,
    repository: Option<&str>,
    job: &str,
    model: &str,
    policy_path: &Path,
    output: Option<&Path>,
) -> Result<()> {
    let policy = OptimizationPolicy::load(policy_path)?;
    let store = Store::open(db)?;
    let profile = store.profile_for(job, repository)?;
    let catalog = default_catalog();
    let recommendation = recommend_with_policy(&profile, &catalog, &policy)
        .with_context(|| format!("no deterministic recommendation for {job}"))?;
    let feasible = feasible_candidates_with_policy(&profile, &catalog, &policy);
    let bundle = prepare_jev_request(&profile, &recommendation, &feasible, Some(model))?;

    let output_path = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(format!(".cishape/decisions/{job}-jev-request.json")));
    ensure_parent(&output_path)?;
    std::fs::write(
        &output_path,
        serde_json::to_vec_pretty(&bundle).context("serialize Jev request bundle")?,
    )
    .with_context(|| format!("write {}", output_path.display()))?;

    println!("prepared Jev shadow decision request");
    println!("  workload          {}", job);
    println!("  evidence runs     {}", profile.runs);
    println!(
        "  baseline          {}",
        recommendation.recommended.shape.display_id()
    );
    println!("  feasible choices  {}", feasible.len());
    println!("  model             {}", model);
    println!("  request           {}", output_path.display());
    Ok(())
}

fn decision_run_command(
    db: &Path,
    request_path: &Path,
    endpoint: &str,
    api_key_env: &str,
    response_output: Option<&Path>,
    output: Option<&Path>,
) -> Result<()> {
    let bundle: JevRequestBundle = serde_json::from_slice(
        &std::fs::read(request_path).with_context(|| format!("read {}", request_path.display()))?,
    )
    .with_context(|| format!("parse {}", request_path.display()))?;

    let api_key = std::env::var(api_key_env)
        .with_context(|| format!("missing Jev API key in environment variable {api_key_env}"))?;
    let client = JevHttpClient::new(endpoint);
    let response = client.send(&api_key, &bundle.wire)?;

    let safe_request_id = safe_file_component(&bundle.decision.request_id);
    let response_path = response_output.map(Path::to_path_buf).unwrap_or_else(|| {
        PathBuf::from(format!(
            ".cishape/decisions/{safe_request_id}-jev-response.json"
        ))
    });
    ensure_parent(&response_path)?;
    std::fs::write(
        &response_path,
        serde_json::to_vec_pretty(&response).context("serialize Jev response")?,
    )
    .with_context(|| format!("write {}", response_path.display()))?;

    let record = record_jev_response(&bundle, response)?;
    persist_decision_record(db, &record, output)?;

    println!("  endpoint          {}", client.endpoint());
    println!("  response          {}", response_path.display());
    Ok(())
}

fn decision_record_command(
    db: &Path,
    request_path: &Path,
    response_path: &Path,
    output: Option<&Path>,
) -> Result<()> {
    let bundle: JevRequestBundle = serde_json::from_slice(
        &std::fs::read(request_path).with_context(|| format!("read {}", request_path.display()))?,
    )
    .with_context(|| format!("parse {}", request_path.display()))?;

    let response: JevSystemOneResponse = serde_json::from_slice(
        &std::fs::read(response_path)
            .with_context(|| format!("read {}", response_path.display()))?,
    )
    .with_context(|| format!("parse {}", response_path.display()))?;

    let record = record_jev_response(&bundle, response)?;
    persist_decision_record(db, &record, output)
}

fn persist_decision_record(
    db: &Path,
    record: &cishape::decision::DecisionRecord,
    output: Option<&Path>,
) -> Result<()> {
    let store = Store::open(db)?;
    let inserted = store.insert_decision(record)?;

    let output_path = output.map(Path::to_path_buf).unwrap_or_else(|| {
        PathBuf::from(format!(
            ".cishape/decisions/{}.json",
            safe_file_component(&record.request_id)
        ))
    });

    ensure_parent(&output_path)?;
    std::fs::write(
        &output_path,
        serde_json::to_vec_pretty(record).context("serialize DecisionRecord")?,
    )
    .with_context(|| format!("write {}", output_path.display()))?;

    println!("recorded decision");
    println!("  provider          {}", record.provider.as_str());
    println!("  mode              {}", record.mode.as_str());
    println!("  workload          {}", record.job);
    println!("  baseline          {}", record.deterministic_baseline);
    println!("  selected          {}", record.selected_candidate);
    println!("  confidence        {:.3}", record.confidence);
    println!("  engine            {}", record.model);
    println!("  agrees baseline   {}", record.agrees_with_baseline);
    println!("  stored            {}", inserted);
    println!("  evidence          {}", output_path.display());
    Ok(())
}

fn safe_file_component(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn default_observation_path(observation: &RunObservation) -> PathBuf {
    let job = observation
        .job
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();

    PathBuf::from(format!(
        ".cishape/runs/{}-{job}.json",
        observation.observed_at_unix_ms
    ))
}

fn ensure_parent(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };

    if parent.as_os_str().is_empty() {
        return Ok(());
    }

    std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))
}

fn recommendation_for(profile: &JobShape) -> Result<Recommendation> {
    recommend(profile, &default_catalog())
        .with_context(|| format!("no feasible runner candidate for {}", profile.job))
}

fn print_observation(observation: &RunObservation, output: &Path) {
    println!();
    println!("CIShape observation");
    println!("  job               {}", observation.job);
    println!("  runner            {}", observation.runner);
    println!(
        "  duration          {:.2}s",
        observation.duration_ms as f64 / 1000.0
    );
    println!("  CPU total         {:.2}s", observation.cpu_seconds);
    println!(
        "  CPU sampled peak  {:.2} cores",
        observation.cpu_peak_millis as f64 / 1000.0
    );
    println!(
        "  memory peak       {:.0} MiB",
        observation.memory_peak_bytes as f64 / (1024.0 * 1024.0)
    );
    println!(
        "  I/O               {:.1} MiB read / {:.1} MiB written",
        observation.read_bytes as f64 / (1024.0 * 1024.0),
        observation.write_bytes as f64 / (1024.0 * 1024.0)
    );
    println!("  exit              {}", observation.exit_code);
    println!("  evidence          {}", output.display());
}

fn print_profile(profile: &JobShape) {
    println!("JobShape {}", profile.job);
    if let Some(repository) = &profile.repository {
        println!("  repository        {repository}");
    }
    println!("  runs              {}", profile.runs);
    println!("  current runner    {}", profile.current_runner);
    println!(
        "  duration p50      {:.2}s",
        profile.duration_p50_ms / 1000.0
    );
    println!(
        "  duration p95      {:.2}s",
        profile.duration_p95_ms / 1000.0
    );
    println!(
        "  CPU peak p95      {:.2} cores",
        profile.cpu_peak_p95_millis / 1000.0
    );
    println!(
        "  memory peak p99   {:.0} MiB",
        profile.memory_peak_p99_bytes / (1024.0 * 1024.0)
    );
}

fn print_recommendation(recommendation: &Recommendation) {
    println!("Recommendation");
    if let Some(repository) = &recommendation.repository {
        println!("  repository        {repository}");
    }
    println!("  current           {}", recommendation.current);
    println!("  recommended       {}", recommendation.recommended.shape);
    println!(
        "  predicted p95     {:.2}s",
        recommendation.predicted_p95_ms / 1000.0
    );
    println!(
        "  estimated cost    ${:.5} -> ${:.5} per p95 run",
        recommendation.current_estimated_cost_usd, recommendation.recommended_estimated_cost_usd
    );
    println!(
        "  estimated saving  {:.1}%",
        recommendation.cost_reduction_percent
    );
    println!("  algorithm         {}", recommendation.algorithm);
}

fn print_explanation(
    profile: &JobShape,
    recommendation: &Recommendation,
    policy: &OptimizationPolicy,
) {
    println!("Why {}?", recommendation.recommended.shape);
    println!(
        "  CPU: p95 peak {:.2} cores; candidate provides {:.1}x headroom.",
        profile.cpu_peak_p95_millis / 1000.0,
        recommendation.cpu_headroom
    );
    println!(
        "  RAM: p99 peak {:.0} MiB; candidate provides {:.1}x headroom.",
        profile.memory_peak_p99_bytes / (1024.0 * 1024.0),
        recommendation.memory_headroom
    );
    println!(
        "  Candidate capacity: {:.0} vCPU, {:.0} GiB RAM.",
        recommendation.recommended.shape.cpu_cores(),
        recommendation.recommended.shape.memory_bytes as f64 / GIB as f64
    );
    println!(
        "  Policy {} requires {:.2}x CPU and {:.2}x memory safety margins.",
        policy.policy_id, policy.cpu_safety_factor, policy.memory_safety_factor
    );
    println!(
        "  Policy applies a {:.2}x latency penalty; objective is {:?}.",
        policy.latency_penalty, policy.objective
    );
}
