//! Project type detection.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Supported project types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectType {
    Rust,
    NodeJs,
    Python,
    Go,
    Ruby,
    Php,
    Unknown,
}

impl std::fmt::Display for ProjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rust => write!(f, "Rust"),
            Self::NodeJs => write!(f, "Node.js"),
            Self::Python => write!(f, "Python"),
            Self::Go => write!(f, "Go"),
            Self::Ruby => write!(f, "Ruby"),
            Self::Php => write!(f, "PHP"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Project information
#[derive(Debug, Clone)]
pub struct ProjectInfo {
    pub project_type: ProjectType,
    pub package_manager: Option<String>,
    pub install_command: Option<Vec<String>>,
}

/// Project type detector
pub struct ProjectDetector;

impl ProjectDetector {
    /// Detect project type in workspace
    pub fn detect(workspace: &Path) -> ProjectInfo {
        // Rust
        if workspace.join("Cargo.toml").exists() {
            return ProjectInfo {
                project_type: ProjectType::Rust,
                package_manager: Some("cargo".to_string()),
                install_command: Some(vec!["cargo".to_string(), "fetch".to_string()]),
            };
        }

        // Node.js
        if workspace.join("package.json").exists() {
            let pm = Self::detect_node_package_manager(workspace);
            return ProjectInfo {
                project_type: ProjectType::NodeJs,
                package_manager: Some(pm.clone()),
                install_command: Some(vec![pm, "install".to_string()]),
            };
        }

        // Python
        if Self::is_python_project(workspace) {
            return ProjectInfo {
                project_type: ProjectType::Python,
                package_manager: Some("pip".to_string()),
                install_command: Self::detect_python_install_command(workspace),
            };
        }

        // Go
        if workspace.join("go.mod").exists() {
            return ProjectInfo {
                project_type: ProjectType::Go,
                package_manager: Some("go".to_string()),
                install_command: Some(vec!["go".to_string(), "mod".to_string(), "download".to_string()]),
            };
        }

        // Ruby
        if workspace.join("Gemfile").exists() {
            return ProjectInfo {
                project_type: ProjectType::Ruby,
                package_manager: Some("bundler".to_string()),
                install_command: Some(vec!["bundle".to_string(), "install".to_string()]),
            };
        }

        // PHP
        if workspace.join("composer.json").exists() {
            return ProjectInfo {
                project_type: ProjectType::Php,
                package_manager: Some("composer".to_string()),
                install_command: Some(vec!["composer".to_string(), "install".to_string()]),
            };
        }

        ProjectInfo {
            project_type: ProjectType::Unknown,
            package_manager: None,
            install_command: None,
        }
    }

    fn detect_node_package_manager(workspace: &Path) -> String {
        if workspace.join("bun.lockb").exists() {
            "bun".to_string()
        } else if workspace.join("pnpm-lock.yaml").exists() {
            "pnpm".to_string()
        } else if workspace.join("yarn.lock").exists() {
            "yarn".to_string()
        } else {
            "npm".to_string()
        }
    }

    fn is_python_project(workspace: &Path) -> bool {
        workspace.join("requirements.txt").exists()
            || workspace.join("pyproject.toml").exists()
            || workspace.join("setup.py").exists()
            || workspace.join("setup.cfg").exists()
            || has_files_matching(workspace, "test_*.py")
            || has_files_matching(workspace, "*_test.py")
    }

    fn detect_python_install_command(workspace: &Path) -> Option<Vec<String>> {
        if workspace.join("requirements.txt").exists() {
            Some(vec!["pip".to_string(), "install".to_string(), "-r".to_string(), "requirements.txt".to_string()])
        } else if workspace.join("pyproject.toml").exists() {
            Some(vec!["pip".to_string(), "install".to_string(), "-e".to_string(), ".".to_string()])
        } else {
            None
        }
    }
}

/// Check if directory has files matching pattern
fn has_files_matching(dir: &Path, pattern: &str) -> bool {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .any(|e| {
            if let Some(name) = e.file_name().to_str() {
                if pattern.starts_with('*') {
                    name.ends_with(&pattern[1..])
                } else if pattern.ends_with('*') {
                    name.starts_with(&pattern[..pattern.len() - 1])
                } else {
                    name == pattern
                }
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
    fn test_detect_rust_project() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "").unwrap();

        let info = ProjectDetector::detect(dir.path());
        assert_eq!(info.project_type, ProjectType::Rust);
        assert_eq!(info.package_manager, Some("cargo".to_string()));
    }

    #[test]
    fn test_detect_node_project() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();

        let info = ProjectDetector::detect(dir.path());
        assert_eq!(info.project_type, ProjectType::NodeJs);
    }

    #[test]
    fn test_detect_unknown_project() {
        let dir = tempdir().unwrap();

        let info = ProjectDetector::detect(dir.path());
        assert_eq!(info.project_type, ProjectType::Unknown);
    }

    #[test]
    fn test_detect_node_package_manager() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        fs::write(dir.path().join("yarn.lock"), "").unwrap();

        let info = ProjectDetector::detect(dir.path());
        assert_eq!(info.package_manager, Some("yarn".to_string()));
    }
}