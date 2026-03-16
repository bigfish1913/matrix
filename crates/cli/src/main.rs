//! Matrix CLI - Command-line interface for the Agent Orchestrator

use clap::Parser;
use matrix_core::{Orchestrator, OrchestratorConfig};
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

    // Resolve workspace path
    let workspace = resolve_workspace(&args)?;
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
        mcp_config: args.mcp_config,
        num_agents: args.agents,
        debug_mode: args.debug,
        ask_mode: args.ask,
        resume: args.resume,
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

    let slug: String = slug.split('-').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("-");

    if slug.is_empty() {
        chrono::Local::now().format("project-%Y%m%d-%H%M%S").to_string()
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
            eprintln!("\x1b[31mError: '{}' not found. Install: {}\x1b[0m", cmd, install);
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