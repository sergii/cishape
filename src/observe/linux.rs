use crate::model::{RUN_OBSERVATION_SCHEMA_VERSION, RunObservation, RunnerShape};
use anyhow::{Context, Result, anyhow};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SAMPLE_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone)]
struct ProcStat {
    pid: u32,
    ppid: u32,
    cpu_ticks: u64,
    rss_pages: i64,
}

#[derive(Debug, Default)]
struct Accumulator {
    previous_cpu_ticks: HashMap<u32, u64>,
    io_read_max: HashMap<u32, u64>,
    io_write_max: HashMap<u32, u64>,
    last_sample_at: Option<Instant>,
    cpu_peak_millis: u32,
    memory_peak_bytes: u64,
}

pub fn command(job: &str, program: &str, args: &[String]) -> Result<RunObservation> {
    let runner = detect_runner_shape()?;
    let (provider, provider_runner) = detect_provider_metadata();
    let observed_at_unix_ms = unix_time_ms()?;
    let rusage_before = child_rusage_cpu_seconds()?;

    let started = Instant::now();
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("spawn observed command {program}"))?;

    let root_pid = child.id();
    let mut accumulator = Accumulator::default();

    let status = loop {
        sample_process_tree(root_pid, &mut accumulator);

        if let Some(status) = child.try_wait().context("poll observed command")? {
            break status;
        }

        thread::sleep(SAMPLE_INTERVAL);
    };

    sample_process_tree(root_pid, &mut accumulator);

    let duration_ms = started.elapsed().as_millis() as u64;
    let rusage_after = child_rusage_cpu_seconds()?;
    let cpu_seconds = (rusage_after - rusage_before).max(0.0);

    let exit_code = match status.code() {
        Some(code) => code,
        None => 128 + status.signal().unwrap_or(1),
    };

    Ok(RunObservation {
        schema_version: RUN_OBSERVATION_SCHEMA_VERSION,
        job: job.to_string(),
        observed_at_unix_ms,
        duration_ms,
        cpu_seconds,
        cpu_peak_millis: accumulator.cpu_peak_millis,
        memory_peak_bytes: accumulator.memory_peak_bytes,
        read_bytes: accumulator.io_read_max.values().copied().sum(),
        write_bytes: accumulator.io_write_max.values().copied().sum(),
        runner,
        provider,
        provider_runner,
        queue_ms: None,
        cost_usd: None,
        exit_code,
    })
}

fn sample_process_tree(root_pid: u32, accumulator: &mut Accumulator) {
    let Ok(all_processes) = scan_proc_stats() else {
        return;
    };

    let descendants = descendants_of(root_pid, &all_processes);
    if descendants.is_empty() {
        return;
    }

    let now = Instant::now();
    let page_size = page_size_bytes();
    let mut rss_bytes = 0_u64;
    let mut interval_cpu_ticks = 0_u64;

    for pid in descendants {
        let Some(stat) = all_processes.get(&pid) else {
            continue;
        };

        if stat.rss_pages > 0 {
            rss_bytes = rss_bytes.saturating_add(stat.rss_pages as u64 * page_size);
        }

        if accumulator.last_sample_at.is_some() {
            let previous = accumulator
                .previous_cpu_ticks
                .insert(pid, stat.cpu_ticks)
                .unwrap_or(0);
            interval_cpu_ticks =
                interval_cpu_ticks.saturating_add(stat.cpu_ticks.saturating_sub(previous));
        } else {
            accumulator.previous_cpu_ticks.insert(pid, stat.cpu_ticks);
        }

        if let Some((read_bytes, write_bytes)) = read_proc_io(pid) {
            accumulator
                .io_read_max
                .entry(pid)
                .and_modify(|value| *value = (*value).max(read_bytes))
                .or_insert(read_bytes);
            accumulator
                .io_write_max
                .entry(pid)
                .and_modify(|value| *value = (*value).max(write_bytes))
                .or_insert(write_bytes);
        }
    }

    accumulator.memory_peak_bytes = accumulator.memory_peak_bytes.max(rss_bytes);

    if let Some(previous_sample_at) = accumulator.last_sample_at {
        let elapsed = now.duration_since(previous_sample_at).as_secs_f64();
        if elapsed > 0.0 {
            let tick_hz = clock_ticks_per_second() as f64;
            let cpu_cores = interval_cpu_ticks as f64 / tick_hz / elapsed;
            let cpu_millis = (cpu_cores * 1000.0).round().clamp(0.0, u32::MAX as f64) as u32;
            accumulator.cpu_peak_millis = accumulator.cpu_peak_millis.max(cpu_millis);
        }
    }

    accumulator.last_sample_at = Some(now);
}

fn scan_proc_stats() -> Result<HashMap<u32, ProcStat>> {
    let mut result = HashMap::new();

    for entry in fs::read_dir("/proc").context("read /proc")? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };

        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        let Ok(pid) = file_name.parse::<u32>() else {
            continue;
        };

        if let Some(stat) = read_proc_stat(pid) {
            result.insert(pid, stat);
        }
    }

    Ok(result)
}

fn read_proc_stat(pid: u32) -> Option<ProcStat> {
    let content = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let close_paren = content.rfind(')')?;
    let fields: Vec<&str> = content.get(close_paren + 1..)?.split_whitespace().collect();

    let ppid = fields.get(1)?.parse().ok()?;
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    let rss_pages: i64 = fields.get(21)?.parse().ok()?;

    Some(ProcStat {
        pid,
        ppid,
        cpu_ticks: utime.saturating_add(stime),
        rss_pages,
    })
}

fn descendants_of(root_pid: u32, processes: &HashMap<u32, ProcStat>) -> HashSet<u32> {
    if !processes.contains_key(&root_pid) {
        return HashSet::new();
    }

    let mut descendants = HashSet::from([root_pid]);
    let mut changed = true;

    while changed {
        changed = false;

        for process in processes.values() {
            if descendants.contains(&process.ppid) && descendants.insert(process.pid) {
                changed = true;
            }
        }
    }

    descendants
}

fn read_proc_io(pid: u32) -> Option<(u64, u64)> {
    let content = fs::read_to_string(format!("/proc/{pid}/io")).ok()?;
    let mut read_bytes = None;
    let mut write_bytes = None;

    for line in content.lines() {
        if let Some(value) = line.strip_prefix("read_bytes:") {
            read_bytes = value.trim().parse().ok();
        } else if let Some(value) = line.strip_prefix("write_bytes:") {
            write_bytes = value.trim().parse().ok();
        }
    }

    Some((read_bytes.unwrap_or(0), write_bytes.unwrap_or(0)))
}

fn detect_runner_shape() -> Result<RunnerShape> {
    let available_cpu_millis = std::thread::available_parallelism()
        .map(|value| value.get() as u32 * 1000)
        .unwrap_or(1000);

    let cpu_millis = cgroup_cpu_limit_millis()
        .map(|limit| limit.min(available_cpu_millis))
        .unwrap_or(available_cpu_millis)
        .max(1);

    let host_memory = host_memory_bytes()?;
    let memory_bytes = cgroup_memory_limit_bytes()
        .map(|limit| limit.min(host_memory))
        .unwrap_or(host_memory)
        .max(1);

    Ok(RunnerShape::new(cpu_millis, memory_bytes))
}

fn host_memory_bytes() -> Result<u64> {
    let content = fs::read_to_string("/proc/meminfo").context("read /proc/meminfo")?;
    let line = content
        .lines()
        .find(|line| line.starts_with("MemTotal:"))
        .ok_or_else(|| anyhow!("MemTotal missing from /proc/meminfo"))?;
    let kib: u64 = line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow!("invalid MemTotal line"))?
        .parse()
        .context("parse MemTotal")?;

    Ok(kib * 1024)
}

fn cgroup_v2_dir() -> Option<PathBuf> {
    let content = fs::read_to_string("/proc/self/cgroup").ok()?;

    for line in content.lines() {
        if let Some(path) = line.strip_prefix("0::") {
            let relative = path.trim_start_matches('/');
            return Some(PathBuf::from("/sys/fs/cgroup").join(relative));
        }
    }

    None
}

fn cgroup_cpu_limit_millis() -> Option<u32> {
    let path = cgroup_v2_dir()?.join("cpu.max");
    let content = fs::read_to_string(path).ok()?;
    let mut fields = content.split_whitespace();
    let quota = fields.next()?;
    let period: u64 = fields.next()?.parse().ok()?;

    if quota == "max" || period == 0 {
        return None;
    }

    let quota: u64 = quota.parse().ok()?;
    let millis = quota.saturating_mul(1000) / period;
    Some(millis.clamp(1, u32::MAX as u64) as u32)
}

fn cgroup_memory_limit_bytes() -> Option<u64> {
    let path = cgroup_v2_dir()?.join("memory.max");
    let content = fs::read_to_string(path).ok()?;
    let value = content.trim();

    if value == "max" {
        return None;
    }

    value.parse().ok()
}

fn detect_provider_metadata() -> (Option<String>, Option<String>) {
    if matches!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true")) {
        return (
            Some("github-actions".into()),
            std::env::var("RUNNER_NAME").ok(),
        );
    }

    if matches!(std::env::var("BUILDKITE").as_deref(), Ok("true")) {
        return (
            Some("buildkite".into()),
            std::env::var("BUILDKITE_AGENT_NAME").ok(),
        );
    }

    if matches!(std::env::var("GITLAB_CI").as_deref(), Ok("true")) {
        return (
            Some("gitlab-ci".into()),
            std::env::var("CI_RUNNER_DESCRIPTION").ok(),
        );
    }

    if std::env::var_os("JENKINS_URL").is_some() {
        return (Some("jenkins".into()), std::env::var("NODE_NAME").ok());
    }

    (None, None)
}

fn unix_time_ms() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before Unix epoch")?
        .as_millis() as u64)
}

fn clock_ticks_per_second() -> u64 {
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if ticks > 0 { ticks as u64 } else { 100 }
}

fn page_size_bytes() -> u64 {
    let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if size > 0 { size as u64 } else { 4096 }
}

fn child_rusage_cpu_seconds() -> Result<f64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    let rc = unsafe { libc::getrusage(libc::RUSAGE_CHILDREN, usage.as_mut_ptr()) };

    if rc != 0 {
        return Err(std::io::Error::last_os_error()).context("getrusage(RUSAGE_CHILDREN)");
    }

    let usage = unsafe { usage.assume_init() };
    Ok(timeval_seconds(usage.ru_utime) + timeval_seconds(usage.ru_stime))
}

fn timeval_seconds(value: libc::timeval) -> f64 {
    value.tv_sec as f64 + value.tv_usec as f64 / 1_000_000.0
}
