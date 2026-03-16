#!/usr/bin/env python3
"""
longtime.py - Long-Running Agent Orchestrator using Claude CLI
Usage: python3 longtime.py "project goal" [path] [--doc <file>] [-n <agents>] [--resume] [--ask] [--debug]
Deps:  claude (Claude Code CLI)
"""

import argparse
import json
import os
import re
import subprocess
import sys
import time
import threading
from concurrent.futures import ThreadPoolExecutor, wait as futures_wait, FIRST_COMPLETED
from datetime import datetime, timezone
from pathlib import Path

# ─── ANSI Colors ──────────────────────────────────────────────────────────────
RED    = "\033[0;31m"
GREEN  = "\033[0;32m"
YELLOW = "\033[1;33m"
BLUE   = "\033[0;34m"
CYAN   = "\033[0;36m"
BOLD   = "\033[1m"
RESET  = "\033[0m"

# ─── Config ───────────────────────────────────────────────────────────────────
SCRIPT_DIR   = Path(__file__).parent.resolve()
# MODEL_FAST   = "claude-sonnet-4-6"
# MODEL_SMART  = "claude-opus-4-6"

MODEL_FAST   = "glm-5"
MODEL_SMART  = "glm-5"
MAX_DEPTH    = 3
MAX_RETRIES  = 3
TIMEOUT_PLAN = 120    # seconds: task planning / assess / verify
TIMEOUT_EXEC = 3600   # seconds: code execution (writing files, complex tasks)

# Prompt size limits to avoid ARG_MAX errors
MAX_PROMPT_LENGTH = 80000     # Maximum total prompt size in characters
MAX_WORKSPACE_FILES = 100     # Maximum workspace files to list
MAX_COMPLETED_CONTEXT = 2000  # Maximum completed context size
MAX_MEMORY_SIZE = 3000        # Maximum memory section size
MAX_DOC_SIZE = 5000           # Maximum document content size


# ─── Logging ──────────────────────────────────────────────────────────────────
_log_file:   Path | None = None
_log_lock    = threading.Lock()   # serialise log writes across threads
_debug_mode: bool = False         # set by --debug flag; enables live Claude output preview

def log(level: str, msg: str) -> None:
    ts = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    with _log_lock:
        if _log_file:
            with open(_log_file, "a", encoding='utf-8') as f:
                f.write(f"[{ts}] [{level}] {msg}\n")
        # 使用ASCII字符替代Unicode emoji，避免Windows GBK编码问题
        icons = {"INFO": f"{BLUE}[i]{RESET} ", "OK": f"{GREEN}[+]{RESET} ",
                 "WARN": f"{YELLOW}[!]{RESET} ", "ERROR": f"{RED}[-]{RESET} ",
                 "STEP": f"{BOLD}{CYAN}[>]{RESET} "}
        print(f"{icons.get(level, '')} {msg}")



# ─── Claude Runner ────────────────────────────────────────────────────────────
def truncate_prompt_safely(prompt: str, max_length: int = None) -> str:
    """
    Truncate prompt to fit within max length while preserving critical information.

    Priority order for truncation:
    1. Reference document (least critical for immediate task)
    2. Completed context (can be summarized)
    3. Workspace files (can be reduced)
    4. Memory section (can be reduced)
    5. Task description and instructions (NEVER truncate these)
    """
    if max_length is None:
        max_length = MAX_PROMPT_LENGTH
    if len(prompt) <= max_length:
        return prompt

    # Split prompt into sections for intelligent truncation
    lines = prompt.split('\n')

    # Find key sections to preserve
    task_start = -1
    workspace_files_idx = -1
    completed_idx = -1
    doc_start = -1
    doc_end = -1
    instructions_idx = -1

    for i, line in enumerate(lines):
        if line.startswith("TASK:"):
            task_start = i
        elif line.startswith("CURRENT FILES:") or line.startswith("WORKSPACE FILES:"):
            workspace_files_idx = i
        elif line.startswith("ALREADY COMPLETED:"):
            completed_idx = i
        elif line.startswith("REFERENCE DOCUMENT") or line.startswith("PROJECT MEMORY"):
            doc_start = i
        elif line.startswith("INSTRUCTIONS:") or line.startswith("DIAGNOSIS INSTRUCTIONS:"):
            instructions_idx = i
        elif doc_start >= 0 and line.strip().startswith("─") and doc_end < 0:
            doc_end = i

    # Strategy 1: If there's a reference document, truncate it first
    if doc_start >= 0 and doc_end > doc_start:
        # Keep document header and a truncated portion
        new_lines = lines[:doc_start+1]
        new_lines.append("(Document content truncated to fit prompt size limit)")
        # Add separator before next section
        next_section_idx = next((i for i in range(doc_end+1, len(lines))
                                   if lines[i].strip() and not lines[i].startswith("─")), doc_end+1)
        new_lines.extend(lines[next_section_idx:])
        result = '\n'.join(new_lines)
        if len(result) <= max_length:
            return result

    # Strategy 2: Truncate workspace files list (keep first 30)
    if workspace_files_idx >= 0:
        # Find the end of workspace files section
        next_section = workspace_files_idx + 1
        for i in range(workspace_files_idx + 1, min(workspace_files_idx + 200, len(lines))):
            if lines[i].strip() and not lines[i].startswith(" ") and i > workspace_files_idx + 1:
                next_section = i
                break

        # Keep only first 30 files
        new_lines = list(lines[:workspace_files_idx + 1])
        file_count = 0
        for i in range(workspace_files_idx + 1, min(workspace_files_idx + 100, len(lines))):
            if file_count >= 30:
                new_lines.append(f"... ({len(lines) - workspace_files_idx - 1 - file_count} more files omitted)")
                break
            new_lines.append(lines[i])
            if not lines[i].startswith(" ") or lines[i].strip():
                file_count += 1
        new_lines.extend(lines[next_section:])
        result = '\n'.join(new_lines)
        if len(result) <= max_length:
            return result

    # Strategy 3: Truncate completed context
    if completed_idx >= 0:
        new_lines = lines[:completed_idx + 1]
        new_lines.append("(Previous tasks context truncated)")
        # Find next section after completed
        for i in range(completed_idx + 1, len(lines)):
            if lines[i].strip() and not lines[i].startswith(" ") and lines[i] not in ["(Previous tasks context truncated)"]:
                new_lines.extend(lines[i:])
                break
        result = '\n'.join(new_lines)
        if len(result) <= max_length:
            return result

    # Strategy 4: Last resort - truncate from the middle while preserving task and instructions
    if task_start >= 0 and instructions_idx >= 0:
        # Keep task description and instructions
        new_lines = lines[:task_start + 10]  # Keep task title and part of description
        new_lines.append("\n... (Content truncated to fit prompt size limit)")
        new_lines.extend(lines[instructions_idx:])  # Keep instructions
        result = '\n'.join(new_lines)
        if len(result) <= max_length:
            return result

    # Final fallback: simple truncation with buffer
    result = []
    current_len = 0
    for line in lines:
        if current_len + len(line) > max_length - 500:  # Larger buffer for safety
            result.append("\n... (Prompt truncated to fit size limit)")
            break
        result.append(line)
        current_len += len(line) + 1
    return '\n'.join(result)

def extract_json_from_text(text: str) -> dict | None:
    """Extract a JSON object from Claude's text response (may be fenced in ```json)."""
    # Try ```json ... ``` block — greedy so we get the full outer object, not the first inner }
    m = re.search(r"```json\s*(\{.*\})\s*```", text, re.DOTALL)
    if m:
        try:
            return json.loads(m.group(1))
        except json.JSONDecodeError as e:
            log("DEBUG", f"JSON parse error in ```json block: {e}")
    # Try bare JSON object — greedy to capture the outermost { ... }
    m = re.search(r"\{.*\}", text, re.DOTALL)
    if m:
        try:
            return json.loads(m.group(0))
        except json.JSONDecodeError as e:
            log("DEBUG", f"JSON parse error in bare object: {e}")
    return None


def call_claude(prompt: str, workdir: Path,
                model: str = MODEL_FAST,
                mcp_config: str | None = None,
                timeout: int = 300,
                resume_session_id: str = "") -> tuple[str, bool, str]:
    """
    Run claude CLI. Returns (result_text, is_error, session_id).
    Prompt is piped via stdin to avoid OS ARG_MAX limits on long prompts.
    In debug mode, uses stream-json+verbose to show live assistant output.
    Pass resume_session_id to continue a previous conversation session.
    """
    # In debug mode: stream-json+verbose gives real-time events we can parse live.
    # In normal mode: json gives a single quiet result line at the end.
    if _debug_mode:
        fmt_args = ["--output-format", "stream-json", "--verbose"]
    else:
        fmt_args = ["--output-format", "json"]

    cmd = [
        "claude",
        "--model", model,
        *fmt_args,
        "--dangerously-skip-permissions",
        "-p",
    ]
    if mcp_config:
        cmd += ["--mcp-config", mcp_config]
    if resume_session_id:
        cmd += ["--resume", resume_session_id]

    all_lines: list[str] = []
    result_json: dict | None = None

    try:
        proc = subprocess.Popen(
            cmd,
            cwd=str(workdir),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding='utf-8',
            errors='replace',
        )
        try:
            proc.stdin.write(prompt)
            proc.stdin.close()
        except BrokenPipeError:
            pass

        # Collect stderr silently (usually empty)
        import threading as _threading
        stderr_buf: list[str] = []
        def _read_stderr():
            for line in proc.stderr:
                stderr_buf.append(line)
        t = _threading.Thread(target=_read_stderr, daemon=True)
        t.start()

        deadline_ts = time.time() + timeout
        for raw_line in proc.stdout:
            all_lines.append(raw_line)
            if time.time() > deadline_ts:
                proc.kill()
                return "claude call timed out", True, ""

            line = raw_line.strip()
            if not line:
                continue

            # Try to parse as JSON event
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                continue

            evt_type = event.get("type", "")

            # In stream mode: print assistant text live
            if _debug_mode and evt_type == "assistant":
                for block in event.get("message", {}).get("content", []):
                    if block.get("type") == "text":
                        text = block["text"].strip()
                        if text:
                            with _log_lock:
                                print(f"  {BLUE}│{RESET} {text[:200]}", flush=True)

            # Capture result event (present in both json and stream-json modes)
            if evt_type == "result":
                result_json = event

        proc.wait()
        t.join(timeout=5)

    except FileNotFoundError:
        return "claude CLI not found", True, ""
    except Exception as e:
        return f"claude call error: {e}", True, ""

    combined = "".join(all_lines) + "".join(stderr_buf)

    # Log raw output
    if _log_file:
        with open(_log_file, "a", encoding='utf-8') as f:
            f.write(f"[claude output]\n{combined}\n")

    # Pass 1: line-by-line NDJSON scan (handles standard stream-json and json modes)
    if result_json is None:
        for raw in all_lines:
            raw = raw.strip()
            if not raw:
                continue
            try:
                obj = json.loads(raw)
                if obj.get("type") == "result" or "result" in obj:
                    result_json = obj
                    break
            except json.JSONDecodeError:
                pass

    # Pass 2: try stdout as a single JSON blob (newer CLI versions may pretty-print)
    if result_json is None:
        stdout_text = "".join(all_lines).strip()
        if stdout_text:
            try:
                obj = json.loads(stdout_text)
                if obj.get("type") == "result" or "result" in obj:
                    result_json = obj
            except json.JSONDecodeError:
                pass

    # Pass 3: regex extraction — handles JSON embedded in noise (warnings, auth prompts, etc.)
    if result_json is None:
        extracted = extract_json_from_text(combined)
        if extracted and "result" in extracted:
            result_json = extracted

    if result_json is None:
        snippet = combined[:800]
        return f"No JSON result from claude.\n{snippet}", True, ""

    is_error = result_json.get("is_error", False)
    result_text = result_json.get("result", "")
    session_id = result_json.get("session_id", "")
    return result_text, is_error, session_id


# ─── Test Runner Detection ────────────────────────────────────────────────────
def detect_test_runner(workspace: Path) -> tuple[str, list[str]] | None:
    """Detect the test framework in workspace. Returns (name, command) or None."""
    if (workspace / "go.mod").exists():
        return "go", ["go", "test", "./..."]
    if (workspace / "Cargo.toml").exists():
        return "cargo", ["cargo", "test"]
    if (workspace / "package.json").exists():
        try:
            pkg = json.loads((workspace / "package.json").read_text(encoding='utf-8'))
            if "test" in pkg.get("scripts", {}):
                return "npm", ["npm", "test", "--", "--passWithNoTests"]
        except Exception:
            pass
    for marker in ["pytest.ini", "setup.cfg", "pyproject.toml", "setup.py"]:
        if (workspace / marker).exists():
            return "pytest", [sys.executable, "-m", "pytest", "-v", "--tb=short"]
    # Loose Python: any test_*.py / *_test.py files
    if list(workspace.rglob("test_*.py")) or list(workspace.rglob("*_test.py")):
        return "pytest", [sys.executable, "-m", "pytest", "-v", "--tb=short"]
    if (workspace / "Makefile").exists():
        content = (workspace / "Makefile").read_text(encoding='utf-8')
        if re.search(r"^test\s*:", content, re.MULTILINE):
            return "make", ["make", "test"]
    return None


# ─── Task Store ───────────────────────────────────────────────────────────────
class Task:
    def __init__(self, id: str, title: str, description: str,
                 parent_id: str | None = None, depth: int = 0):
        self.id = id
        self.title = title
        self.description = description
        self.status = "pending"          # pending|in_progress|completed|failed|skipped
        self.parent_id = parent_id
        self.depth = depth
        self.complexity = "unknown"
        self.retries = 0
        self.session_id = ""
        self.result = ""
        self.error = ""
        self.test_failure_context = ""   # Last test run failure output (for retry context)
        self.test_result = ""            # Latest test run output
        self.depends_on: list[str] = []  # Task IDs this task depends on
        self.created_at = datetime.now(timezone.utc).isoformat()
        self.completed_at = ""
        self.verification_result = {}
        self.test_passed: bool = False   # True once unit-tests have passed; skip on retry
        self.modified_files: list[str] = []  # Files changed during this task's execution

    def to_dict(self) -> dict:
        return self.__dict__.copy()

    @classmethod
    def from_dict(cls, d: dict) -> "Task":
        t = cls(d["id"], d["title"], d["description"], d.get("parent_id"), d.get("depth", 0))
        for k, v in d.items():
            setattr(t, k, v)
        return t

    def save(self, tasks_dir: Path) -> None:
        path = tasks_dir / f"{self.id}.json"
        path.write_text(json.dumps(self.to_dict(), ensure_ascii=False, indent=2), encoding='utf-8')

    @staticmethod
    def load(path: Path) -> "Task":
        return Task.from_dict(json.loads(path.read_text(encoding='utf-8')))


class TaskStore:
    def __init__(self, tasks_dir: Path):
        self.tasks_dir = tasks_dir
        self.manifest_path = tasks_dir / "manifest.json"
        tasks_dir.mkdir(parents=True, exist_ok=True)

    def save_task(self, task: Task) -> None:
        task.save(self.tasks_dir)

    def all_tasks(self) -> list[Task]:
        files = sorted(self.tasks_dir.glob("task-*.json"))
        return [Task.load(f) for f in files]

    def pending_tasks(self) -> list[Task]:
        return [t for t in self.all_tasks() if t.status == "pending"]

    def validate_dependencies(self) -> list[str]:
        """
        Check for missing and circular dependencies.
        Returns a list of warning strings (empty = plan is valid).
        """
        tasks = self.all_tasks()
        task_ids = {t.id for t in tasks}
        warnings: list[str] = []

        # Check for references to non-existent task IDs
        for t in tasks:
            for dep in getattr(t, "depends_on", []):
                if dep not in task_ids:
                    warnings.append(f"[{t.id}] depends on missing task [{dep}]")

        # Detect cycles via DFS (Kahn's algorithm / colour marking)
        WHITE, GREY, BLACK = 0, 1, 2
        colour: dict[str, int] = {t.id: WHITE for t in tasks}
        dep_map = {t.id: list(getattr(t, "depends_on", [])) for t in tasks}
        cycle_found: list[str] = []

        def _dfs(tid: str, path: list[str]) -> bool:
            colour[tid] = GREY
            for dep in dep_map.get(tid, []):
                if dep not in colour:
                    continue
                if colour[dep] == GREY:
                    cycle = path + [dep]
                    cycle_found.append(" → ".join(cycle))
                    return True
                if colour[dep] == WHITE:
                    if _dfs(dep, path + [dep]):
                        return True
            colour[tid] = BLACK
            return False

        for t in tasks:
            if colour[t.id] == WHITE:
                _dfs(t.id, [t.id])

        for cycle in cycle_found:
            warnings.append(f"Circular dependency detected: {cycle}")

        return warnings

    def count(self, status: str) -> int:
        return sum(1 for t in self.all_tasks() if t.status == status)

    def total(self) -> int:
        return len(self.all_tasks())

    def save_manifest(self, goal: str) -> None:
        tasks = self.all_tasks()
        manifest = {
            "goal": goal,
            "total": len(tasks),
            "completed": self.count("completed"),
            "failed": self.count("failed"),
            "updated_at": datetime.now(timezone.utc).isoformat(),
            "tasks": [t.id for t in tasks],
        }
        self.manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2), encoding='utf-8')


# ─── Agent Pool ────────────────────────────────────────────────────────────────
class AgentPool:
    """
    Reusable Claude session pool.

    Session reuse policy (in priority order):
      1. Retry → resume the task's *own* session (full failure context).
      2. Dependency chain → inherit the most-recently-completed dependency's session
         so the agent already "knows" what the upstream task produced.
      3. Depth-0 task with *no* dependencies → always start fresh (avoids context
         bleed from an unrelated previous task on the same thread).
      4. Anything else → continue the thread's rolling session.

    Call `clear_thread()` to explicitly reset a worker's context when you know
    the next task is unrelated to everything the thread has done so far.
    """

    def __init__(self) -> None:
        self._lock = threading.Lock()
        # task_id → session_id  (recorded when execution finishes successfully)
        self._task_sessions: dict[str, str] = {}
        # thread_name → session_id  (rolling "last session" per worker thread)
        self._thread_sessions: dict[str, str] = {}

    # ── Public API ─────────────────────────────────────────────────────────────

    def get_session(self, task: "Task") -> str:
        """Return the best session_id to resume for *task*, or '' for a fresh one."""
        with self._lock:
            # 1. Retry: own session has the full failure context
            if task.retries > 0 and task.session_id:
                return task.session_id

            # 2. Dependency chain: inherit last dep's session
            for dep_id in reversed(getattr(task, "depends_on", [])):
                sid = self._task_sessions.get(dep_id, "")
                if sid:
                    return sid

            # 3. Depth-0 + no deps → fresh (prevent unrelated context bleed)
            if task.depth == 0 and not getattr(task, "depends_on", []):
                return ""

            # 4. Thread's rolling session
            return self._thread_sessions.get(threading.current_thread().name, "")

    def record(self, task: "Task", session_id: str) -> None:
        """Store *session_id* after a successful execution step."""
        if not session_id:
            return
        with self._lock:
            self._task_sessions[task.id] = session_id
            self._thread_sessions[threading.current_thread().name] = session_id

    def clear_thread(self) -> None:
        """Drop the current thread's rolling session (force fresh on next task)."""
        name = threading.current_thread().name
        with self._lock:
            self._thread_sessions.pop(name, None)

    def stats(self) -> str:
        with self._lock:
            return (f"pool: {len(self._task_sessions)} task sessions  "
                    f"| {len(self._thread_sessions)} active thread sessions")


# ─── Orchestrator ─────────────────────────────────────────────────────────────
class Orchestrator:
    def __init__(self, goal: str, workspace: Path, tasks_dir: Path,
                 doc_content: str = "", resume: bool = False,
                 mcp_config: str | None = None, num_agents: int = 1,
                 ask_mode: bool = False):
        self.goal = goal
        self.workspace = workspace
        self.store = TaskStore(tasks_dir)
        self.doc_content = doc_content
        self.resume = resume
        self.mcp_config = mcp_config   # Path to MCP config JSON for e2e tests
        self.num_agents = num_agents
        self.ask_mode = ask_mode       # --ask: interactive clarification before planning
        self._git_lock = threading.Lock()   # serialise git operations
        self._setup_done = False            # run setup_workspace only once
        self._agent_pool = AgentPool()      # reusable Claude session pool
        workspace.mkdir(parents=True, exist_ok=True)

    def _doc_section(self, header: str = "REFERENCE DOCUMENT") -> str:
        if not self.doc_content:
            return ""
        content = self.doc_content
        # Truncate document if too long
        if len(content) > MAX_DOC_SIZE:
            content = content[:MAX_DOC_SIZE] + "\n... (document truncated)"
        return f"\n{header}:\n{'─'*40}\n{content}\n{'─'*40}\n"

    def _completed_context(self) -> str:
        completed = [t for t in self.store.all_tasks() if t.status == "completed"]
        if not completed:
            return "  (none yet)"
        result = "\n".join(f"- {t.title}" for t in completed)
        # Truncate if too long
        if len(result) > MAX_COMPLETED_CONTEXT:
            result = result[:MAX_COMPLETED_CONTEXT] + "\n... (truncated)"
        return result

    def _workspace_files(self, limit: int = None) -> str:
        """
        Get workspace files with intelligent prioritization.
        Important files are listed first:
        1. Source code files (py, ts, js, go, rs, java, etc.)
        2. Config files (json, yaml, toml, xml, etc.)
        3. Documentation (md, txt, etc.)
        4. Build files (Dockerfile, Makefile, etc.)
        """
        if limit is None:
            limit = MAX_WORKSPACE_FILES

        # File extensions by priority (most important first)
        source_exts = {'.py', '.js', '.ts', '.tsx', '.jsx', '.go', '.rs', '.java', '.kt', '.cs', '.cpp', '.c', '.h'}
        config_exts = {'.json', '.yaml', '.yml', '.toml', '.xml', '.ini', '.cfg', '.conf'}
        doc_exts = {'.md', '.txt', '.rst', '.adoc'}
        build_exts = {'.dockerfile', 'Dockerfile', '.mk', 'Makefile'}

        files_with_meta = []
        for p in self.workspace.rglob("*"):
            if p.is_file() and not p.name.startswith("."):
                rel_path = str(p.relative_to(self.workspace))
                ext = Path(p.name).suffix.lower()

                # Determine priority score
                priority = 0
                if ext in source_exts:
                    priority = 4
                elif ext in config_exts:
                    priority = 3
                elif ext in doc_exts:
                    priority = 2
                elif ext in build_exts or p.name in ['Dockerfile', 'Makefile']:
                    priority = 2
                else:
                    priority = 1

                files_with_meta.append((priority, rel_path, p.stat().st_mtime))

        # Sort by priority (descending), then by modification time (descending)
        files_with_meta.sort(key=lambda x: (-x[0], -x[2]))

        # Extract just the file paths
        files = [f[1] for f in files_with_meta]

        if not files:
            return "(empty)"

        truncated = files[:limit]
        suffix = f"\n... ({len(files) - limit} more files truncated)" if len(files) > limit else ""
        return "\n".join(truncated) + suffix


    def _ws_snapshot(self) -> dict[str, float]:
        """Snapshot workspace file modification times (excludes .git). Used to track per-task changes."""
        result: dict[str, float] = {}
        for p in self.workspace.rglob("*"):
            if p.is_file() and ".git" not in p.parts:
                result[str(p.relative_to(self.workspace))] = p.stat().st_mtime
        return result

    @staticmethod
    def _snapshot_diff(before: dict[str, float], after: dict[str, float]) -> list[str]:
        """Return files that are new or have a later mtime in *after* vs *before*."""
        return [path for path, mtime in after.items()
                if path not in before or before[path] < mtime]

    @property
    def _memory_path(self) -> Path:
        return self.workspace / ".claude" / "memory.md"

    def _read_memory(self) -> str:
        """Read shared memory file."""
        if self._memory_path.exists():
            return self._memory_path.read_text(encoding='utf-8')
        return "# Project Memory (auto-updated by agents)\n\n"

    def _update_memory(self, task: Task) -> None:
        """Ask Claude to extract key learnings and append to memory."""
        log("INFO", f"Updating memory from [{task.id}] {task.title}")

        current_memory = self._read_memory()
        prompt = f"""You are a technical writer updating project documentation.

CURRENT MEMORY:
{current_memory}

JUST COMPLETED TASK:
- {task.title}: {task.description}
- Result: {task.result[:1500]}

Extract any NEW information that should be added to memory:
- New architecture decisions
- Data models / schemas
- API endpoints
- Important functions or modules
- Gotchas / warnings discovered

If nothing significant to add, respond with "SKIP".
Otherwise respond with markdown content to APPEND (no ``` fences, just raw markdown)."""

        result, is_error, _ = call_claude(prompt, self.workspace, timeout=TIMEOUT_PLAN)
        if is_error or result.strip() == "SKIP":
            return

        # Append to memory
        new_content = result.strip()
        if new_content:
            self._memory_path.parent.mkdir(parents=True, exist_ok=True)
            with open(self._memory_path, "a", encoding='utf-8') as f:
                f.write(f"\n\n---\n## From [{task.id}] {task.title}\n\n{new_content}")
            log("OK", f"Memory updated from [{task.id}]")

    def _dependency_context(self, task: Task) -> str:
        """Get results from tasks this one depends on."""
        if not task.depends_on:
            return "(no dependencies)"

        deps = []
        for dep_id in task.depends_on:
            dep_path = self.store.tasks_dir / f"{dep_id}.json"
            if dep_path.exists():
                dep_task = Task.load(dep_path)
                deps.append(f"### [{dep_id}] {dep_task.title}\n{dep_task.result[:800]}")

        return "\n\n".join(deps) if deps else "(no dependencies)"

    # ── Phase 0: Interactive Clarification ───────────────────────────────────
    def clarify_goal(self) -> str:
        """
        Ask Claude to generate targeted clarifying questions, present them
        to the user interactively, and return a formatted Q&A block that
        will be injected into the task-generation prompt.
        """
        log("STEP", "Generating clarifying questions about the project goal...")
        prompt = f"""You are helping plan a software development project.

GOAL: {self.goal}
{self._doc_section()}
Generate 3-5 concise, targeted clarifying questions whose answers would significantly
affect the technical design, architecture, or implementation plan.

Focus on: preferred tech stack, scale/performance needs, third-party integrations,
deployment environment, must-have vs. nice-to-have features, existing codebase constraints.

Respond ONLY with a JSON object (no markdown):
{{"questions": ["Question 1?", "Question 2?", "Question 3?"]}}"""

        result, is_error, _ = call_claude(prompt, self.workspace, timeout=TIMEOUT_PLAN)
        if is_error:
            log("WARN", f"Could not generate clarifying questions: {result[:200]}")
            return ""

        data = extract_json_from_text(result)
        if not data or "questions" not in data:
            log("WARN", "Could not parse clarifying questions, skipping")
            return ""

        questions = data["questions"]
        print(f"\n{BOLD}{CYAN}╔══════════════════════════════════════════════╗{RESET}")
        print(f"{BOLD}{CYAN}║  [?] Clarifying Questions                     ║{RESET}")
        print(f"{BOLD}{CYAN}╚══════════════════════════════════════════════╝{RESET}")
        print(f"  {YELLOW}Answer these to help plan your project better.{RESET}")
        print(f"  {YELLOW}Press Enter to skip any question.{RESET}\n")

        answers: list[str] = []
        for i, question in enumerate(questions, 1):
            print(f"  {BOLD}[{i}/{len(questions)}]{RESET} {question}")
            try:
                answer = input(f"  {CYAN}▶ {RESET}").strip()
            except (EOFError, KeyboardInterrupt):
                print()
                break
            if answer:
                answers.append(f"Q{i}: {question}\nA{i}: {answer}")
            print()

        if not answers:
            print(f"  {YELLOW}(no answers provided, proceeding with goal as-is){RESET}\n")
            return ""

        clarification = "\n".join(answers)
        print(f"  {GREEN}[+]{RESET} Clarifications captured — incorporating into planning...\n")
        return clarification

    # ── Topology Diagram ──────────────────────────────────────────────────────
    def print_topology(self) -> None:
        """
        Print an ASCII task dependency topology and save a Mermaid diagram.
        Tasks are grouped by dependency level (Level 0 = no deps, Level 1 = depends on L0, …).
        """
        tasks = self.store.all_tasks()
        if not tasks:
            return

        task_map = {t.id: t for t in tasks}

        # dependents[A] = list of task-IDs that depend on A
        dependents: dict[str, list[str]] = {t.id: [] for t in tasks}
        for t in tasks:
            for dep_id in getattr(t, "depends_on", []):
                if dep_id in dependents:
                    dependents[dep_id].append(t.id)

        # Compute dependency level for each task (memoised DFS)
        levels: dict[str, int] = {}

        def _level(tid: str) -> int:
            if tid in levels:
                return levels[tid]
            t = task_map.get(tid)
            if not t or not getattr(t, "depends_on", []):
                levels[tid] = 0
                return 0
            lvl = max((_level(d) for d in t.depends_on if d in task_map), default=-1) + 1
            levels[tid] = lvl
            return lvl

        for t in tasks:
            _level(t.id)

        max_level = max(levels.values(), default=0)

        # Visual helpers
        status_color = {
            "completed":   GREEN,
            "in_progress": CYAN,
            "failed":      RED,
            "pending":     YELLOW,
            "skipped":     BLUE,
        }
        status_icon = {
            "completed":   "[+]",
            "in_progress": "[*]",
            "failed":      "[-]",
            "pending":     "[ ]",
            "skipped":     "[/]",
        }

        W = 60   # column width for task name

        print(f"\n{BOLD}{CYAN}╔══════════════════════════════════════════════════════════╗{RESET}")
        print(f"{BOLD}{CYAN}║  Task Topology Graph                                     ║{RESET}")
        print(f"{BOLD}{CYAN}╚══════════════════════════════════════════════════════════╝{RESET}")
        print(f"  {GREEN}[+] completed{RESET}  {CYAN}[*] running{RESET}  "
              f"{YELLOW}[ ] pending{RESET}  {RED}[-] failed{RESET}  {BLUE}[/] skipped{RESET}\n")

        for level in range(max_level + 1):
            level_tasks = [t for t in tasks if levels.get(t.id, 0) == level]
            if not level_tasks:
                continue

            label = "no dependencies" if level == 0 else f"depends on Level {level - 1}"
            print(f"  {BOLD}── Level {level}  ({label}) {'─' * max(0, 44 - len(label))}{RESET}")

            for t in level_tasks:
                color  = status_color.get(t.status, RESET)
                icon   = status_icon.get(t.status, "?")
                title  = t.title[:52] if len(t.title) > 52 else t.title
                dep_str = ""
                if getattr(t, "depends_on", []):
                    dep_str = f"  {BLUE}◀ {', '.join(t.depends_on)}{RESET}"
                print(f"    {color}{icon}{RESET} {BOLD}[{t.id}]{RESET} {title}{dep_str}")

            # Show forward edges  (what these tasks unlock)
            for t in level_tasks:
                if dependents.get(t.id):
                    enables = ", ".join(dependents[t.id])
                    print(f"         {BLUE}└▶ enables: {enables}{RESET}")
            print()

        # Summary line
        total     = len(tasks)
        completed = sum(1 for t in tasks if t.status == "completed")
        failed    = sum(1 for t in tasks if t.status == "failed")
        pending   = sum(1 for t in tasks if t.status == "pending")
        print(f"  {BOLD}Total: {total} tasks | Levels: {max_level + 1} | "
              f"{GREEN}{completed} done{RESET} / {YELLOW}{pending} pending{RESET}"
              + (f" / {RED}{failed} failed{RESET}" if failed else "") + f"{RESET}\n")

        # ── Save Mermaid diagram ───────────────────────────────────────────────
        status_style = {
            "completed":   "fill:#22c55e,color:#fff",
            "in_progress": "fill:#06b6d4,color:#fff",
            "failed":      "fill:#ef4444,color:#fff",
            "pending":     "fill:#f59e0b,color:#fff",
            "skipped":     "fill:#6366f1,color:#fff",
        }

        mermaid: list[str] = ["```mermaid", "graph TD"]
        for t in tasks:
            safe_title = t.title.replace('"', "'")[:50]
            mermaid.append(f'    {t.id}["{t.id}: {safe_title}"]')
        for t in tasks:
            for dep_id in getattr(t, "depends_on", []):
                if dep_id in task_map:
                    mermaid.append(f"    {dep_id} --> {t.id}")
        # Style nodes by status
        by_status: dict[str, list[str]] = {}
        for t in tasks:
            by_status.setdefault(t.status, []).append(t.id)
        for status, ids in by_status.items():
            style = status_style.get(status)
            if style:
                mermaid.append(f"    style {ids[0]} {style}")   # at least colour one
                for tid in ids:
                    mermaid.append(f"    style {tid} {style}")
        mermaid.append("```")

        topology_path = (
            (_log_file.parent if _log_file else self.store.tasks_dir) / "topology.md"
        )
        topology_path.write_text(
            f"# Task Topology\n\n"
            f"Generated: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}\n\n"
            f"**Goal:** {self.goal}\n\n"
            f"**Total tasks:** {total}  |  **Levels:** {max_level + 1}\n\n"
            + "\n".join(mermaid) + "\n",
            encoding='utf-8'
        )
        log("OK", f"Topology diagram saved: {topology_path}")

    # ── Phase 1: Generate Tasks ───────────────────────────────────────────────
    def generate_tasks(self, clarification: str = "") -> None:
        log("STEP", f"Generating task list for: {self.goal}")

        clarification_section = ""
        if clarification:
            clarification_section = (
                f"\nUSER CLARIFICATIONS (incorporate these into the plan):\n"
                f"{'─' * 40}\n{clarification}\n{'─' * 40}\n"
            )

        prompt = f"""You are a software project planner. Break down the following project goal into concrete, actionable development tasks.

PROJECT GOAL: {self.goal}
{self._doc_section()}{clarification_section}
CRITICAL CONSTRAINTS (OBEY THESE OR YOUR OUTPUT WILL BE REJECTED):
- This is NOT an interactive session — DO NOT ask questions, DO NOT request clarification
- Make reasonable assumptions if anything is unclear — NEVER stop to ask
- IGNORE any existing git history, commits, or project structure you may see
- Your JSON output MUST start with "task-001" — NEVER skip to task-002 or later
- Generate the COMPLETE task list as if starting from ZERO — include all setup, initialization, configuration tasks
- DO NOT be "smart" and skip tasks that "might already be done" — your job is to generate the FULL plan
- Output ONLY valid JSON — no markdown, no explanation, no questions, NO conversational text

Rules:
- Each task should be self-contained and achievable in one focused work session
- Tasks should be ordered logically (dependencies first)
- Include setup, implementation, and testing tasks
- Each task should be small enough to complete in one focused session
- IMPORTANT: Generate at most 30 tasks in this response. If the project needs more, the system will request additional tasks later.
- If a reference document is provided, ensure tasks fully implement its requirements
- If the project has both frontend and backend, split tasks clearly: backend tasks work in a `backend/` subdirectory, frontend tasks in `frontend/`. Shared tasks (e.g. API contract, docker-compose) go in the root. Make this explicit in each task's description.

Respond ONLY with a JSON object (no markdown, no explanation, no ```json fence):
{{
  "tasks": [
    {{
      "id": "task-001",
      "title": "Short title",
      "description": "Detailed description with clear acceptance criteria",
      "depends_on": []
    }}
  ]
}}

The `depends_on` field is an array of task IDs that must complete before this task can run. Use it to declare dependencies explicitly."""

        # Use a temporary directory as workdir to prevent Claude from seeing existing git history
        # which causes it to skip already-completed tasks like task-001, task-002, etc.
        import tempfile
        with tempfile.TemporaryDirectory() as temp_dir:
            result, is_error, _ = call_claude(prompt, Path(temp_dir), timeout=TIMEOUT_EXEC)
            if is_error:
                # Try to salvage JSON even from error responses (e.g. rate-limit warnings mixed in)
                data = extract_json_from_text(result)
                if not data or "tasks" not in data:
                    log("ERROR", f"Failed to generate tasks: {result[:500]}")
                    sys.exit(1)
                log("WARN", "Recovered task list from imperfect claude response")
            else:
                data = extract_json_from_text(result)

            if not data or "tasks" not in data:
                log("ERROR", f"Invalid task list from Claude:\n{result[:500]}")
                # Save full response for debugging
                debug_path = self.store.tasks_dir / "claude_response_debug.txt"
                debug_path.write_text(result, encoding='utf-8')
                log("ERROR", f"Full Claude response saved to: {debug_path}")
                sys.exit(1)

            tasks_data = data["tasks"]
            log("OK", f"Generated {len(tasks_data)} tasks")
            for t in tasks_data:
                task = Task(t["id"], t["title"], t["description"])
                task.depends_on = t.get("depends_on", [])
                self.store.save_task(task)
                log("INFO", f"  Task: [{task.id}] {task.title}" +
                    (f" (depends: {task.depends_on})" if task.depends_on else ""))
            self.store.save_manifest(self.goal)

            # Validate dependency graph immediately after generation
            issues = self.store.validate_dependencies()
            for issue in issues:
                log("WARN", f"Task plan issue: {issue}")
            if issues:
                log("WARN", f"{len(issues)} dependency issue(s) detected — blocked tasks will be skipped")

        tasks_data = data["tasks"]
        log("OK", f"Generated {len(tasks_data)} tasks")
        for t in tasks_data:
            task = Task(t["id"], t["title"], t["description"])
            task.depends_on = t.get("depends_on", [])
            self.store.save_task(task)
            log("INFO", f"  Task: [{task.id}] {task.title}" +
                (f" (depends: {task.depends_on})" if task.depends_on else ""))
        self.store.save_manifest(self.goal)

        # Validate dependency graph immediately after generation
        issues = self.store.validate_dependencies()
        for issue in issues:
            log("WARN", f"Task plan issue: {issue}")
        if issues:
            log("WARN", f"{len(issues)} dependency issue(s) detected — blocked tasks will be skipped")

    # ── Phase 2: Assess & Split ───────────────────────────────────────────────
    def assess_and_split(self, task: Task) -> bool:
        """Returns True if task should proceed to execution, False if it was split."""
        if task.depth >= MAX_DEPTH:
            task.complexity = "simple"
            self.store.save_task(task)
            return True

        log("INFO", f"Assessing complexity: [{task.id}] {task.title}")
        prompt = f"""You are a senior software engineer evaluating a development task.

TASK: {task.title}
DESCRIPTION: {task.description}
CURRENT DEPTH: {task.depth} / {MAX_DEPTH}

ALREADY COMPLETED:
{self._completed_context()}

Is this task SIMPLE (doable in one claude session) or COMPLEX (needs splitting)?

Respond ONLY with JSON:
{{"split": false, "reason": "brief reason"}}
OR if splitting needed:
{{"split": true, "reason": "brief reason", "subtasks": [{{"title": "...", "description": "..."}}]}}"""

        result, is_error, _ = call_claude(prompt, self.workspace, timeout=TIMEOUT_PLAN)
        if is_error:
            log("WARN", f"Complexity check failed, treating as simple: {result[:200]}")
            task.complexity = "simple"
            self.store.save_task(task)
            return True

        data = extract_json_from_text(result)
        if not data:
            task.complexity = "simple"
            self.store.save_task(task)
            return True

        if data.get("split"):
            subtasks = data.get("subtasks", [])
            log("INFO", f"  Splitting [{task.id}] into {len(subtasks)} subtasks")
            for i, sub in enumerate(subtasks, 1):
                sub_id = f"{task.id}-{i}"
                subtask = Task(sub_id, sub["title"], sub["description"],
                               parent_id=task.id, depth=task.depth + 1)
                self.store.save_task(subtask)
                log("INFO", f"    Subtask: [{sub_id}] {sub['title']}")
            task.status = "skipped"
            task.complexity = "complex"
            self.store.save_task(task)
            self.store.save_manifest(self.goal)
            return False

        task.complexity = "simple"
        self.store.save_task(task)
        return True

    # ── Phase 2b: Setup Workspace ─────────────────────────────────────────────
    def setup_workspace(self) -> None:
        """
        Run once before first task execution.
        Detects project type from existing files (package.json, requirements.txt,
        go.mod, Cargo.toml, etc.) and installs dependencies.
        Skips silently if workspace is empty or already set up.
        """
        if self._setup_done:
            return
        self._setup_done = True

        files = list(self.workspace.rglob("*"))
        project_files = [f.name for f in files if f.is_file()]
        if not project_files:
            return  # Empty workspace, nothing to set up

        log("STEP", "Setting up workspace environment...")

        # Detect and run install commands directly without asking Claude
        cmds: list[tuple[str, list[str]]] = []

        if (self.workspace / "package.json").exists():
            pm = "bun" if (self.workspace / "bun.lockb").exists() else \
                 "pnpm" if (self.workspace / "pnpm-lock.yaml").exists() else \
                 "yarn" if (self.workspace / "yarn.lock").exists() else "npm"
            cmds.append((f"{pm} install", [pm, "install"]))

        if (self.workspace / "requirements.txt").exists():
            cmds.append(("pip install -r requirements.txt",
                         ["pip", "install", "-r", "requirements.txt"]))
        elif (self.workspace / "pyproject.toml").exists():
            cmds.append(("pip install -e .", ["pip", "install", "-e", "."]))

        if (self.workspace / "go.mod").exists():
            cmds.append(("go mod download", ["go", "mod", "download"]))

        if (self.workspace / "Cargo.toml").exists():
            cmds.append(("cargo fetch", ["cargo", "fetch"]))

        if (self.workspace / "Gemfile").exists():
            cmds.append(("bundle install", ["bundle", "install"]))

        if (self.workspace / "composer.json").exists():
            cmds.append(("composer install", ["composer", "install"]))

        # Also check backend/ and frontend/ subdirs
        for subdir in ["backend", "frontend", "server", "client", "api"]:
            sub = self.workspace / subdir
            if (sub / "package.json").exists():
                pm = "bun" if (sub / "bun.lockb").exists() else \
                     "pnpm" if (sub / "pnpm-lock.yaml").exists() else \
                     "yarn" if (sub / "yarn.lock").exists() else "npm"
                cmds.append((f"{pm} install ({subdir})",
                             ["sh", "-c", f"cd {sub} && {pm} install"]))
            if (sub / "requirements.txt").exists():
                cmds.append((f"pip install ({subdir})",
                             ["sh", "-c",
                              f"cd {sub} && pip install -r requirements.txt"]))
            if (sub / "go.mod").exists():
                cmds.append((f"go mod download ({subdir})",
                             ["sh", "-c", f"cd {sub} && go mod download"]))

        if not cmds:
            log("INFO", "No dependency manifests found, skipping setup")
            return

        for label, cmd in cmds:
            log("INFO", f"  Running: {label}")
            try:
                proc = subprocess.run(
                    cmd, cwd=str(self.workspace),
                    capture_output=True, text=True, encoding='utf-8', errors='replace', timeout=300,
                )
                if proc.returncode == 0:
                    log("OK", f"  {label} succeeded")
                else:
                    log("WARN", f"  {label} failed:\n{proc.stderr.strip()[:300]}")
            except subprocess.TimeoutExpired:
                log("WARN", f"  {label} timed out (300s)")
            except FileNotFoundError:
                log("WARN", f"  Command not found: {cmd[0]}")

    # ── Phase 3: Execute Task ─────────────────────────────────────────────────
    def execute_task(self, task: Task) -> bool:
        model = MODEL_SMART if task.complexity == "complex" else MODEL_FAST
        log("STEP", f"Executing [{task.id}] {task.title}  (model: {model})")

        memory_section = ""
        memory_content = self._read_memory()
        if len(memory_content) > 100:  # Has meaningful content
            # Truncate memory if too long
            if len(memory_content) > MAX_MEMORY_SIZE:
                memory_content = memory_content[:MAX_MEMORY_SIZE] + "\n... (memory truncated)"
            memory_section = f"\nPROJECT MEMORY (key decisions and patterns):\n{'─'*40}\n{memory_content}\n{'─'*40}\n"

        dep_section = ""
        dep_context = self._dependency_context(task)
        if dep_context != "(no dependencies)":
            # Limit dependency context size
            if len(dep_context) > 3000:
                dep_context = dep_context[:3000] + "\n... (dependency context truncated)"
            dep_section = f"\nDEPENDENCY CONTEXT (results from tasks this depends on):\n{'─'*40}\n{dep_context}\n{'─'*40}\n"

        prompt = f"""You are an expert software developer implementing a specific task.

TASK: {task.title}
DESCRIPTION: {task.description}
{self._doc_section("REFERENCE DOCUMENT (authoritative specification)")}
{memory_section}{dep_section}
WORKSPACE: {self.workspace}
CURRENT FILES:
{self._workspace_files()}

ALREADY COMPLETED:
{self._completed_context()}

INSTRUCTIONS:
1. Implement the task completely according to the description
2. Create all necessary files in the workspace directory
3. Production-quality code, no TODOs or placeholders
4. Ensure compatibility with previously completed work
5. Follow patterns and decisions from PROJECT MEMORY

Implement the task now. Work directly in the workspace directory."""

        # Check prompt length and warn if approaching limit
        prompt_len = len(prompt)
        if prompt_len > MAX_PROMPT_LENGTH:
            log("WARN", f"Prompt length ({prompt_len}) exceeds safe limit, truncating...")
            # Truncate workspace files first as they're often the largest
            workspace_files = self._workspace_files(limit=30)
            prompt = f"""You are an expert software developer implementing a specific task.

TASK: {task.title}
DESCRIPTION: {task.description}
{self._doc_section("REFERENCE DOCUMENT (authoritative specification)")}
{memory_section}{dep_section}
WORKSPACE: {self.workspace}
CURRENT FILES (showing 30 most important):
{workspace_files}

ALREADY COMPLETED:
{self._completed_context()}

INSTRUCTIONS:
1. Implement the task completely according to the description
2. Create all necessary files in the workspace directory
3. Production-quality code, no TODOs or placeholders
4. Ensure compatibility with previously completed work
5. Follow patterns and decisions from PROJECT MEMORY

Implement the task now. Work directly in the workspace directory."""

        task.status = "in_progress"
        self.store.save_task(task)

        # Agent pool: pick the best session to resume (or start fresh)
        resume_sid = self._agent_pool.get_session(task)
        if resume_sid:
            if task.retries > 0:
                log("INFO", f"  Resuming own session {resume_sid[:16]}…  (retry {task.retries})")
            else:
                log("INFO", f"  Reusing pooled session {resume_sid[:16]}…  (dependency context)")

        # Snapshot workspace before Claude writes files so we can identify this task's changes
        pre_snap = self._ws_snapshot()

        result, is_error, session_id = call_claude(
            prompt, self.workspace, model=model,
            timeout=TIMEOUT_EXEC, resume_session_id=resume_sid,
        )
        if is_error:
            log("ERROR", f"Execution failed [{task.id}]: {result[:300]}")
            task.status = "failed"
            task.error = result[:1000]
            self.store.save_task(task)
            return False

        # Record which files this task created/modified (for isolated git staging)
        post_snap = self._ws_snapshot()
        task.modified_files = self._snapshot_diff(pre_snap, post_snap)

        task.result = result[:2000]
        task.session_id = session_id
        self._agent_pool.record(task, session_id)   # make session available to dependents
        log("OK", f"Executed [{task.id}] {task.title}  ({self._agent_pool.stats()})")
        return True

    # ── Phase 3b: Write & Run Tests ───────────────────────────────────────────
    def test_task(self, task: Task) -> tuple[bool, str]:
        """
        Ask Claude to write tests for this task, then run them.
        Returns (passed, test_output).
        """
        log("INFO", f"Writing tests for [{task.id}] {task.title}")

        failure_hint = ""
        if task.test_failure_context:
            failure_hint = f"\nPREVIOUS TEST FAILURES (fix these):\n{task.test_failure_context[:1000]}\n"

        prompt = f"""You are a QA engineer writing tests for a completed development task.

TASK: {task.title}
DESCRIPTION: {task.description}
{failure_hint}
WORKSPACE FILES:
{self._workspace_files(limit=50)}

IMPLEMENTATION SUMMARY:
{task.result[:1000]}

INSTRUCTIONS:
1. If this task has no testable logic (e.g. config-only, docs, pure scaffolding), respond ONLY with JSON: {{"skip": true, "reason": "..."}}
2. Otherwise write tests that verify the task's acceptance criteria
3. Use the appropriate test framework for this project
4. Tests must be runnable (not pseudocode)
5. Save the test files to the workspace

Write the test files now (or respond with skip JSON if not applicable)."""

        # Truncate if needed
        prompt = truncate_prompt_safely(prompt)

        result, is_error, _ = call_claude(prompt, self.workspace, timeout=TIMEOUT_EXEC)
        if is_error:
            log("WARN", f"Test writing failed, skipping tests: {result[:200]}")
            return True, ""

        skip_data = extract_json_from_text(result)
        if skip_data and skip_data.get("skip"):
            log("INFO", f"Tests skipped [{task.id}]: {skip_data.get('reason', '')}")
            return True, ""

        # Detect and run tests
        runner = detect_test_runner(self.workspace)
        if runner is None:
            log("WARN", "No test runner detected, skipping test execution")
            return True, ""

        runner_name, runner_cmd = runner
        log("INFO", f"Running tests with {runner_name}: {' '.join(runner_cmd)}")

        # Auto-install missing test runners
        import shutil
        if runner_name == "pytest" and not shutil.which("pytest"):
            log("INFO", "pytest not found, installing via pip...")
            subprocess.run([sys.executable, "-m", "pip", "install", "pytest"],
                           capture_output=True, encoding='utf-8', errors='replace')
        elif runner_name in ("npm", "vitest", "jest") and not shutil.which("npm"):
            log("WARN", "npm not found, skipping tests")
            return True, ""

        try:
            proc = subprocess.run(
                runner_cmd,
                cwd=str(self.workspace),
                capture_output=True,
                text=True,
                encoding='utf-8',
                errors='replace',
                timeout=120,
            )
            output = (proc.stdout + proc.stderr).strip()
        except subprocess.TimeoutExpired:
            log("WARN", "Test run timed out (120s)")
            return False, "Test run timed out"
        except FileNotFoundError as e:
            log("WARN", f"Test runner not found: {e}")
            return True, ""

        task.test_result = output[:3000]
        self.store.save_task(task)

        if proc.returncode == 0:
            log("OK", f"Tests passed [{task.id}]")
            return True, output
        else:
            lines = output.splitlines()
            # Show last 20 lines of failure output
            preview = "\n".join(lines[-20:]) if len(lines) > 20 else output
            log("WARN", f"Tests FAILED [{task.id}]:\n{preview}")
            return False, output

    # ── Phase 3b-fix: Fix Test Failures ──────────────────────────────────────
    def fix_test_failure(self, task: Task, test_output: str) -> bool:
        """
        Ask Claude to diagnose test failures and fix either the implementation
        or the tests (whichever is wrong), then re-run tests.
        Returns True if tests pass after fix, False otherwise.
        """
        log("INFO", f"Attempting to fix test failures [{task.id}]")

        prompt = f"""You are a senior developer debugging failing tests.

TASK: {task.title}
DESCRIPTION: {task.description}

TEST FAILURE OUTPUT:
{test_output[:3000]}

WORKSPACE FILES:
{self._workspace_files(limit=50)}

DIAGNOSIS INSTRUCTIONS:
1. Read the failing test(s) and the implementation they test
2. Determine whether the BUG is in:
   a) The IMPLEMENTATION (code doesn't behave as described) → fix the implementation
   b) The TEST (test expects wrong behavior, or tests something not in the task) → fix the test
3. Make the minimal fix needed to make tests pass
4. Do NOT rewrite everything — only change what's broken

Fix the issue now. Work directly in the workspace directory."""

        # Truncate if needed
        prompt = truncate_prompt_safely(prompt)

        result, is_error, _ = call_claude(prompt, self.workspace,
                                          model=MODEL_SMART, timeout=TIMEOUT_EXEC)
        if is_error:
            log("WARN", f"Fix attempt failed: {result[:200]}")
            return False

        # Re-run tests to see if they pass now
        runner = detect_test_runner(self.workspace)
        if runner is None:
            return True  # Can't verify, assume fixed

        runner_name, runner_cmd = runner
        import shutil
        if runner_name == "pytest" and not shutil.which("pytest"):
            subprocess.run([sys.executable, "-m", "pip", "install", "pytest"],
                           capture_output=True, encoding='utf-8', errors='replace')
        try:
            proc = subprocess.run(runner_cmd, cwd=str(self.workspace),
                                  capture_output=True, text=True, encoding='utf-8', errors='replace', timeout=120)
        except (subprocess.TimeoutExpired, FileNotFoundError):
            return False

        stdout = proc.stdout or ""
        stderr = proc.stderr or ""
        output = (stdout + stderr).strip()
        task.test_result = output[:3000]
        self.store.save_task(task)

        if proc.returncode == 0:
            log("OK", f"Tests pass after fix [{task.id}]")
            return True
        else:
            lines = output.splitlines()
            preview = "\n".join(lines[-15:]) if len(lines) > 15 else output
            log("WARN", f"Tests still failing after fix [{task.id}]:\n{preview}")
            task.test_failure_context = output[:2000]
            self.store.save_task(task)
            return False

    # ── Phase 3c: MCP E2E Tests (browser / API) ───────────────────────────────
    def mcp_e2e_test(self, task: Task) -> tuple[bool, str]:
        """
        Use Claude + MCP tools (e.g. Playwright) to run integration/browser tests.
        Only runs when --mcp-config is provided.
        Returns (passed, output).
        """
        if not self.mcp_config:
            return True, ""

        log("INFO", f"Running MCP e2e tests for [{task.id}] {task.title}")

        failure_hint = ""
        if task.test_failure_context:
            failure_hint = f"\nPREVIOUS E2E FAILURES (fix these):\n{task.test_failure_context[:800]}\n"

        prompt = f"""You are a QA engineer running end-to-end integration tests using MCP tools.

TASK: {task.title}
DESCRIPTION: {task.description}
{failure_hint}
WORKSPACE: {self.workspace}
WORKSPACE FILES:
{self._workspace_files(limit=50)}

INSTRUCTIONS:
1. Use the available MCP tools to test the task's functionality end-to-end
2. For web applications: use Playwright browser tools to navigate and interact with the app
3. For APIs: use HTTP tools to send requests and verify responses
4. Verify the acceptance criteria from the task description are met
5. If the app needs to be started first, do so (e.g. npm run dev, python app.py)

After testing, respond with a JSON summary:
{{"passed": true, "summary": "what was tested and results"}}
OR:
{{"passed": false, "failures": ["failure 1", "failure 2"], "summary": "what failed"}}"""

        # Truncate if needed
        prompt = truncate_prompt_safely(prompt)

        result, is_error, _ = call_claude(
            prompt, self.workspace,
            model=MODEL_SMART,
            mcp_config=self.mcp_config,
            timeout=TIMEOUT_EXEC,
        )

        if is_error:
            log("WARN", f"MCP e2e test call failed, skipping: {result[:200]}")
            return True, ""

        data = extract_json_from_text(result)
        if not data:
            log("WARN", "Could not parse MCP test result, assuming pass")
            return True, result

        if data.get("passed"):
            log("OK", f"MCP e2e tests passed [{task.id}]: {data.get('summary', '')}")
            return True, result
        else:
            failures = "\n".join(data.get("failures", []))
            log("WARN", f"MCP e2e tests FAILED [{task.id}]:\n{failures}")
            return False, result

    # ── Phase 4: Verify Task ──────────────────────────────────────────────────
    def verify_task(self, task: Task) -> bool:
        log("INFO", f"Verifying [{task.id}] {task.title}")

        prompt = f"""You are a code reviewer verifying a completed development task.

TASK: {task.title}
DESCRIPTION: {task.description}

WORKSPACE FILES:
{self._workspace_files(limit=50)}

EXECUTION SUMMARY:
{task.result[:1500]}

Was this task FULLY and CORRECTLY completed?
- All required files exist
- Implementation matches acceptance criteria
- No obvious errors or incomplete code

Respond ONLY with JSON:
{{"verified": true, "summary": "what was implemented"}}
OR:
{{"verified": false, "issues": ["issue1", "issue2"], "summary": "what needs fixing"}}"""

        # Truncate if needed
        prompt = truncate_prompt_safely(prompt)

        result, is_error, _ = call_claude(prompt, self.workspace, timeout=TIMEOUT_PLAN)
        if is_error:
            log("WARN", f"Verification call failed, assuming pass")
            task.status = "completed"
            task.completed_at = datetime.now(timezone.utc).isoformat()
            self.store.save_task(task)
            return True

        data = extract_json_from_text(result)
        if not data:
            log("WARN", "Could not parse verification result, assuming pass")
            task.status = "completed"
            task.completed_at = datetime.now(timezone.utc).isoformat()
            self.store.save_task(task)
            return True

        task.verification_result = data
        if data.get("verified"):
            task.status = "completed"
            task.completed_at = datetime.now(timezone.utc).isoformat()
            self.store.save_task(task)
            log("OK", f"Verified [{task.id}]: {data.get('summary', '')}")
            return True
        else:
            issues = ", ".join(data.get("issues", []))
            log("WARN", f"Verification failed [{task.id}]: {issues}")
            self.store.save_task(task)
            return False

    # ── Phase 5: Git Commit ───────────────────────────────────────────────────
    def _ensure_gitignore(self) -> None:
        """Create or update .gitignore in workspace. Safe to call multiple times."""
        gitignore = self.workspace / ".gitignore"
        GITIGNORE_CONTENT = """\
# ── Node / JS / TS ──────────────────────────────────────────────────────────
node_modules/
npm-debug.log*
yarn-debug.log*
yarn-error.log*
pnpm-debug.log*
.pnpm-store/
.npm/
.yarn/
.pnp.*
.next/
.nuxt/
.output/
.svelte-kit/
dist/
out/
build/
.cache/
.parcel-cache/
.turbo/
.vercel/
.netlify/
storybook-static/

# ── Python ───────────────────────────────────────────────────────────────────
__pycache__/
*.py[cod]
*$py.class
*.so
*.pyd
.Python
venv/
.venv/
env/
ENV/
pip-log.txt
pip-delete-this-directory.txt
*.egg-info/
*.egg
.eggs/
dist/
build/
.mypy_cache/
.pytype/
.pyre/
.ruff_cache/
.pytest_cache/
.tox/
htmlcov/
.coverage
.coverage.*
coverage.xml
coverage/

# ── Java / Kotlin / Gradle / Maven ──────────────────────────────────────────
*.class
*.jar
*.war
*.ear
*.nar
target/
.gradle/
gradle-app.setting
!gradle-wrapper.jar
.gradletasknamecache
*.iml

# ── Go ───────────────────────────────────────────────────────────────────────
*.exe
*.exe~
*.dll
*.test
*.out
vendor/

# ── Rust ─────────────────────────────────────────────────────────────────────
target/
**/*.rs.bk

# ── Ruby ─────────────────────────────────────────────────────────────────────
*.gem
*.rbc
/.config
/coverage/
/InstalledFiles
/pkg/
/spec/reports/
/test/tmp/
/test/version_tmp/
/tmp/
.bundle/
vendor/bundle/

# ── PHP ──────────────────────────────────────────────────────────────────────
vendor/
composer.lock
.phpunit.result.cache

# ── C / C++ ──────────────────────────────────────────────────────────────────
*.o
*.a
*.lib
*.app
*.dSYM/
CMakeFiles/
CMakeCache.txt
cmake_install.cmake

# ── Swift / Xcode ────────────────────────────────────────────────────────────
*.xcodeproj/
*.xcworkspace/
DerivedData/
*.moved-aside
*.pbxuser
*.mode1v3
*.mode2v3
*.perspectivev3
xcuserdata/
*.xccheckout
*.xcscmblueprint
.build/

# ── Docker ───────────────────────────────────────────────────────────────────
.dockerignore

# ── Databases ────────────────────────────────────────────────────────────────
*.sqlite
*.sqlite3
*.db
*.db-shm
*.db-wal
dump.rdb

# ── Secrets / Env ────────────────────────────────────────────────────────────
.env
.env.*
!.env.example
!.env.sample
*.pem
*.key
*.p12
*.pfx
secrets.json
secrets.yaml
secrets.yml

# ── Logs ─────────────────────────────────────────────────────────────────────
*.log
logs/
*.log.*

# ── OS ───────────────────────────────────────────────────────────────────────
.DS_Store
.DS_Store?
._*
.Spotlight-V100
.Trashes
ehthumbs.db
Thumbs.db
desktop.ini

# ── IDE ──────────────────────────────────────────────────────────────────────
.vscode/
.idea/
*.swp
*.swo
*~
*.bak
.project
.classpath
.settings/
nbproject/

# ── Test coverage ────────────────────────────────────────────────────────────
.nyc_output/
lcov.info
*.lcov

# ── Misc ─────────────────────────────────────────────────────────────────────
*.tmp
*.temp
.sass-cache/
.terraform/
*.tfstate
*.tfstate.*

# ── Claude / longtime metadata ───────────────────────────────────────────────
.claude/
tasks/
logs/
workspace/
"""
        if gitignore.exists():
            existing = gitignore.read_text(encoding='utf-8')
            missing = [
                line for line in GITIGNORE_CONTENT.splitlines()
                if line and not line.startswith("#") and line not in existing
            ]
            if missing:
                with open(gitignore, "a", encoding='utf-8') as f:
                    f.write("\n# (auto-added by longtime)\n")
                    f.write("\n".join(missing) + "\n")
                log("INFO", f".gitignore updated ({len(missing)} entries added)")
        else:
            gitignore.write_text(GITIGNORE_CONTENT, encoding='utf-8')
            log("INFO", ".gitignore created in workspace")

    def _init_git_repo(self) -> None:
        """Initialize git repo with .gitignore."""
        # Before creating a nested .git, protect the parent git repo by ensuring
        # the workspace directory is listed in the parent's .gitignore.
        parent_top = subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            cwd=str(self.workspace.parent if self.workspace.parent.exists() else self.workspace),
            capture_output=True, text=True, encoding='utf-8', errors='replace',
        )
        if parent_top.returncode == 0:
            parent_root = Path(parent_top.stdout.strip())
            try:
                rel_ws = self.workspace.relative_to(parent_root)
                entry = f"/{rel_ws}/"
                parent_gitignore = parent_root / ".gitignore"
                if parent_gitignore.exists():
                    existing = parent_gitignore.read_text(encoding='utf-8')
                else:
                    existing = ""
                if entry not in existing:
                    with open(parent_gitignore, "a", encoding='utf-8') as f:
                        f.write(f"\n# longtime workspace (nested git repo — do not track)\n{entry}\n")
                    log("INFO", f"Added {entry} to parent .gitignore to hide nested repo")
            except ValueError:
                pass  # workspace is not inside parent_root

        subprocess.run(["git", "init"], cwd=str(self.workspace), capture_output=True, encoding='utf-8', errors='replace')
        subprocess.run(["git", "config", "user.email", "longtime@agent"],
                       cwd=str(self.workspace), capture_output=True, encoding='utf-8', errors='replace')
        subprocess.run(["git", "config", "user.name", "Longtime Agent"],
                       cwd=str(self.workspace), capture_output=True, encoding='utf-8', errors='replace')

        self._ensure_gitignore()

        # Also ensure SCRIPT_DIR-level .gitignore excludes longtime's own generated dirs
        root_gitignore = SCRIPT_DIR / ".gitignore"
        root_entry = "tasks/\nlogs/\nworkspace/\n*.log\n"
        if not root_gitignore.exists():
            root_gitignore.write_text(root_entry, encoding='utf-8')
        elif root_entry.strip() not in root_gitignore.read_text(encoding='utf-8'):
            with open(root_gitignore, "a", encoding='utf-8') as f:
                f.write("\n# longtime generated\ntasks/\nlogs/\nworkspace/\n")

        # Remove any already-tracked files that should now be ignored
        # (runs once at repo init — cheap since there are usually no commits yet)
        subprocess.run(
            ["git", "rm", "-r", "--cached", "--ignore-unmatch", "."],
            cwd=str(self.workspace), capture_output=True, encoding='utf-8', errors='replace',
        )

        log("INFO", "Initialized git repository in workspace")

    def _get_main_branch(self) -> str:
        """Return the current branch name (used as the integration branch)."""
        r = subprocess.run(
            ["git", "rev-parse", "--abbrev-ref", "HEAD"],
            cwd=str(self.workspace),
            capture_output=True, text=True, encoding='utf-8', errors='replace',
        )
        branch = r.stdout.strip()
        return branch if branch and branch != "HEAD" else "main"

    def _git(self, *args: str) -> subprocess.CompletedProcess:
        """Run a git command in the workspace, capturing output."""
        return subprocess.run(
            ["git", *args],
            cwd=str(self.workspace),
            capture_output=True, text=True, encoding='utf-8', errors='replace',
        )

    def git_commit_task(self, task: Task) -> None:
        """
        Per-task branch + squash merge:
          1. Create agent/{task.id} branch at current HEAD
          2. Stage + commit only this task's files on that branch
          3. Switch back to main, squash-merge the task branch (one clean commit)
          4. Delete the task branch

        _git_lock is held throughout, so parallel agents never race on git state.
        Other agents' untracked working-tree files are unaffected by the checkout
        because git only touches tracked files when switching branches.
        """
        import shutil
        if not shutil.which("git"):
            log("WARN", "git not found, skipping commit")
            return

        with self._git_lock:
            # Init repo if needed
            if not (self.workspace / ".git").exists():
                self._init_git_repo()

            msg = f"[{task.id}] {task.title}"

            # ── Stage this task's files ───────────────────────────────────────
            files = getattr(task, "modified_files", [])
            if files:
                for f in files:
                    if (self.workspace / f).exists():
                        self._git("add", "--", f)
                    else:
                        self._git("rm", "--cached", "--ignore-unmatch", "--", f)
            else:
                self._git("add", "-A")   # fallback: agents=1 or no snapshot

            # Nothing staged → nothing to do
            if self._git("diff", "--cached", "--quiet").returncode == 0:
                log("INFO", f"No changes to commit for [{task.id}]")
                return

            # ── Decide strategy based on whether any commits exist ────────────
            has_commits = self._git("rev-parse", "--verify", "HEAD").returncode == 0

            if not has_commits:
                # Empty repo: can't branch before the first commit — commit directly.
                proc = self._git("commit", "-m", msg)
                if proc.returncode != 0:
                    log("WARN", f"git commit failed [{task.id}]: {(proc.stderr or '').strip()[:200]}")
                else:
                    log("OK", f"Committed (initial): {msg}")
                return

            # ── Branch + squash merge ─────────────────────────────────────────
            main_branch  = self._get_main_branch()
            task_branch  = f"agent/{task.id}"

            # 1. Create task branch at current HEAD (no working-tree changes)
            self._git("checkout", "-b", task_branch)

            # 2. Commit staged files on the task branch
            proc = self._git("commit", "-m", msg)
            if proc.returncode != 0:
                log("WARN", f"git commit on task branch failed [{task.id}]: {(proc.stderr or '').strip()[:200]}")
                self._git("checkout", main_branch)
                self._git("branch", "-D", task_branch)
                return

            # 3. Return to main branch (other agents' untracked files stay intact)
            self._git("checkout", main_branch)

            # 4. Squash merge: stage task branch diff as a single pending commit
            merge = self._git("merge", "--squash", task_branch)
            if merge.returncode != 0:
                log("WARN", f"Squash merge failed [{task.id}]: {(merge.stderr or '').strip()[:200]}")
                self._git("branch", "-D", task_branch)
                return

            # 5. Create the squash commit on main
            proc = self._git("commit", "-m", msg)
            if proc.returncode != 0:
                log("WARN", f"Squash commit failed [{task.id}]: {(proc.stderr or '').strip()[:200]}")
            else:
                log("OK", f"Committed (squash merge): {msg}")

            # 6. Clean up task branch
            self._git("branch", "-D", task_branch)

    # ── Phase 5: Final Tests ──────────────────────────────────────────────────
    def run_final_tests(self) -> None:
        log("STEP", "Running final comprehensive tests...")
        completed = [t for t in self.store.all_tasks() if t.status == "completed"]
        completed_titles = "\n".join(f"- {t.title}" for t in completed) or "(none)"

        prompt = f"""You are a QA engineer performing final acceptance testing.

ORIGINAL GOAL: {self.goal}

COMPLETED TASKS:
{completed_titles}

WORKSPACE FILES:
{self._workspace_files()}

INSTRUCTIONS:
1. Review all files in the workspace
2. Verify the project meets the original goal
3. Run any tests that exist (npm test, pytest, go test, etc.)
4. Check for bugs, missing features, or incomplete implementations
5. Provide a final comprehensive test report

Perform the final testing now."""

        result, _, _ = call_claude(prompt, self.workspace, model=MODEL_SMART, timeout=TIMEOUT_EXEC)

        print(f"\n{BOLD}{GREEN}{'═'*45}{RESET}")
        print(f"{BOLD}{GREEN}  FINAL TEST REPORT{RESET}")
        print(f"{BOLD}{GREEN}{'═'*45}{RESET}\n")
        print(result)

        report_path = _log_file.parent / "final-test-report.md" if _log_file else self.workspace / "test-report.md"
        report_path.write_text(result, encoding='utf-8')
        log("OK", f"Report saved: {report_path}")

    # ── Final Commit ──────────────────────────────────────────────────────────
    def _final_commit(self, completed: int, total: int) -> None:
        """Commit any remaining changes (e.g. test report) after all tasks finish."""
        import shutil
        if not shutil.which("git"):
            return
        if not (self.workspace / ".git").exists():
            return

        with self._git_lock:
            subprocess.run(["git", "add", "-A"], cwd=str(self.workspace), capture_output=True, encoding='utf-8', errors='replace')

            # Check if there's anything new to commit
            result = subprocess.run(
                ["git", "diff", "--cached", "--quiet"],
                cwd=str(self.workspace), capture_output=True, encoding='utf-8', errors='replace',
            )
            if result.returncode == 0:
                log("INFO", "Final commit: nothing new to commit")
                return

            elapsed = int(time.time() - self._start_time)
            h, rem = divmod(elapsed, 3600)
            m, s   = divmod(rem, 60)
            duration = f"{h}h{m:02d}m{s:02d}s" if h else f"{m}m{s:02d}s"

            msg = f"chore: all tasks complete ({completed}/{total} tasks, {duration})"
            proc = subprocess.run(
                ["git", "commit", "-m", msg],
                cwd=str(self.workspace), capture_output=True, text=True, encoding='utf-8', errors='replace',
            )
            if proc.returncode == 0:
                log("OK", f"Final commit: {msg}")
            else:
                stderr = proc.stderr or ""
                log("WARN", f"Final commit failed: {stderr.strip()[:200]}")

    # ── Print Progress ────────────────────────────────────────────────────────
    def print_progress(self) -> None:
        total     = self.store.total()
        completed = self.store.count("completed")
        pending   = self.store.count("pending")
        failed    = self.store.count("failed")
        running   = self.store.count("in_progress")
        skipped   = self.store.count("skipped")
        done      = completed + failed + skipped   # tasks that won't run again

        # ── Elapsed / ETA ──────────────────────────────────────────────────
        elapsed = time.time() - self._start_time
        def _fmt(secs: float) -> str:
            secs = int(secs)
            h, rem = divmod(secs, 3600)
            m, s   = divmod(rem, 60)
            return f"{h}h{m:02d}m{s:02d}s" if h else f"{m}m{s:02d}s"

        elapsed_str = _fmt(elapsed)
        if done > 0 and done < total:
            eta_secs = elapsed / done * (total - done)
            eta_str  = f"~{_fmt(eta_secs)}"
        elif done >= total:
            eta_str  = "done"
        else:
            eta_str  = "calculating…"

        print(f"\n{CYAN}Progress: {GREEN}{completed} completed{RESET} | "
              f"{YELLOW}{pending} pending{RESET} | "
              f"{BLUE}{running} running{RESET} | "
              f"{RED}{failed} failed{RESET} / {total} total  "
              f"[elapsed {BOLD}{elapsed_str}{RESET}  ETA {BOLD}{eta_str}{RESET}]\n")

    # ── Single-task pipeline (runs in a thread) ───────────────────────────────
    def _run_task_pipeline(self, task_id: str) -> None:
        """
        Full pipeline for one task: assess → execute → test → mcp_e2e → verify → commit.
        Designed to be called from a ThreadPoolExecutor worker.
        If a step fails and retries remain, marks the task pending and returns
        (the main loop will re-dispatch it).
        """
        task = Task.load(self.store.tasks_dir / f"{task_id}.json")

        # Step 1: Assess / split
        if not self.assess_and_split(task):
            return  # Task was split; subtasks will be dispatched by the main loop

        task = Task.load(self.store.tasks_dir / f"{task_id}.json")

        # Step 1b: Setup workspace (runs once, installs dependencies)
        self.setup_workspace()

        # Step 2: Execute
        if not self.execute_task(task):
            task = Task.load(self.store.tasks_dir / f"{task_id}.json")
            if task.retries < MAX_RETRIES:
                task.retries += 1
                task.status = "pending"
                self.store.save_task(task)
                log("WARN", f"Retrying [{task.id}] (attempt {task.retries}/{MAX_RETRIES})")
            else:
                task.status = "failed"
                self.store.save_task(task)
                log("ERROR", f"[{task.id}] permanently failed")
            return

        task = Task.load(self.store.tasks_dir / f"{task_id}.json")

        # Step 3a: Unit tests — skip entirely if a previous attempt already passed
        if task.test_passed:
            log("INFO", f"Tests already passed [{task.id}] — skipping re-test [+]")
            tests_passed, test_output = True, task.test_result
        else:
            tests_passed, test_output = self.test_task(task)
            if tests_passed and test_output:
                # Tests actually ran (non-empty output) and passed → cache the result
                task = Task.load(self.store.tasks_dir / f"{task_id}.json")
                task.test_passed = True
                self.store.save_task(task)

        if not tests_passed:
            task = Task.load(self.store.tasks_dir / f"{task_id}.json")
            # First try a targeted fix rather than full re-execution
            if self.fix_test_failure(task, test_output):
                tests_passed = True  # Fix worked, continue pipeline
                task = Task.load(self.store.tasks_dir / f"{task_id}.json")
                task.test_passed = True
                self.store.save_task(task)
            elif task.retries < MAX_RETRIES:
                task.retries += 1
                task.status = "pending"
                task.test_failure_context = test_output[:2000]
                self.store.save_task(task)
                log("WARN", f"Tests failed, re-executing [{task.id}] (attempt {task.retries}/{MAX_RETRIES})")
                return
            else:
                task.status = "failed"
                task.error = f"Tests failed after {MAX_RETRIES} attempts:\n{test_output[:1000]}"
                self.store.save_task(task)
                log("ERROR", f"[{task.id}] tests failed permanently")
                return

        task = Task.load(self.store.tasks_dir / f"{task_id}.json")

        # Step 3b: MCP e2e tests (only when --mcp-config set)
        mcp_passed, mcp_output = self.mcp_e2e_test(task)
        if not mcp_passed:
            task = Task.load(self.store.tasks_dir / f"{task_id}.json")
            if task.retries < MAX_RETRIES:
                task.retries += 1
                task.status = "pending"
                task.test_failure_context = mcp_output[:2000]
                self.store.save_task(task)
                log("WARN", f"MCP e2e failed, re-executing [{task.id}] (attempt {task.retries}/{MAX_RETRIES})")
            else:
                task.status = "failed"
                task.error = f"MCP e2e tests failed:\n{mcp_output[:1000]}"
                self.store.save_task(task)
                log("ERROR", f"[{task.id}] MCP e2e tests failed permanently")
            return

        task = Task.load(self.store.tasks_dir / f"{task_id}.json")

        # Step 4: Verify
        if not self.verify_task(task):
            task = Task.load(self.store.tasks_dir / f"{task_id}.json")
            if task.retries < MAX_RETRIES:
                task.retries += 1
                task.status = "pending"
                self.store.save_task(task)
                log("WARN", f"Re-executing [{task.id}] (attempt {task.retries}/{MAX_RETRIES})")
            else:
                task.status = "failed"
                self.store.save_task(task)
                log("ERROR", f"[{task.id}] failed verification after {MAX_RETRIES} attempts")
            return

        # Step 5: Git commit (lock held inside git_commit_task)
        task = Task.load(self.store.tasks_dir / f"{task_id}.json")
        self.git_commit_task(task)

        # Step 6: Update shared memory
        self._update_memory(task)

    # ── Main Loop ─────────────────────────────────────────────────────────────
    def run(self) -> None:
        self._start_time = time.time()
        print(f"\n{BOLD}{CYAN}╔══════════════════════════════════════════╗{RESET}")
        print(f"{BOLD}{CYAN}║     LONGTIME - Agent Orchestrator        ║{RESET}")
        print(f"{BOLD}{CYAN}╚══════════════════════════════════════════╝{RESET}\n")
        print(f"{BOLD}Goal:{RESET}      {self.goal}")
        print(f"{BOLD}Workspace:{RESET} {self.workspace}")
        print(f"{BOLD}Agents:{RESET}    {self.num_agents}")
        if self.ask_mode:
            print(f"{BOLD}Mode:{RESET}      {CYAN}interactive (--ask){RESET}")
        print()

        # Ensure .gitignore exists in workspace before any work begins
        self._ensure_gitignore()

        # Phase 0: Interactive clarification (only on fresh run with --ask)
        clarification = ""
        if self.ask_mode and not self.resume:
            clarification = self.clarify_goal()

        # Phase 1: Generate tasks (or resume)
        # Auto-detect existing tasks even when --resume wasn't explicitly passed
        if not self.resume and self.store.total() > 0:
            total = self.store.total()
            done  = sum(1 for t in self.store.all_tasks() if t.status == "completed")
            pending = total - done
            print()
            print(f"  {YELLOW}[!]{RESET} Found {total} existing tasks ({done} completed, {pending} pending).")
            try:
                answer = input("     Resume previous run? [Y/n] ").strip().lower()
            except EOFError:
                answer = "n"
            except KeyboardInterrupt:
                print()
                sys.exit(0)
            print()
            if answer in ("", "y", "yes"):
                self.resume = True
            else:
                # User chose to start fresh — clear existing tasks
                log("INFO", "Clearing existing tasks, regenerating...")
                for t in self.store.all_tasks():
                    task_file = self.store.tasks_dir / f"{t.id}.json"
                    if task_file.exists():
                        task_file.unlink()

        if self.resume and self.store.total() > 0:
            # Count task statuses
            completed = [t for t in self.store.all_tasks() if t.status == "completed"]
            failed = [t for t in self.store.all_tasks() if t.status == "failed"]
            pending = [t for t in self.store.all_tasks() if t.status == "pending"]
            in_progress = [t for t in self.store.all_tasks() if t.status == "in_progress"]

            log("INFO", f"Resuming with {self.store.total()} existing tasks")
            print(f"     {GREEN}Completed:{RESET} {len(completed)}")
            print(f"     {RED}Failed:{RESET}    {len(failed)}")
            print(f"     {YELLOW}Pending:{RESET}    {len(pending)}")
            print(f"     {CYAN}In Progress:{RESET} {len(in_progress)}")
            print()

            # Reset any tasks stuck at in_progress from a crashed run
            for t in in_progress:
                t.status = "pending"
                self.store.save_task(t)
                log("INFO", f"  Reset stuck task to pending: [{t.id}]")

            # Ask user if they want to retry failed tasks
            if failed:
                print(f"  {YELLOW}[!]{RESET} Found {len(failed)} failed task(s):")
                for t in failed[:5]:  # Show first 5
                    print(f"       - [{t.id}] {t.title[:50]}")
                if len(failed) > 5:
                    print(f"       ... and {len(failed) - 5} more")
                print()
                try:
                    answer = input("     Retry failed tasks? [Y/n] ").strip().lower()
                except EOFError:
                    answer = "n"
                except KeyboardInterrupt:
                    print()
                    sys.exit(0)
                print()
                if answer in ("", "y", "yes"):
                    for t in failed:
                        t.status = "pending"
                        t.retries = 0
                        t.error = ""
                        self.store.save_task(t)
                        log("INFO", f"  Reset failed task to pending: [{t.id}]")
                else:
                    log("INFO", f"  Keeping {len(failed)} failed task(s) in failed state")
        else:
            self.generate_tasks(clarification=clarification)

        # Show topology after task generation / resume load
        self.print_topology()

        self.print_progress()

        # ── Tiered dispatch ────────────────────────────────────────────────────
        # Primary tasks (depth=0) get their own reserved slots so subtasks
        # (depth>0, spawned by splits) can never starve top-level work.
        #   primary_slots  = ceil(num_agents / 2)  → at least 1
        #   subtask_slots  = num_agents - primary_slots  (may be 0 when agents=1)
        # When agents=1 there is only one pool and tasks run in depth order.
        import math
        primary_slots  = max(1, math.ceil(self.num_agents / 2))
        subtask_slots  = max(1, self.num_agents - primary_slots)
        total_slots    = primary_slots + subtask_slots

        # Parallel main loop
        dispatched: dict = {}           # future → task_id
        dispatched_ids: set[str] = set()
        primary_running = 0             # depth-0 tasks currently in flight
        subtask_running = 0             # depth>0 tasks currently in flight
        deadline = time.time() + 24 * 3600

        with ThreadPoolExecutor(max_workers=total_slots) as executor:
            while True:
                if time.time() > deadline:
                    log("WARN", "Safety limit reached (24h). Stopping.")
                    break

                # --- Collect completed futures ---
                done = {f for f in dispatched if f.done()}
                for f in done:
                    tid = dispatched.pop(f)
                    dispatched_ids.discard(tid)
                    # Determine tier of finished task to update counters
                    try:
                        finished = Task.load(self.store.tasks_dir / f"{tid}.json")
                        if finished.depth == 0:
                            primary_running -= 1
                        else:
                            subtask_running -= 1
                    except Exception:
                        primary_running = max(0, primary_running - 1)
                    try:
                        f.result()
                    except Exception as e:
                        log("ERROR", f"Pipeline exception [{tid}]: {e}")

                if done:
                    self.store.save_manifest(self.goal)
                    self.print_progress()

                # --- Dispatch pending tasks respecting tier limits ---
                completed_ids = {t.id for t in self.store.all_tasks()
                                 if t.status in ("completed", "skipped")}
                pending = [
                    t for t in self.store.pending_tasks()
                    if t.id not in dispatched_ids
                    and all(dep in completed_ids for dep in getattr(t, "depends_on", []))
                ]
                # Split pending into tiers; depth=0 first, then deeper tasks
                primary_pending = [t for t in pending if t.depth == 0]
                subtask_pending = [t for t in pending if t.depth > 0]

                for task in primary_pending:
                    if primary_running >= primary_slots:
                        break
                    task.status = "in_progress"
                    self.store.save_task(task)
                    f = executor.submit(self._run_task_pipeline, task.id)
                    dispatched[f] = task.id
                    dispatched_ids.add(task.id)
                    primary_running += 1
                    log("INFO", f"[primary] Dispatched [{task.id}] {task.title}")

                for task in subtask_pending:
                    if subtask_running >= subtask_slots:
                        break
                    task.status = "in_progress"
                    self.store.save_task(task)
                    f = executor.submit(self._run_task_pipeline, task.id)
                    dispatched[f] = task.id
                    dispatched_ids.add(task.id)
                    subtask_running += 1
                    log("INFO", f"[subtask] Dispatched [{task.id}] {task.title}")

                # --- Exit conditions ---
                if not dispatched and not self.store.pending_tasks():
                    log("INFO", "No more pending tasks. Loop complete.")
                    break
                if not dispatched and not pending and self.store.pending_tasks():
                    log("WARN", "Remaining tasks are blocked by unresolvable dependencies.")
                    break

                if dispatched:
                    futures_wait(list(dispatched.keys()),
                                 timeout=2, return_when=FIRST_COMPLETED)

        # Summary
        completed = self.store.count("completed")
        failed    = self.store.count("failed")
        total     = self.store.total()
        print(f"\n{BOLD}{CYAN}{'═'*45}{RESET}")
        print(f"{BOLD}All tasks processed: {completed}/{total} completed, {failed} failed{RESET}")
        print(f"{BOLD}{CYAN}{'═'*45}{RESET}\n")

        if completed > 0:
            self.run_final_tests()
            self._final_commit(completed, total)
        else:
            log("WARN", "No tasks completed, skipping final tests.")


# ─── Entry Point ──────────────────────────────────────────────────────────────
def resolve_workspace(path_str: str, goal: str) -> Path:
    """If path exists as a dir → create named subdir inside. Otherwise use directly."""
    path = Path(path_str).resolve()
    if path.is_dir():
        # Slug from goal (ASCII only; fallback to timestamp for non-ASCII)
        slug = re.sub(r"[^a-z0-9]+", "-", goal.lower()).strip("-")[:40]
        if not slug:
            slug = f"project-{datetime.now().strftime('%Y%m%d-%H%M%S')}"
        return path / slug
    return path


def main() -> None:
    global _log_file

    import shutil

    # ── Runtime dependency check ───────────────────────────────────────────────
    # Hard requirements: must exist or we abort
    hard = {
        "claude": "npm i -g @anthropic-ai/claude-code",
        "git":    "https://git-scm.com/downloads",
    }
    # Soft requirements: warn if missing (only needed when that feature is used)
    soft = {
        "node":   "https://nodejs.org  (needed for JS/TS projects)",
        "npm":    "https://nodejs.org  (needed for JS/TS projects)",
        "python3":"https://python.org  (needed for Python projects)",
        "pip":    "https://pip.pypa.io (needed for Python projects)",
        "go":     "https://go.dev      (needed for Go projects)",
        "cargo":  "https://rustup.rs   (needed for Rust projects)",
        "pytest": "pip install pytest  (needed to run Python tests)",
        "vitest": "npm i -g vitest     (needed to run Vite/TS tests)",
        "jest":   "npm i -g jest       (needed to run Jest tests)",
    }

    missing_hard = [(cmd, fix) for cmd, fix in hard.items() if not shutil.which(cmd)]
    if missing_hard:
        for cmd, fix in missing_hard:
            print(f"{RED}Error: '{cmd}' not found.  Install: {fix}{RESET}")
        sys.exit(1)

    missing_soft = [(cmd, fix) for cmd, fix in soft.items() if not shutil.which(cmd)]
    if missing_soft:
        print(f"{YELLOW}Warning: some optional tools are missing (install if needed):{RESET}")
        for cmd, fix in missing_soft:
            print(f"  {YELLOW}·{RESET} {cmd:10s}  →  {fix}")

    parser = argparse.ArgumentParser(
        prog="longtime.py",
        description="Long-Running Agent Orchestrator using Claude CLI",
    )
    parser.add_argument("goal", help="Project goal")
    parser.add_argument("path", nargs="?", help="Output path (parent dir or new dir)")
    parser.add_argument("--doc", metavar="FILE", help="Specification/requirements document")
    parser.add_argument("-d", "--workspace", metavar="DIR", help="Explicit workspace dir")
    parser.add_argument("--mcp-config", metavar="FILE",
                        help="MCP config JSON for e2e tests (Playwright, HTTP, etc.)")
    parser.add_argument("--resume", action="store_true", help="Resume previous run")
    parser.add_argument("-n", "--agents", metavar="N", type=int, default=1,
                        help="Number of parallel agent workers (default: 1)")
    parser.add_argument("--debug", action="store_true",
                        help="Stream Claude's live output to console (verbose)")
    parser.add_argument("--ask", "-q", action="store_true",
                        help="Ask clarifying questions before generating the task plan")
    args = parser.parse_args()

    global _debug_mode
    _debug_mode = args.debug

    # Resolve workspace
    if args.workspace:
        workspace = Path(args.workspace).resolve()
    elif args.path:
        workspace = resolve_workspace(args.path, args.goal)
    else:
        workspace = SCRIPT_DIR / "workspace"

    # Directories
    tasks_dir = SCRIPT_DIR / "tasks"
    logs_dir  = SCRIPT_DIR / "logs"
    logs_dir.mkdir(parents=True, exist_ok=True)
    tasks_dir.mkdir(parents=True, exist_ok=True)

    _log_file = logs_dir / f"run-{datetime.now().strftime('%Y%m%d-%H%M%S')}.log"

    log("INFO", f"Workspace: {workspace}")
    log("INFO", f"Tasks:     {tasks_dir}")
    log("INFO", f"Log:       {_log_file}")

    # Load doc
    doc_content = ""
    if args.doc:
        doc_path = Path(args.doc)
        if not doc_path.exists():
            print(f"{RED}Error: Document not found: {args.doc}{RESET}")
            sys.exit(1)
        doc_content = doc_path.read_text(encoding='utf-8')
        lines = doc_content.count("\n") + 1
        log("INFO", f"Loaded document: {args.doc} ({lines} lines)")

    # Run
    orch = Orchestrator(
        goal=args.goal,
        workspace=workspace,
        tasks_dir=tasks_dir,
        doc_content=doc_content,
        resume=args.resume,
        mcp_config=args.mcp_config,
        num_agents=args.agents,
        ask_mode=args.ask,
    )
    orch.run()


if __name__ == "__main__":
    main()
