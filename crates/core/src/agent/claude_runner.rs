//! Claude CLI runner - handles subprocess calls to claude.

use crate::config::Model;
use crate::config::MAX_PROMPT_LENGTH;
use crate::error::{Error, Result};
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tracing::{debug, info, warn};

/// Result from a Claude CLI call
#[derive(Debug, Clone)]
pub struct ClaudeResult {
    pub text: String,
    pub is_error: bool,
    pub session_id: Option<String>,
}

/// Claude CLI runner
#[derive(Debug, Clone)]
pub struct ClaudeRunner {
    model: String,
    debug_mode: bool,
}

impl ClaudeRunner {
    /// Create a new ClaudeRunner with default model
    pub fn new() -> Self {
        Self {
            model: Model::default_fast().to_string(),
            debug_mode: false,
        }
    }

    /// Set the model
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Set the model from Model enum
    pub fn with_model_enum(mut self, model: Model) -> Self {
        self.model = model.to_string();
        self
    }

    /// Enable debug mode
    pub fn with_debug(mut self, debug: bool) -> Self {
        self.debug_mode = debug;
        self
    }

    /// Call Claude CLI with a prompt
    pub async fn call(
        &self,
        prompt: &str,
        workdir: &Path,
        timeout_secs: Option<u64>,
        mcp_config: Option<&Path>,
        resume_session_id: Option<&str>,
    ) -> Result<ClaudeResult> {
        let timeout_duration = Duration::from_secs(timeout_secs.unwrap_or(120));

        // Truncate prompt if too long
        let prompt = if prompt.len() > MAX_PROMPT_LENGTH {
            warn!(
                len = prompt.len(),
                max = MAX_PROMPT_LENGTH,
                "Prompt truncated"
            );
            truncate_prompt_safely(prompt, MAX_PROMPT_LENGTH)
        } else {
            prompt.to_string()
        };

        if self.debug_mode {
            // Use streaming mode for real-time output
            self.call_streaming(&prompt, workdir, timeout_duration, mcp_config, resume_session_id).await
        } else {
            // Use batch mode
            self.call_batch(&prompt, workdir, timeout_duration, mcp_config, resume_session_id).await
        }
    }

    /// Batch mode - wait for completion
    async fn call_batch(
        &self,
        prompt: &str,
        workdir: &Path,
        timeout_duration: Duration,
        mcp_config: Option<&Path>,
        resume_session_id: Option<&str>,
    ) -> Result<ClaudeResult> {
        let mut cmd = Command::new("claude");
        cmd.args(["--model", &self.model])
            .args(["--output-format", "json"])
            .arg("--dangerously-skip-permissions")
            .arg("-p")
            .current_dir(workdir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(mcp) = mcp_config {
            if let Some(mcp_str) = mcp.to_str() {
                cmd.args(["--mcp-config", mcp_str]);
            }
        }

        if let Some(sid) = resume_session_id {
            cmd.args(["--resume", sid]);
        }

        debug!(model = %self.model, "Calling Claude CLI (batch mode)");

        let result = timeout(timeout_duration, async {
            let mut child = cmd
                .kill_on_drop(true) // Kill child when future is dropped
                .spawn()
                .map_err(|e| Error::ClaudeCli(format!("Failed to spawn: {}", e)))?;

            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(prompt.as_bytes())
                    .await
                    .map_err(|e| Error::ClaudeUi(format!("Failed to write stdin: {}", e)))?;
                stdin
                    .shutdown()
                    .await
                    .map_err(|e| Error::ClaudeCli(format!("Failed to close stdin: {}", e)))?;
            }

            let output = child
                .wait_with_output()
                .await
                .map_err(|e| Error::ClaudeCli(format!("Failed to wait: {}", e)))?;

            Ok::<_, Error>(output)
        })
        .await
        .map_err(|_| Error::Timeout("Claude call timed out".to_string()))??;

        let stdout = String::from_utf8_lossy(&result.stdout);
        let stderr = String::from_utf8_lossy(&result.stderr);
        let combined = format!("{}{}", stdout, stderr);

        // Log the response for TUI display
        info!(target: "claude", "[Claude Response] {}", &combined[..combined.len().min(500)]);

        parse_claude_result(&combined)
    }

    /// Streaming mode - real-time output
    async fn call_streaming(
        &self,
        prompt: &str,
        workdir: &Path,
        timeout_duration: Duration,
        mcp_config: Option<&Path>,
        resume_session_id: Option<&str>,
    ) -> Result<ClaudeResult> {
        let mut cmd = Command::new("claude");
        cmd.args(["--model", &self.model])
            .args(["--output-format", "stream-json"])
            .arg("--verbose")
            .arg("--dangerously-skip-permissions")
            .arg("-p")
            .current_dir(workdir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(mcp) = mcp_config {
            if let Some(mcp_str) = mcp.to_str() {
                cmd.args(["--mcp-config", mcp_str]);
            }
        }

        if let Some(sid) = resume_session_id {
            cmd.args(["--resume", sid]);
        }

        info!(model = %self.model, "Calling Claude CLI (streaming mode)");

        let result = timeout(timeout_duration, async {
            let mut child = cmd
                .kill_on_drop(true) // Kill child when future is dropped
                .spawn()
                .map_err(|e| Error::ClaudeCli(format!("Failed to spawn: {}", e)))?;

            // Write prompt to stdin
            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(prompt.as_bytes())
                    .await
                    .map_err(|e| Error::ClaudeCli(format!("Failed to write stdin: {}", e)))?;
                stdin
                    .shutdown()
                    .await
                    .map_err(|e| Error::ClaudeCli(format!("Failed to close stdin: {}", e)))?;
            }

            // Read stdout in real-time
            let stdout = child.stdout.take().expect("stdout not captured");
            let mut reader = BufReader::new(stdout).lines();
            let mut final_result: Option<ClaudeResult> = None;
            let mut all_text = String::new();

            while let Some(line) = reader.next_line().await.map_err(|e| {
                Error::ClaudeCli(format!("Failed to read stdout: {}", e))
            })? {
                // Parse and display each line
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                    // Check for result
                    if let Some(result) = json.get("result") {
                        if let Some(text) = result.as_str() {
                            all_text = text.to_string();
                            // Send result to TUI via tracing
                            info!(target: "claude", "[Result] {}", text);
                        }
                    }

                    // Check for session_id
                    if let Some(sid) = json.get("session_id").and_then(|v| v.as_str()) {
                        if final_result.is_none() {
                            final_result = Some(ClaudeResult {
                                text: String::new(),
                                is_error: false,
                                session_id: Some(sid.to_string()),
                            });
                        } else if let Some(ref mut r) = final_result {
                            r.session_id = Some(sid.to_string());
                        }
                    }

                    // Check for is_error
                    if let Some(is_error) = json.get("is_error").and_then(|v| v.as_bool()) {
                        if let Some(ref mut r) = final_result {
                            r.is_error = is_error;
                        }
                    }

                    // Check for tool use (for debug output)
                    if let Some(tool_name) = json.get("tool_name").and_then(|v| v.as_str()) {
                        info!(target: "claude", "[Tool] {}", tool_name);
                    }

                    // Check for tool input
                    if let Some(input) = json.get("tool_input") {
                        if let Some(input_str) = input.as_str() {
                            info!(target: "claude", "[Tool Input] {}", input_str);
                        } else {
                            info!(target: "claude", "[Tool Input] {}", input.to_string());
                        }
                    }

                    // Check for message content (thinking)
                    if let Some(content) = json.get("content").and_then(|v| v.as_array()) {
                        for item in content {
                            if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                                info!(target: "claude", "[Thinking] {}", text);
                            }
                        }
                    }
                }
            }

            // Wait for process to complete
            let status = child.wait().await
                .map_err(|e| Error::ClaudeCli(format!("Failed to wait: {}", e)))?;

            if !status.success() {
                warn!(exit_code = ?status.code(), "Claude exited with non-zero status");
            }

            // Build final result
            if let Some(mut r) = final_result {
                r.text = all_text;
                Ok::<ClaudeResult, Error>(r)
            } else {
                Ok::<ClaudeResult, Error>(ClaudeResult {
                    text: all_text,
                    is_error: false,
                    session_id: None,
                })
            }
        })
        .await
        .map_err(|_| Error::Timeout("Claude call timed out".to_string()))??;

        Ok(result)
    }
}

impl Default for ClaudeRunner {
    fn default() -> Self {
        Self::new()
    }
}

/// Truncate prompt safely, preserving structure
fn truncate_prompt_safely(prompt: &str, max_length: usize) -> String {
    if prompt.len() <= max_length {
        return prompt.to_string();
    }

    // Reserve space for the truncation marker
    let marker = "\n\n... (content truncated) ...\n\n";
    let marker_len = marker.len();

    // Ensure we have enough space for meaningful content
    if max_length <= marker_len + 10 {
        // Not enough space for structure, just truncate
        return format!(
            "... (truncated)\n{}",
            &prompt[prompt.len().saturating_sub(max_length.saturating_sub(15))..]
        );
    }

    let available = max_length.saturating_sub(marker_len);
    let keep_len = available / 2;
    format!(
        "{}{}{}",
        &prompt[..keep_len],
        marker,
        &prompt[prompt.len().saturating_sub(keep_len)..]
    )
}

/// Parse Claude CLI output to extract result
fn parse_claude_result(output: &str) -> Result<ClaudeResult> {
    // Try to extract JSON from code block
    if let Some(json) = extract_json_from_code_block(output) {
        if let Ok(result) = parse_result_json(&json) {
            return Ok(result);
        }
    }

    // Try to parse each line as JSON
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(result) = parse_result_json(line) {
            return Ok(result);
        }
    }

    // Try to parse entire output as JSON
    if let Ok(result) = parse_result_json(output) {
        return Ok(result);
    }

    // Try to find any JSON object in the output
    if let Some(json) = find_json_object(output) {
        if let Ok(result) = parse_result_json(&json) {
            return Ok(result);
        }
    }

    // If all parsing fails, return the raw output as the result
    // This handles cases where Claude returns plain text instead of JSON
    warn!(output_len = output.len(), "Could not parse JSON, returning raw output");
    Ok(ClaudeResult {
        text: output.to_string(),
        is_error: false,
        session_id: None,
    })
}

/// Extract JSON from ```json code block
fn extract_json_from_code_block(text: &str) -> Option<String> {
    let re = regex::Regex::new(r"```json\s*(\{.*?\})\s*```").ok()?;
    let caps = re.captures(text)?;
    Some(caps[1].to_string())
}

/// Find any JSON object in text
fn find_json_object(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let mut depth = 0;
    let mut end = start;
    for (i, c) in text[start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = start + i + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    if depth == 0 {
        Some(text[start..end].to_string())
    } else {
        None
    }
}

/// Parse a JSON string into ClaudeResult
fn parse_result_json(json: &str) -> Result<ClaudeResult> {
    let obj: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| Error::ParseError(format!("JSON parse error: {}", e)))?;

    let is_error = obj
        .get("is_error")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let text = obj
        .get("result")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let session_id = obj
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(ClaudeResult {
        text,
        is_error,
        session_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_prompt_safely() {
        let long_prompt = "x".repeat(100000);
        let truncated = truncate_prompt_safely(&long_prompt, 80000);
        assert!(truncated.len() <= 80000);
        assert!(truncated.contains("truncated"));
    }

    #[test]
    fn test_parse_result_json() {
        let json = r#"{"result": "Hello", "is_error": false, "session_id": "abc123"}"#;
        let result = parse_result_json(json).unwrap();
        assert_eq!(result.text, "Hello");
        assert!(!result.is_error);
        assert_eq!(result.session_id, Some("abc123".to_string()));
    }

    #[test]
    fn test_parse_result_json_error() {
        let json = r#"{"result": "Error message", "is_error": true}"#;
        let result = parse_result_json(json).unwrap();
        assert!(result.is_error);
        assert_eq!(result.text, "Error message");
    }

    #[test]
    fn test_extract_json_from_code_block() {
        let text = r#"```json
{"result": "test", "is_error": false}
```"#;
        let json = extract_json_from_code_block(text).unwrap();
        assert!(json.contains("result"));
    }

    #[test]
    fn test_claude_runner_default() {
        let runner = ClaudeRunner::default();
        assert_eq!(runner.model, "haiku");
        assert!(!runner.debug_mode);
    }

    #[test]
    fn test_claude_runner_with_model() {
        let runner = ClaudeRunner::new().with_model("claude-3-opus");
        assert_eq!(runner.model, "claude-3-opus");
    }

    #[test]
    fn test_claude_runner_with_model_enum() {
        let runner = ClaudeRunner::new().with_model_enum(Model::Smart);
        assert_eq!(runner.model, "sonnet");
    }

    #[test]
    fn test_claude_runner_with_debug() {
        let runner = ClaudeRunner::new().with_debug(true);
        assert!(runner.debug_mode);
    }

    #[test]
    fn test_truncate_preserves_structure() {
        let prompt = "Start middle content end with more text to exceed limit".to_string();
        let truncated = truncate_prompt_safely(&prompt, 50);
        assert!(truncated.contains("truncated"));
        assert!(truncated.len() <= 50);
    }

    #[test]
    fn test_truncate_very_small_max() {
        let prompt = "This is a test prompt".to_string();
        let truncated = truncate_prompt_safely(&prompt, 5);
        assert!(truncated.contains("truncated"));
    }

    #[test]
    fn test_find_json_object() {
        let text = r#"Some text before {"result": "test", "is_error": false} some text after"#;
        let json = find_json_object(text).unwrap();
        assert!(json.contains("result"));
    }

    #[test]
    fn test_find_json_object_nested() {
        let text = r#"Outer {"inner": {"key": "value"}} text"#;
        let json = find_json_object(text).unwrap();
        assert!(json.contains("inner"));
    }

    #[test]
    fn test_find_json_object_none() {
        let text = "No JSON here";
        assert!(find_json_object(text).is_none());
    }
}
