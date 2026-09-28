use anyhow::{Context, Result};
use cishape::interchange;
use cishape::model::{GIB, JobShape, Recommendation, RunObservation};
use cishape::observe;
use cishape::optimize::{default_catalog, recommend};
use cishape::store::Store;
use cishape::synthetic;
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
    /// Build a historical JobShape from stored runs.
    Profile {
        #[arg(long, default_value = ".cishape/cishape.duckdb")]
        db: PathBuf,
        job: String,
    },
    /// Recommend the cheapest runner shape that satisfies v0 safety constraints.
    Recommend {
        #[arg(long, default_value = ".cishape/cishape.duckdb")]
        db: PathBuf,
        job: String,
    },
    /// Explain the deterministic recommendation and its safety margins.
    Explain {
        #[arg(long, default_value = ".cishape/cishape.duckdb")]
        db: PathBuf,
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
        Command::Export { db, format, output } => {
            export_command(&db, format, output.as_deref())
        }
        Command::Profile { db, job } => {
            let store = Store::open(&db)?;
            print_profile(&store.profile(&job)?);
            Ok(())
        }
        Command::Recommend { db, job } => {
            let store = Store::open(&db)?;
            let profile = store.profile(&job)?;
            let recommendation = recommendation_for(&profile)?;
            print_recommendation(&recommendation);
            Ok(())
        }
        Command::Explain { db, job } => {
            let store = Store::open(&db)?;
            let profile = store.profile(&job)?;
            let recommendation = recommendation_for(&profile)?;
            print_explanation(&profile, &recommendation);
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
    print_explanation(&profile, &recommendation);

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

fn print_explanation(profile: &JobShape, recommendation: &Recommendation) {
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
    println!("  v0 only selects candidates that preserve 1.5x CPU and memory safety margins.");
    println!(
        "  v0 applies a conservative 5% latency penalty instead of pretending to know workload scaling."
    );
}
