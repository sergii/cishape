use anyhow::{Context, Result};
use cishape::model::{GIB, JobShape, Recommendation};
use cishape::optimize::{default_catalog, recommend};
use cishape::store::Store;
use cishape::synthetic;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(name = "cishape")]
#[command(about = "Observe CI workload shapes and fit them to better runner shapes")]
struct Cli {
    #[command(subcommand)]
    command: Command,
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
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create {}", parent.display()))?;
    }

    let mut store = Store::open(path)?;
    let observations = synthetic::oversized_lint(job, runs);
    store.insert_runs(&observations)?;

    println!(
        "stored {runs} synthetic runs for {job} in {}",
        path.display()
    );
    Ok(())
}

fn recommendation_for(profile: &JobShape) -> Result<Recommendation> {
    recommend(profile, &default_catalog())
        .with_context(|| format!("no feasible runner candidate for {}", profile.job))
}

fn print_profile(profile: &JobShape) {
    println!("JobShape {}", profile.job);
    println!("  runs              {}", profile.runs);
    println!("  current runner    {}", profile.current_runner);
    println!("  duration p50      {:.2}s", profile.duration_p50_ms / 1000.0);
    println!("  duration p95      {:.2}s", profile.duration_p95_ms / 1000.0);
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
        recommendation.current_estimated_cost_usd,
        recommendation.recommended_estimated_cost_usd
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
    println!(
        "  v0 only selects candidates that preserve 1.5x CPU and memory safety margins."
    );
    println!(
        "  v0 applies a conservative 5% latency penalty instead of pretending to know workload scaling."
    );
}
