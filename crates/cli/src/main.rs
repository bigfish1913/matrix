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
#[command(next_display_order = None)] // Keep argument order stable
struct Args {
    /// Project goal description
    #[arg(required = true)]
    goal: String,

    /// Output path (parent dir or new dir)
    #[arg(name = "PATH")]
    path: Option<PathBuf>,

    /// Specification/requirements document
    #[arg(short = 'd', long = "doc", value_name = "FILE")]
    doc: Option<PathBuf>,

    /// Explicit workspace directory
    #[arg(short = 'w', long = "workspace", value_name = "DIR")]
    workspace: Option<PathBuf>,

    /// MCP config JSON for e2e tests
    #[arg(long = "mcp-config", value_name = "FILE")]
    mcp_config: Option<PathBuf>,

    /// Resume previous run
    #[arg(short, long)]
    resume: bool,

    /// Number of parallel agent workers
    #[arg(short = 'n', long, default_value = "1")]
    agents: usize,

    /// Skip clarifying questions before planning
    #[arg(short = 'Q', long = "no-ask")]
    no_ask: bool,

    /// Language for AI prompts (default: zh, options: zh, en)
    #[arg(short = 'L', long, default_value = "zh")]
    lang: String,

    /// Disable TUI mode, use simple output
    #[arg(long)]
    no_tui: bool,

    /// Quiet mode: minimal output
    #[arg(short, long)]
    quiet: bool,

    /// Verbose mode: detailed Claude output with debug logs
    #[arg(short, long)]
    verbose: bool,

    /// Log file path (default: .matrix/matrix.log)
    #[arg(long = "log-file", value_name = "FILE")]
    log_file: Option<PathBuf>,
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
    // Parse args first to determine if TUI mode is enabled
    let args = Args::parse();
    let use_tui = !args.no_tui && std::io::stdout().is_terminal();

    // Tracing will be initialized in run_with_tui or run_simple with appropriate layers

    // Check runtime dependencies
    check_dependencies()?;

    if use_tui {
        run_with_tui(&args).await
    } else {
        run_simple(&args).await
    }
}

/// Run with TUI mode
async fn run_with_tui(args: &Args) -> anyhow::Result<()> {
    use matrix_core::tui::{
        create_event_channel, init_terminal, restore_terminal, LogBuffer, TerminalGuard,
        TuiLogLayer,
    };
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    // Resolve workspace path first to determine log file location
    let workspace = resolve_workspace(args)?;

    // Initialize terminal and wrap in guard for automatic restoration on panic/unwind
    let terminal = init_terminal()?;
    let terminal_guard = TerminalGuard::new(terminal);

    // Create event channel for orchestrator -> TUI communication
    let (event_sender, event_receiver) = create_event_channel();

    // Create event channel for TUI -> orchestrator communication (question responses)
    let (response_sender, response_receiver) = create_event_channel();

    // Setup log file: use provided path or default to .matrix/matrix.log
    let log_file_path = args
        .log_file
        .clone()
        .unwrap_or_else(|| workspace.join(".matrix").join("matrix.log"));

    // Ensure parent directory exists
    if let Some(parent) = log_file_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Create file appender
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file_path)?;

    // Initialize tracing with both TuiLogLayer and file layer
    // Use INFO level by default, or respect MATRIX_LOG env var
    let log_filter = std::env::var("MATRIX_LOG").unwrap_or_else(|_| "matrix=info".to_string());
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(log_filter))
        .with(TuiLogLayer::new(event_sender.clone()))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::sync::Mutex::new(log_file))
                .with_ansi(false),
        )
        .init();

    // Log that logging has started
    info!(log_file = %log_file_path.display(), "Logging initialized");

    // Create log buffer for shared logging
    let log_buffer = LogBuffer::new(1000);

    // Get verbosity level
    let verbosity = get_verbosity(args);

    // Create TUI app with both event receiver (from orchestrator) and response sender (to orchestrator)
    let mut app = TuiApp::new(verbosity)
        .with_event_receiver(event_receiver)
        .with_response_sender(response_sender)
        .with_log_buffer(log_buffer.clone());

    let tasks_dir = workspace.join(".matrix").join("tasks");

    // Load document content
    let doc_content = if let Some(doc_path) = &args.doc {
        if !doc_path.exists() {
            // Restore terminal before exiting
            drop(terminal_guard);
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
        debug_mode: args.verbose,
        ask_mode: !args.no_ask,
        resume: args.resume,
        event_sender: Some(event_sender),
        event_receiver: Some(response_receiver),
        language: args.lang.clone(),
    };

    // Spawn orchestrator as background task
    let orchestrator_handle = tokio::spawn(async move {
        let mut orchestrator = Orchestrator::new(config)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        orchestrator
            .run()
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    });

    // Extract terminal from guard for the event loop
    // If the loop panics, the guard will restore the terminal on drop
    let terminal = terminal_guard.into_inner();

    // Run TUI event loop
    let result = run_tui_loop(terminal, &mut app, orchestrator_handle).await;

    // Always restore terminal - run_tui_loop always returns Ok with the terminal
    match result {
        Ok((terminal, _)) => {
            if let Err(e) = restore_terminal(terminal) {
                eprintln!("Warning: Failed to restore terminal: {}", e);
            }
        }
        Err(e) => {
            // This should not happen as run_tui_loop always returns Ok
            eprintln!("TUI error: {}", e);
        }
    }

    Ok(())
}

/// Run TUI event loop
/// Returns the terminal so it can be properly restored
async fn run_tui_loop(
    mut terminal: MatrixTerminal,
    app: &mut TuiApp,
    orchestrator_handle: tokio::task::JoinHandle<anyhow::Result<()>>,
) -> anyhow::Result<(MatrixTerminal, bool)> {
    use matrix_core::tui::event_stream;
    use matrix_core::TuiEvent;
    use std::pin::pin;
    use tokio::signal;

    let events = event_stream();
    let mut events = pin!(events);
    let mut orchestrator_handle = std::pin::pin!(orchestrator_handle);
    let mut orchestrator_completed = false;

    // Initial draw
    if let Err(_) = terminal.draw(|frame| {
        render_app(frame, app);
    }) {
        return Ok((terminal, false));
    }

    while app.running {
        tokio::select! {
            // TUI keyboard/tick events
            Some(event) = events.next() => {
                match event {
                    TuiEvent::Key(_) | TuiEvent::MouseScroll { .. } => {
                        app.handle_tui_event(event);
                        // Redraw immediately on input
                        let _ = terminal.draw(|frame| {
                            render_app(frame, app);
                        });
                    }
                    TuiEvent::Tick => {
                        // Handle tick to update spinner animation
                        app.handle_tui_event(event);
                        // Poll for new orchestrator events
                        app.poll_events();
                        // Always redraw on tick to update elapsed time and animation
                        let _ = terminal.draw(|frame| {
                            render_app(frame, app);
                        });
                    }
                    _ => {}
                }
            }

            // Handle Ctrl+C signal
            _ = signal::ctrl_c() => {
                tracing::info!("Received Ctrl+C signal, shutting down");
                std::process::exit(130);
            }

            // Check if orchestrator finished (only poll if not completed)
            result = &mut orchestrator_handle, if !orchestrator_completed => {
                app.poll_events();

                match result {
                    Ok(Ok(())) => {
                        info!("Orchestrator completed successfully");
                        orchestrator_completed = true;
                        let _ = terminal.draw(|frame| {
                            render_app(frame, app);
                        });
                    }
                    Ok(Err(e)) => {
                        tracing::error!("Orchestrator failed: {}", e);
                        orchestrator_completed = true;
                        let _ = terminal.draw(|frame| {
                            render_app(frame, app);
                        });
                    }
                    Err(e) => {
                        tracing::error!("Orchestrator task panicked: {}", e);
                        orchestrator_completed = true;
                        let _ = terminal.draw(|frame| {
                            render_app(frame, app);
                        });
                    }
                }
            }
        }
    }

    // User pressed 'q' to exit - abort orchestrator if still running
    if !orchestrator_completed {
        orchestrator_handle.abort();
        let timeout = tokio::time::Duration::from_secs(3);
        let _ = tokio::time::timeout(timeout, orchestrator_handle).await;
    }

    Ok((terminal, orchestrator_completed))
}

/// Run with simple output (no TUI)
async fn run_simple(args: &Args) -> anyhow::Result<()> {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    // Resolve workspace path
    let workspace = resolve_workspace(args)?;
    let tasks_dir = workspace.join(".matrix").join("tasks");

    // Setup log file: use provided path or default to .matrix/matrix.log
    let log_file_path = args
        .log_file
        .clone()
        .unwrap_or_else(|| workspace.join(".matrix").join("matrix.log"));

    // Ensure parent directory exists
    if let Some(parent) = log_file_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Create file appender
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file_path)?;

    // Initialize tracing with both console and file output
    let log_filter = std::env::var("MATRIX_LOG").unwrap_or_else(|_| "matrix=info".to_string());
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(log_filter))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr)) // Console output
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::sync::Mutex::new(log_file))
                .with_ansi(false),
        ) // File output
        .init();

    info!(log_file = %log_file_path.display(), "Logging initialized");

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
        debug_mode: args.verbose,
        ask_mode: !args.no_ask,
        resume: args.resume,
        event_sender: None,
        event_receiver: None,
        language: args.lang.clone(),
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
    // Try to create a slug from ASCII alphanumeric characters
    let ascii_slug: String = s.chars().filter(|c| c.is_ascii_alphanumeric()).collect();

    if !ascii_slug.is_empty() {
        // Use ASCII characters if available
        let slug: String = ascii_slug
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();

        let slug: String = slug
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");

        slug.chars().take(40).collect()
    } else {
        // For non-ASCII input (like Chinese), use timestamp-based name
        chrono::Local::now()
            .format("project-%Y%m%d-%H%M%S")
            .to_string()
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
