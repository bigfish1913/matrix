# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Purpose

Create a Rust 1:1 replication of `docs/longtime.py` — a Long-Running Agent Orchestrator that uses Claude CLI to autonomously develop software projects.

## Requirements

From `docs/require1.0.md`:
1. Parameter naming must follow Rust conventions (snake_case)
2. Full feature coverage of longtime.py functionality
3. **Workspace architecture**: Use Rust workspace structure for future cli, Claude Code plugin, gui, and web clients
4. **Taskfile**: One-click installation and global deployment

## Architecture Overview

The Python source (`docs/longtime.py`) is a ~2500-line script implementing an autonomous AI agent orchestration system. Key components:

### Core Data Structures

| Component | Purpose |
|-----------|---------|
| `Task` | Single development task with status, dependencies, retries, results |
| `TaskStore` | Task persistence (JSON files) and dependency graph validation |
| `AgentPool` | Claude session reuse for context continuity across tasks |
| `Orchestrator` | Main orchestration engine coordinating all phases |

### Execution Pipeline

1. **Phase 0** (optional): Interactive clarification (`--ask` flag)
2. **Phase 1**: Task generation via Claude
3. **Phase 2**: Assess complexity and split complex tasks recursively
4. **Phase 3**: Execute, run tests, auto-fix failures, verify
5. **Phase 4**: Git commit with branch isolation
6. **Phase 5**: Final tests and summary

### Key Features to Implement

- **Claude CLI subprocess calls**: Pipe prompts via stdin, parse JSON from stdout
- **Multi-language project detection**: package.json, Cargo.toml, go.mod, requirements.txt, etc.
- **Test runner detection**: pytest, npm test, cargo test, go test, make test
- **Thread-safe task management**: Dependency graph with cycle detection
- **Parallel execution**: ThreadPoolExecutor pattern with tiered dispatch (primary tasks vs subtasks)
- **Git operations**: init, branch, commit, squash merge
- **Workspace snapshots**: Track file modifications per task via mtime
- **Shared memory system**: Cross-agent knowledge transfer via `.claude/memory.md`
- **Topology visualization**: ASCII diagrams + Mermaid export
- **ANSI terminal colors**: Cross-platform output formatting

## Project Structure

Rust workspace architecture for multiple clients:
```
matrix/
├── Cargo.toml          # Workspace root
├── crates/
│   ├── core/           # Shared orchestration logic
│   ├── cli/            # Command-line interface
│   ├── plugin/         # Claude Code plugin
│   ├── gui/            # GUI client (future)
│   └── web/            # Web client (future)
├── Taskfile.yml        # Build & deployment automation
└── docs/
    └── longtime.py     # Source to replicate
```

## Development Commands

Using Taskfile for automation:

```bash
task install          # One-click install and global deployment
task build            # Build all workspace crates
task test             # Run all tests
task run              # Run CLI with default args
task test -- <test>   # Run specific test
```

Direct cargo commands:

```bash
cargo build --workspace           # Build all crates
cargo test --workspace            # Run all tests
cargo run -p matrix-cli           # Run CLI crate
cargo test -p matrix-core <test>  # Run specific test in core
```

## CLI Interface (from longtime.py)

```
Usage: longtime <goal> [path] [OPTIONS]

Arguments:
  <goal>              Project goal description
  [path]              Output path (parent dir or new dir)

Options:
  --doc <FILE>        Specification/requirements document
  -d, --workspace     Explicit workspace directory
  --mcp-config <FILE> MCP config JSON for e2e tests
  --resume            Resume previous run
  -n, --agents <N>    Number of parallel agents (default: 1)
  --debug             Stream Claude's live output
  --ask, -q           Ask clarifying questions before planning
```

## Configuration Constants

From the Python source:
- `MODEL_FAST` / `MODEL_SMART`: Default model (currently "glm-5")
- `MAX_DEPTH`: 3 (maximum task split depth)
- `MAX_RETRIES`: 3 (retry attempts per task)
- `TIMEOUT_PLAN`: 120s (planning/assess/verify operations)
- `TIMEOUT_EXEC`: 3600s (code execution)
- `MAX_PROMPT_LENGTH`: 80000 chars

## Dependency Detection Patterns

The source detects project types by checking for marker files:

| Project Type | Marker Files | Test Command |
|--------------|--------------|--------------|
| Go | go.mod | `go test ./...` |
| Rust | Cargo.toml | `cargo test` |
| Node.js | package.json (with test script) | `npm test` |
| Python | pytest.ini, setup.cfg, pyproject.toml, setup.py, test_*.py | `pytest -v` |
| Makefile | Makefile (with test target) | `make test` |