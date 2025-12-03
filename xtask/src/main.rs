use std::{
    path::{Path, PathBuf},
    process::ExitStatus,
};

use anyhow::{Context, Result, anyhow};
use clap::{Args, Parser, Subcommand};
use futures::stream::{FuturesUnordered, StreamExt};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::oneshot,
    task::JoinHandle,
};

#[derive(Parser)]
#[command(author, version, about = "Developer tooling for Dewet")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the daemon, debug window, and Godot puppet together
    Dev(DevArgs),
}

#[derive(Args)]
struct DevArgs {
    /// Skip launching the Rust daemon
    #[arg(long)]
    no_daemon: bool,
    /// Skip launching the Tauri debug window
    #[arg(long)]
    no_debug: bool,
    /// Skip launching the Godot puppet window
    #[arg(long)]
    no_godot: bool,
    /// Override the Godot binary name or path (default: godot4)
    #[arg(long, default_value = "godot4")]
    godot_binary: String,
    /// Skip rebuilding the debug-ui bundle before booting Tauri
    #[arg(long)]
    skip_ui_build: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Dev(args) => run_dev(args).await?,
    }
    Ok(())
}

async fn run_dev(args: DevArgs) -> Result<()> {
    let root = workspace_root()?;

    if !args.skip_ui_build && !args.no_debug {
        ensure_debug_ui(&root).await?;
    }

    let mut specs = Vec::new();

    if !args.no_daemon {
        specs.push(ProcessSpec {
            name: "daemon".to_string(),
            program: "cargo".to_string(),
            args: vec![
                "run".into(),
                "-p".into(),
                "dewet-daemon".into(),
                "--features".into(),
                "native-capture".into(),
            ],
            cwd: root.clone(),
        });
    }

    if !args.no_debug {
        specs.push(ProcessSpec {
            name: "debug".to_string(),
            program: "cargo".to_string(),
            args: vec!["tauri".into(), "dev".into()],
            cwd: root.join("crates").join("dewet-debug"),
        });
    }

    if !args.no_godot {
        let godot_project = root.join("godot");
        specs.push(ProcessSpec {
            name: "godot".to_string(),
            program: args.godot_binary.clone(),
            args: vec![
                "--path".into(),
                godot_project
                    .to_str()
                    .ok_or_else(|| anyhow!("Invalid Godot path"))?
                    .to_string(),
                "--scene".into(),
                "main/Dewet.tscn".into(),
            ],
            cwd: godot_project,
        });
    }

    if specs.is_empty() {
        anyhow::bail!("nothing to run – every target was disabled");
    }

    let mut processes = Vec::with_capacity(specs.len());
    for spec in specs {
        processes.push(spawn_process(spec)?);
    }

    let mut waits: FuturesUnordered<_> = processes
        .iter_mut()
        .filter_map(|proc| proc.join.take().map(|join| (proc.name.clone(), join)))
        .map(|(name, join)| async move { (name, join.await) })
        .collect();

    let trigger = tokio::select! {
        Some((name, outcome)) = waits.next() => ExitTrigger::Process { name, outcome },
        _ = tokio::signal::ctrl_c() => ExitTrigger::CtrlC,
    };

    let mut exit_error: Option<anyhow::Error> = None;
    match trigger {
        ExitTrigger::CtrlC => {
            println!("[xtask] Ctrl+C detected, shutting everything down…");
        }
        ExitTrigger::Process { name, outcome } => {
            exit_error = handle_process_outcome(&name, outcome);
        }
    }

    for proc in &mut processes {
        proc.kill();
    }

    while let Some((name, outcome)) = waits.next().await {
        if let Some(err) = handle_process_outcome(&name, outcome) {
            exit_error.get_or_insert(err);
        }
    }

    if let Some(err) = exit_error {
        Err(err)
    } else {
        Ok(())
    }
}

async fn ensure_debug_ui(root: &Path) -> Result<()> {
    let ui_dir = root.join("debug-ui");
    println!("[xtask] Ensuring debug-ui bundle is up to date…");
    run_blocking_step("npm install", "npm", &["install"], &ui_dir).await?;
    run_blocking_step("npm run build", "npm", &["run", "build"], &ui_dir).await?;
    Ok(())
}

async fn run_blocking_step(label: &str, program: &str, args: &[&str], cwd: &Path) -> Result<()> {
    println!("[xtask] running {label} in {}", cwd.display());
    let status = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .status()
        .await
        .with_context(|| format!("failed to run {label}"))?;

    if !status.success() {
        anyhow::bail!("{label} exited with {}", format_status(&status));
    }
    Ok(())
}

struct ProcessSpec {
    name: String,
    program: String,
    args: Vec<String>,
    cwd: PathBuf,
}

struct ManagedProcess {
    name: String,
    kill: Option<oneshot::Sender<()>>,
    join: Option<JoinHandle<anyhow::Result<ExitStatus>>>,
}

impl ManagedProcess {
    fn kill(&mut self) {
        if let Some(kill) = self.kill.take() {
            let _ = kill.send(());
        }
    }
}

fn spawn_process(spec: ProcessSpec) -> Result<ManagedProcess> {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("failed to launch {}", spec.name))?;

    if let Some(stdout) = child.stdout.take() {
        spawn_pipe(spec.name.clone(), stdout, false);
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_pipe(spec.name.clone(), stderr, true);
    }

    let (kill_tx, mut kill_rx) = oneshot::channel();
    let name = spec.name.clone();
    let join = tokio::spawn(async move {
        tokio::select! {
            _ = &mut kill_rx => {
                let _ = child.start_kill();
                let status = child.wait().await?;
                Err(anyhow!("{name} terminated ({})", format_status(&status)))
            }
            status = child.wait() => {
                let status = status?;
                if status.success() {
                    Ok(status)
                } else {
                    Err(anyhow!("{name} exited with {}", format_status(&status)))
                }
            }
        }
    });

    Ok(ManagedProcess {
        name: spec.name,
        kill: Some(kill_tx),
        join: Some(join),
    })
}

fn spawn_pipe<T>(name: String, reader: T, is_err: bool)
where
    T: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if is_err {
                eprintln!("[{name}] {line}");
            } else {
                println!("[{name}] {line}");
            }
        }
    });
}

fn workspace_root() -> Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| anyhow!("unable to locate workspace root from xtask manifest"))
}

fn handle_process_outcome(
    name: &str,
    outcome: Result<anyhow::Result<ExitStatus>, tokio::task::JoinError>,
) -> Option<anyhow::Error> {
    match outcome {
        Ok(Ok(status)) => {
            println!(
                "[xtask] {name} exited with {} – stopping remaining tasks",
                format_status(&status)
            );
            None
        }
        Ok(Err(err)) => {
            eprintln!("[xtask] {name} error: {err}");
            Some(err)
        }
        Err(err) => {
            let wrapped = anyhow!("task for {name} panicked: {err}");
            eprintln!("[xtask] {wrapped}");
            Some(wrapped)
        }
    }
}

fn format_status(status: &ExitStatus) -> String {
    status
        .code()
        .map(|code| format!("code {code}"))
        .unwrap_or_else(|| "signal".to_string())
}

enum ExitTrigger {
    CtrlC,
    Process {
        name: String,
        outcome: Result<anyhow::Result<ExitStatus>, tokio::task::JoinError>,
    },
}
