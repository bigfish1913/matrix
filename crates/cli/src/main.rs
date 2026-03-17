//! Matrix CLI - Command-line interface for the Agent Orchestrator

use clap::Parser;
use futures::StreamExt;
use matrix_core::{
    render_app, MatrixTerminal, Orchestrator, OrchestratorConfig, TuiApp, VerbosityLevel,
};
use std::io::IsTerminal;
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(name = "matrix")]
#[command(author, version, about = "Long-Running Agent Orchestrator using Claude CLI", long_about = None)]
struct Args {
    /// Project goal description
    goal: String,

    /// Output path (parent dir or new dir)
    #[arg(name = "PATH")]
    path: Option<PathBuf>,

    /// Specification/requirements document
    #[arg(short, long = "doc")]
    doc: Option<PathBuf>,

    /// Explicit workspace directory
    #[arg(short = 'w', long = "workspace")]
    workspace: Option<PathBuf>,

    /// MCP config JSON for e2e tests
    #[arg(long = "mcp-config")]
    mcp_config: Option<PathBuf>,

    /// Resume previous run
    #[arg(short, long)]
    resume: bool,

    /// Number of parallel agent workers
    #[arg(short = 'n', long, default_value = "1")]
    agents: usize,

    /// Stream Claude's live output (verbose)
    #[arg(long)]
    debug: bool,

    /// Ask clarifying questions before planning
    #[arg(short, long)]
    ask: bool,

    /// Disable TUI mode, use simple output
    #[arg(long)]
    no_tui: bool,

    /// Quiet mode: minimal output
    #[arg(short, long)]
    quiet: bool,

    /// Verbose mode: detailed Claude output
    #[arg(short, long)]
    verbose: bool,
}

/// Determine verbosity level from CLI args
fn get_verbosity(args: &Args) -> VerbosityLevel {
    if args.quiet {
        VerbosityLevel::Quiet
    } else if args.verbose {
        VerbosityLevel::Verbose
    } else {
        VerbosityLevel::Normal
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "matrix=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    // Check runtime dependencies
    check_dependencies()?;

    // Determine if TUI should be used
    let use_tui = !args.no_tui && std::io::stdout().is_terminal();

    if use_tui {
        run_with_tui(&args).await
    } else {
        run_simple(&args).await
    }
}

/// Run with TUI mode
async fn run_with_tui(args: &Args) -> anyhow::Result<()> {
    use matrix_core::tui::{create_event_channel, init_terminal, restore_terminal, LogBuffer};

    // Initialize terminal
    let terminal = init_terminal()?;

    // Create event channel for orchestrator -> TUI communication
    let (event_sender, event_receiver) = create_event_channel();

    // Create log buffer for shared logging
    let log_buffer = LogBuffer::new(1000);

    // Get verbosity level
    let verbosity = get_verbosity(args);

    // Create TUI app
    let mut app = TuiApp::new(verbosity)
        .with_event_receiver(event_receiver)
        .with_log_buffer(log_buffer.clone());

    // Resolve workspace path
    let workspace = resolve_workspace(args)?;
    let tasks_dir = workspace.join(".matrix").join("tasks");

    // Load document content
    let doc_content = if let Some(doc_path) = &args.doc {
        if !doc_path.exists() {
            // Restore terminal before exiting
            let _ = restore_terminal(terminal);
            anyhow::bail!("Document not found: {}", doc_path.display());
        }
        let content = std::fs::read_to_string(doc_path)?;
        info!(lines = content.lines().count(), "Loaded document");
        Some(content)
    } else {
        None
    };

    // Create config
    let config = OrchestratorConfig {
        goal: args.goal.clone(),
        workspace,
        tasks_dir,
        doc_content,
        mcp_config: args.mcp_config.clone(),
        num_agents: args.agents,
        debug_mode: args.debug,
        ask_mode: args.ask,
        resume: args.resume,
        event_sender: Some(event_sender),
    };

    // Spawn orchestrator as background task
    let orchestrator_handle = tokio::spawn(async move {
        let mut orchestrator = Orchestrator::new(config).await.map_err(|e| anyhow::anyhow!("{}", e))?;
        orchestrator.run().await.map_err(|e| anyhow::anyhow!("{}", e))
    });

    // Run TUI event loop
    let result = run_tui_loop(terminal, &mut app, orchestrator_handle).await;

    // Restore terminal (always, even on error)
    // Note: terminal is moved back from run_tui_loop on completion
    if let Ok((terminal, _)) = result {
        let _ = restore_terminal(terminal);
    }

    Ok(())
}

/// Run TUI event loop
/// Returns the terminal so it can be properly restored
async fn run_tui_loop(
    terminal: MatrixTerminal,
    app: &mut TuiApp,
    orchestrator_handle: tokio::task::JoinHandle<anyhow::Result<()>>,
) -> anyhow::Result<(MatrixTerminal, bool)> {
    use matrix_core::tui::event_stream;
    use matrix_core::TuiEvent;
    use std::pin::pin;

    let events = event_stream();
    let mut events = pin!(events);
    let mut orchestrator_handle = std::pin::pin!(orchestrator_handle);
    let mut terminal = terminal;
    let mut orchestrator_completed = false;
    let mut last_tick = std::time::Instant::now();
    let tick_interval = std::time::Duration::from_secs(1);  // Only redraw on tick every 1s

    while app.running {
        // Poll orchestrator events (may trigger redraw via auto-follow)
        let had_events = app.poll_events_count() > 0;

        // Wait for next event
        tokio::select! {
            // TUI keyboard/tick events
            Some(event) = events.next() => {
                match event {
                    TuiEvent::Key(key) => {
                        app.handle_key(key);
                        // Redraw after keyboard input
                        terminal.draw(|frame| {
                            render_app(frame, app);
                        })?;
                    }
                    TuiEvent::Tick => {
                        // Only redraw on tick if:
                        // 1. It's been more than 1 second since last tick redraw, OR
                        // 2. We're running (need to update elapsed time)
                        let now = std::time::Instant::now();
                        let need_redraw = app.start_time.is_some() &&
                            (now.duration_since(last_tick) >= tick_interval || had_events);

                        if need_redraw {
                            terminal.draw(|frame| {
                                render_app(frame, app);
                            })?;
                            last_tick = now;
                        }
                    }
                    _ => {}
                }
            }

            // Check if orchestrator finished
            result = &mut orchestrator_handle => {
                match result {
                    Ok(Ok(())) => {
                        info!("Orchestrator completed successfully");
                        orchestrator_completed = true;
                        // Redraw final state
                        terminal.draw(|frame| {
                            render_app(frame, app);
                        })?;
                        // Don't exit immediately - let user see the results
                        // User can press 'q' to exit
                    }
                    Ok(Err(e)) => {
                        tracing::error!("Orchestrator failed: {}", e);
                        orchestrator_completed = true;
                        terminal.draw(|frame| {
                            render_app(frame, app);
                        })?;
                    }
                    Err(e) => {
                        tracing::error!("Orchestrator task panicked: {}", e);
                        orchestrator_completed = true;
                        terminal.draw(|frame| {
                            render_app(frame, app);
                        })?;
                    }
                }
            }
        }
    }

    Ok((terminal, orchestrator_completed))
}

/// Run with simple output (no TUI)
async fn run_simple(args: &Args) -> anyhow::Result<()> {
    // Resolve workspace path
    let workspace = resolve_workspace(args)?;
    let tasks_dir = workspace.join(".matrix").join("tasks");

    // Load document content
    let doc_content = if let Some(doc_path) = &args.doc {
        if !doc_path.exists() {
            anyhow::bail!("Document not found: {}", doc_path.display());
        }
        let content = std::fs::read_to_string(doc_path)?;
        info!(lines = content.lines().count(), "Loaded document");
        Some(content)
    } else {
        None
    };

    // Create config
    let config = OrchestratorConfig {
        goal: args.goal.clone(),
        workspace,
        tasks_dir,
        doc_content,
        mcp_config: args.mcp_config.clone(),
        num_agents: args.agents,
        debug_mode: args.debug,
        ask_mode: args.ask,
        resume: args.resume,
        event_sender: None,
    };

    // Run orchestrator
    let mut orchestrator = Orchestrator::new(config).await?;
    orchestrator.run().await?;

    Ok(())
}

/// Resolve workspace path
fn resolve_workspace(args: &Args) -> anyhow::Result<PathBuf> {
    if let Some(ws) = &args.workspace {
        return Ok(ws.clone());
    }

    if let Some(path) = &args.path {
        if path.is_dir() {
            // Create named subdirectory
            let slug = slugify(&args.goal);
            return Ok(path.join(slug));
        }
        return Ok(path.clone());
    }

    Ok(std::env::current_dir()?.join("workspace"))
}

/// Generate URL-friendly slug
fn slugify(s: &str) -> String {
    let slug: String = s
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();

    let slug: String = slug
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    if slug.is_empty() {
        chrono::Local::now()
            .format("project-%Y%m%d-%H%M%S")
            .to_string()
    } else {
        slug.chars().take(40).collect()
    }
}

/// Check required dependencies
fn check_dependencies() -> anyhow::Result<()> {
    let hard_deps = [
        ("claude", "npm i -g @anthropic-ai/claude-code"),
        ("git", "https://git-scm.com/downloads"),
    ];

    let soft_deps = [
        ("node", "https://nodejs.org (needed for JS/TS projects)"),
        ("npm", "https://nodejs.org (needed for JS/TS projects)"),
        ("python", "https://python.org (needed for Python projects)"),
        ("cargo", "https://rustup.rs (needed for Rust projects)"),
    ];

    // Check hard dependencies
    for (cmd, install) in &hard_deps {
        if which::which(cmd).is_err() {
            eprintln!(
                "\x1b[31mError: '{}' not found. Install: {}\x1b[0m",
                cmd, install
            );
            anyhow::bail!("Missing required dependency: {}", cmd);
        }
    }

    // Check soft dependencies
    let missing_soft: Vec<_> = soft_deps
        .iter()
        .filter(|(cmd, _)| which::which(cmd).is_err())
        .collect();

    if !missing_soft.is_empty() {
        eprintln!("\x1b[33mWarning: some optional tools are missing:\x1b[0m");
        for (cmd, install) in missing_soft {
            eprintln!("  \x1b[33m·\x1b[0m {:10}  ->  {}", cmd, install);
        }
    }

    Ok(())
}
