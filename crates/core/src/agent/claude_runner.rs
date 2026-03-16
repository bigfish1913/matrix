//! Claude CLI runner - handles subprocess calls to claude.

use crate::config::MAX_PROMPT_LENGTH;
use crate::error::{Error, Result};
use std::path::Path;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tracing::{debug, warn};

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
            model: "glm-5".to_string(),
            debug_mode: false,
        }
    }

    /// Set the model
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
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
            warn!(len = prompt.len(), max = MAX_PROMPT_LENGTH, "Prompt truncated");
            truncate_prompt_safely(prompt, MAX_PROMPT_LENGTH)
        } else {
            prompt.to_string()
        };

        // Build command
        let mut cmd = Command::new("claude");
        cmd.args(["--model", &self.model])
            .args(["--output-format", if self.debug_mode { "stream-json" } else { "json" }])
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

        debug!(model = %self.model, "Calling Claude CLI");

        let result = timeout(timeout_duration, async {
            let mut child = cmd.spawn()
                .map_err(|e| Error::ClaudeCli(format!("Failed to spawn: {}", e)))?;

            // Write prompt to stdin
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(prompt.as_bytes()).await
                    .map_err(|e| Error::ClaudeCli(format!("Failed to write stdin: {}", e)))?;
                stdin.shutdown().await
                    .map_err(|e| Error::ClaudeCli(format!("Failed to close stdin: {}", e)))?;
            }

            // Collect output
            let output = child.wait_with_output().await
                .map_err(|e| Error::ClaudeCli(format!("Failed to wait: {}", e)))?;

            Ok::<_, Error>(output)
        })
        .await
        .map_err(|_| Error::Timeout("Claude call timed out".to_string()))??;

        let stdout = String::from_utf8_lossy(&result.stdout);
        let stderr = String::from_utf8_lossy(&result.stderr);
        let combined = format!("{}{}", stdout, stderr);

        // Parse JSON result
        parse_claude_result(&combined)
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
        return format!("... (truncated)\n{}", &prompt[prompt.len().saturating_sub(max_length.saturating_sub(15))..]);
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

    Err(Error::ParseError(format!(
        "No valid JSON result from Claude. Output: {}",
        &output[..output.len().min(500)]
    )))
}

/// Extract JSON from ```json code block
fn extract_json_from_code_block(text: &str) -> Option<String> {
    let re = regex::Regex::new(r"```json\s*(\{.*?\})\s*```").ok()?;
    let caps = re.captures(text)?;
    Some(caps[1].to_string())
}

/// Parse a JSON string into ClaudeResult
fn parse_result_json(json: &str) -> Result<ClaudeResult> {
    let obj: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| Error::ParseError(format!("JSON parse error: {}", e)))?;

    let is_error = obj.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false);
    let text = obj.get("result").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let session_id = obj.get("session_id").and_then(|v| v.as_str()).map(|s| s.to_string());

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
        assert_eq!(runner.model, "glm-5");
        assert!(!runner.debug_mode);
    }

    #[test]
    fn test_claude_runner_with_model() {
        let runner = ClaudeRunner::new().with_model("claude-3-opus");
        assert_eq!(runner.model, "claude-3-opus");
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
}