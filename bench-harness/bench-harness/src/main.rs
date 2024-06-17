use std::{collections::HashMap, path::PathBuf, time::Duration};

use anyhow::Context;
use clap::Parser;
use parking_lot::lock_api::RawMutex;

use crate::{
    config::{Config, TaskConfig},
    tasks::Task,
};

mod afl;
mod config;
mod docker;
mod firecracker;
mod image_builder;
mod setup;
mod tasks;
mod utils;
mod worker;

#[derive(Copy, Clone, Debug)]
enum WorkerBackend {
    Local,
    Firecracker,
    Docker,
    Dummy,
}

impl std::fmt::Display for WorkerBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local => f.write_str("local"),
            Self::Firecracker => f.write_str("firecracker"),
            Self::Docker => f.write_str("docker"),
            Self::Dummy => f.write_str("dummy"),
        }
    }
}

impl std::str::FromStr for WorkerBackend {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "local" => Ok(WorkerBackend::Local),
            "firecracker" => Ok(WorkerBackend::Firecracker),
            "docker" => Ok(WorkerBackend::Docker),
            "dummy" => Ok(WorkerBackend::Dummy),
            _ => Err(anyhow::anyhow!("Invalid worker backend: {}", s)),
        }
    }
}

#[derive(clap::Subcommand)]
enum Command {
    /// Build any un-cached images and data.
    Build,
    /// Run an agent with the target instance name.
    Debug { instance: String },
    /// Expand an individual benchmark command.
    Expand { benchmark: String },
    /// Run a benchmark.
    Bench {
        /// Print information about the benchmark without running it.
        #[clap(long)]
        dry_run: bool,
        /// Path to benchmark configuration file.
        bench: PathBuf,
    },
    /// (Legacy) Run a benchmark.
    BenchLegacy { id: String, trials: usize, tasks: String },
    /// (Legacy) Expand the configuration specified for the target task.
    ExpandLegacy { task: String },
}

#[derive(clap::Parser)]
struct Args {
    /// Path to the file to use for config.
    #[clap(short, long, value_name = "FILE", default_value = "config.toml")]
    config: PathBuf,
    /// Number of workers to use for running benchmarks.
    #[clap(short, long, default_value_t = 1)]
    workers: usize,
    /// The backend to use for workers.
    #[clap(long, default_value_t = WorkerBackend::Firecracker)]
    backend: WorkerBackend,
    /// The subcommand to run.
    #[clap(subcommand)]
    command: Command,
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_env_var("RUST_LOG")
                .with_default_directive(tracing::Level::INFO.into())
                .from_env_lossy(),
        )
        .with_target(false)
        .init();

    init_cancellation();

    let args = Args::parse();
    if let Err(e) = run(&args) {
        eprintln!("{:?}", e);
    }
}

fn run(args: &Args) -> anyhow::Result<()> {
    let mut config: Config = config::toml_from_path(&args.config)?;

    for entry in &config.include {
        let path = match args.config.parent() {
            Some(parent) => parent.join(entry),
            None => entry.clone(),
        };
        config
            .data
            .merge(config::toml_from_path(&path)?)
            .with_context(|| format!("error loading config from {}", path.display()))?;
    }

    let mut loaded_templates = vec![];
    for (name, path) in &config.templates {
        let data = std::fs::read_to_string(path)
            .with_context(|| format!("error loading template {name} from {}", path.display()))?;
        loaded_templates.push((name, data));
    }

    let mut env = minijinja::Environment::new();
    for (name, template) in &loaded_templates {
        env.add_template(name, template)?;
    }

    fn is_containing(v: std::borrow::Cow<'_, str>, other: std::borrow::Cow<'_, str>) -> bool {
        v.contains(other.as_ref())
    }
    env.add_test("containing", is_containing);

    std::fs::create_dir_all(&config.cache.dir).with_context(|| {
        format!("error creating cache directory {}", config.cache.dir.display())
    })?;

    match &args.command {
        Command::Build => firecracker::build_images(&config),
        Command::Debug { instance } => {
            let instances = firecracker::get_instance_config(&config)?;
            let instance = instances
                .get(instance)
                .ok_or_else(|| anyhow::format_err!("Unknown instance: {instance}"))?;
            firecracker::spawn_debug_vm(instance)
        }
        Command::Bench { dry_run, bench } => run_bench_v2(args, &config, &env, *dry_run, bench),
        Command::BenchLegacy { id, trials, tasks } => run_bench(args, config, id, *trials, tasks),
        Command::ExpandLegacy { task } => {
            match config.get_task(task) {
                Ok(task) => eprintln!("{task:#?}"),
                Err(e) => eprintln!("Error expanding {task}: {e:#}"),
            }
            Ok(())
        }
        Command::Expand { benchmark } => {
            match render_tasks_template(&env, &benchmark) {
                Ok(tasks) => eprintln!("{tasks:#?}"),
                Err(e) => eprintln!("Error expanding {benchmark}: {e:#}"),
            }
            Ok(())
        }
    }
}
pub(crate) fn render_tasks_template(
    env: &minijinja::Environment,
    benchmark: &str,
) -> anyhow::Result<Vec<TaskConfig>> {
    let benchmark: Vec<crate::config::BenchGroup> = ron::from_str(benchmark)
        .with_context(|| format!("{}", StringWithLineNumbers(&benchmark)))?;
    let mut output = vec![];

    for entry in benchmark {
        let mut ctx = entry.config;
        for trial in entry.trials {
            ctx.insert("trial".into(), format!("{trial}"));

            let template = env.get_template(&entry.template)?;
            let task_str = template.render(&ctx)?;

            output.push(ron::from_str(&task_str).with_context(|| {
                format!(
                    "failed expanding template: '{}' (trial={trial})\n{}",
                    entry.template,
                    StringWithLineNumbers(&task_str)
                )
            })?);
        }
    }

    Ok(output)
}

struct StringWithLineNumbers<'a>(&'a str);

impl<'a> std::fmt::Display for StringWithLineNumbers<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, line) in self.0.lines().enumerate() {
            writeln!(f, "{:>3} | {line}", i + 1)?
        }
        Ok(())
    }
}

fn run_bench(
    args: &Args,
    mut config: Config,
    id: &str,
    trials: usize,
    task_list: &str,
) -> anyhow::Result<()> {
    let mut worker_pool = start_workers(&config, args.backend, args.workers)?;

    config.vars.push(config::KeyValue::new("BENCH_ID", id));

    for task_name in task_list
        .split(&[',', '\n'])
        .map(str::trim)
        .filter(|x| !x.is_empty() && !x.starts_with('#'))
    {
        let task = match config.get_task(task_name) {
            Ok(task) => task,
            Err(e) => {
                tracing::error!("Error running {task_name}: {e}");
                continue;
            }
        };

        for i in 0..trials {
            let mut task = task.clone();

            // Merge task specific variables with global variables. Note, the ordering matters here,
            // as we want to allow task local variables to reference globals.
            let mut vars = config.vars.clone();
            vars.push(config::KeyValue::new("TRIAL", format!("{i}")));
            vars.push(config::KeyValue::new("TASK_NAME", task_name));
            vars.extend(std::mem::take(&mut task.vars));

            worker_pool.add_task(Task {
                name: task_name.to_string(),
                instance: task.instance.clone(),
                vars,
                runable: Box::new(tasks::DynamicTask::TaskList { tasks: task.tasks.clone() }),
            })?;
        }
    }
    tracing::info!("All pending tasks started");

    worker_pool.wait_for_workers();
    tracing::info!("All tasks complete");

    Ok(())
}

fn run_bench_v2(
    args: &Args,
    config: &Config,
    env: &minijinja::Environment,
    dry_run: bool,
    benchmark: &std::path::Path,
) -> anyhow::Result<()> {
    let data = std::fs::read_to_string(&benchmark)
        .with_context(|| format!("failed to read: {}", benchmark.display()))?;
    let data = env
        .render_str(&data, &HashMap::<(), ()>::new())
        .with_context(|| format!("error rendering: {}", benchmark.display()))?;
    let task_list = render_tasks_template(env, &data)?;

    let num_workers = args.workers.min(task_list.len());
    tracing::info!(
        "{} tasks running on {num_workers} workers. Estimated time: {}",
        task_list.len(),
        utils::HumanReadableDuration(estimate_total_duration(&task_list, num_workers)),
    );

    if !dry_run {
        let mut worker_pool = start_workers(&config, args.backend, args.workers)?;

        for (i, mut task) in task_list.into_iter().enumerate() {
            let mut vars = config.vars.clone();
            vars.extend(std::mem::take(&mut task.vars));
            worker_pool.add_task(Task {
                name: format!("task-{i}"),
                instance: task.instance.clone(),
                vars,
                runable: Box::new(tasks::DynamicTask::TaskList { tasks: task.tasks }),
            })?;
        }

        tracing::info!("All pending tasks started");
        worker_pool.wait_for_workers();
        tracing::info!("All tasks complete");
    }

    Ok(())
}

fn start_workers(
    config: &Config,
    backend: WorkerBackend,
    workers: usize,
) -> anyhow::Result<worker::WorkerPool> {
    let mut worker_pool = worker::WorkerPool::new();
    match backend {
        WorkerBackend::Local => {
            let config = config
                .local_worker
                .as_ref()
                .ok_or_else(|| anyhow::format_err!("No local worker config"))?;
            for i in 0..workers {
                let mut worker = config.clone();
                worker.id = i;
                worker_pool.add_worker(move |task| worker.run_task(task))?;
            }
        }
        WorkerBackend::Firecracker => {
            let instances = std::sync::Arc::new(firecracker::get_instance_config(config)?);
            for i in 0..workers {
                let mut worker = worker::FirecrackerWorker {
                    id: format!("vm{i}-data"),
                    instances: instances.clone(),
                };
                worker_pool.add_worker(move |task| worker.run_task(task))?;
            }
        }
        WorkerBackend::Docker => {
            let instances = std::sync::Arc::new(docker::prepare_instances(config)?);
            for i in 0..workers {
                let mut worker = worker::DockerWorker {
                    id: format!("container-{i}"),
                    instances: instances.clone(),
                };
                worker_pool.add_worker(move |task| worker.run_task(task))?;
            }
        }
        WorkerBackend::Dummy => {
            for id in 0..workers {
                let mut worker = worker::DummyWorker { id };
                worker_pool.add_worker(move |task| worker.run_task(task))?;
            }
        }
    }
    tracing::info!("{workers} workers started");
    Ok(worker_pool)
}

fn estimate_total_duration(tasks: &[TaskConfig], workers: usize) -> Duration {
    let workers = workers.min(10000).max(1);

    let mut heap = std::collections::BinaryHeap::new();
    for id in 0..workers {
        heap.push(std::cmp::Reverse(Duration::from_millis(id as u64 * 100)));
    }

    let mut current_time = Duration::from_secs(0);
    for task in tasks {
        // Determine the next time a worker is free.
        let next_slot = heap.pop().unwrap();
        current_time = next_slot.0;

        // Determine the time when the current task will be complete at.
        let task_duration: Duration = task.tasks.iter().map(|x| x.estimate_duration()).sum();
        heap.push(std::cmp::Reverse(current_time + task_duration));
    }

    // Get the finish time of the last worker.
    while let Some(time) = heap.pop() {
        current_time = time.0;
    }

    current_time
}

pub trait XShellExt {
    /// Runs a command, returning stdout on success, and including stderr in the error message
    fn read_with_err(self) -> anyhow::Result<String>;

    /// Echos command to tracing
    fn trace_cmd(self) -> Self;
}

impl<'a> XShellExt for xshell::Cmd<'a> {
    fn read_with_err(self) -> anyhow::Result<String> {
        let cmd = format!("{}", self);
        let output = self.trace_cmd().ignore_status().output()?;
        match output.status.success() {
            true => Ok(String::from_utf8(output.stdout)?),
            false => {
                anyhow::bail!("`{cmd}` failed with {}", String::from_utf8_lossy(&output.stderr))
            }
        }
    }

    fn trace_cmd(mut self) -> Self {
        tracing::info!("$ {}", self);
        self.set_quiet(false);
        self
    }
}

/// Mutex for syncronizing host file system operations in workers.
pub static HOST_FS_LOCK: parking_lot::Mutex<()> =
    parking_lot::Mutex::const_new(parking_lot::RawMutex::INIT, ());

/// Global stop flag used for supporting clean exits.
static STOP_NOW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Channel used for listening for cancellation events.
static CANCELATION_RECEIVER: once_cell::sync::OnceCell<crossbeam_channel::Receiver<()>> =
    once_cell::sync::OnceCell::new();

fn init_cancellation() {
    let (cancel_tx, cancel_rx) = crossbeam_channel::bounded(0);
    CANCELATION_RECEIVER.set(cancel_rx).unwrap();
    let mut cancel_tx = Some(cancel_tx);
    ctrlc::set_handler(move || {
        STOP_NOW.store(true, std::sync::atomic::Ordering::Release);
        cancel_tx.take();
    })
    .unwrap();
}

pub(crate) fn should_stop() -> bool {
    STOP_NOW.load(std::sync::atomic::Ordering::Acquire)
}

pub(crate) fn cancellation_channel() -> &'static crossbeam_channel::Receiver<()> {
    CANCELATION_RECEIVER.get().unwrap()
}
