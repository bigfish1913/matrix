//! Test runner detection.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Test runner configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestRunner {
    pub name: String,
    pub command: Vec<String>,
}

/// Test runner detector
pub struct TestRunnerDetector;

impl TestRunnerDetector {
    /// Detect test runner in workspace
    pub fn detect(workspace: &Path) -> Option<TestRunner> {
        // Go
        if workspace.join("go.mod").exists() {
            return Some(TestRunner {
                name: "go".to_string(),
                command: vec!["go".to_string(), "test".to_string(), "./...".to_string()],
            });
        }

        // Rust
        if workspace.join("Cargo.toml").exists() {
            return Some(TestRunner {
                name: "cargo".to_string(),
                command: vec!["cargo".to_string(), "test".to_string()],
            });
        }

        // Node.js
        if workspace.join("package.json").exists() {
            if let Ok(content) = std::fs::read_to_string(workspace.join("package.json")) {
                if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
                    if pkg.get("scripts").and_then(|s| s.get("test")).is_some() {
                        return Some(TestRunner {
                            name: "npm".to_string(),
                            command: vec![
                                "npm".to_string(),
                                "test".to_string(),
                                "--".to_string(),
                                "--passWithNoTests".to_string(),
                            ],
                        });
                    }
                }
            }
        }

        // Python
        for marker in ["pytest.ini", "setup.cfg", "pyproject.toml", "setup.py"] {
            if workspace.join(marker).exists() {
                return Some(TestRunner {
                    name: "pytest".to_string(),
                    command: vec![
                        "python".to_string(),
                        "-m".to_string(),
                        "pytest".to_string(),
                        "-v".to_string(),
                        "--tb=short".to_string(),
                    ],
                });
            }
        }

        // Python test files
        if has_python_test_files(workspace) {
            return Some(TestRunner {
                name: "pytest".to_string(),
                command: vec![
                    "python".to_string(),
                    "-m".to_string(),
                    "pytest".to_string(),
                    "-v".to_string(),
                    "--tb=short".to_string(),
                ],
            });
        }

        // Makefile
        if workspace.join("Makefile").exists() {
            if let Ok(content) = std::fs::read_to_string(workspace.join("Makefile")) {
                if content.lines().any(|line| line.starts_with("test:")) {
                    return Some(TestRunner {
                        name: "make".to_string(),
                        command: vec!["make".to_string(), "test".to_string()],
                    });
                }
            }
        }

        None
    }

    /// Detect test runners in workspace and subdirectories
    pub fn detect_with_subdirs(workspace: &Path) -> Vec<(String, TestRunner)> {
        let mut runners = Vec::new();

        if let Some(runner) = Self::detect(workspace) {
            runners.push((".".to_string(), runner));
        }

        for subdir in ["backend", "frontend", "server", "client", "api"] {
            let sub = workspace.join(subdir);
            if sub.exists() {
                if let Some(runner) = Self::detect(&sub) {
                    runners.push((subdir.to_string(), runner));
                }
            }
        }

        runners
    }
}

fn has_python_test_files(workspace: &Path) -> bool {
    walkdir::WalkDir::new(workspace)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .any(|e| {
            if let Some(name) = e.file_name().to_str() {
                name.starts_with("test_") || name.ends_with("_test.py")
            } else {
                false
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_detect_cargo_test() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "").unwrap();

        let runner = TestRunnerDetector::detect(dir.path()).unwrap();
        assert_eq!(runner.name, "cargo");
        assert_eq!(runner.command, vec!["cargo", "test"]);
    }

    #[test]
    fn test_detect_go_test() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "").unwrap();

        let runner = TestRunnerDetector::detect(dir.path()).unwrap();
        assert_eq!(runner.name, "go");
    }

    #[test]
    fn test_detect_npm_test() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"scripts": {"test": "jest"}}"#).unwrap();

        let runner = TestRunnerDetector::detect(dir.path()).unwrap();
        assert_eq!(runner.name, "npm");
    }

    #[test]
    fn test_detect_makefile_test() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Makefile"), "test:\n\techo test\n").unwrap();

        let runner = TestRunnerDetector::detect(dir.path()).unwrap();
        assert_eq!(runner.name, "make");
    }

    #[test]
    fn test_no_test_runner() {
        let dir = tempdir().unwrap();
        let runner = TestRunnerDetector::detect(dir.path());
        assert!(runner.is_none());
    }
}