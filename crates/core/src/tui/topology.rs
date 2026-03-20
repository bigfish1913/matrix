//! Task topology visualization using Mermaid diagrams.

use crate::models::TaskStatus;
use std::collections::{HashMap, HashSet};

/// Task info for topology visualization
#[derive(Debug, Clone)]
pub struct TaskTopologyInfo {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    pub parent_id: Option<String>,
    pub depth: u32,
    pub depends_on: Vec<String>,
}

/// Generate Mermaid flowchart diagram from tasks
pub fn generate_mermaid_diagram(tasks: &[TaskTopologyInfo]) -> String {
    if tasks.is_empty() {
        return "// No tasks to display".to_string();
    }

    let mut lines = vec!["```mermaid".to_string(), "flowchart TD".to_string()];

    // Create a map for quick lookup
    let task_map: HashMap<&str, &TaskTopologyInfo> =
        tasks.iter().map(|t| (t.id.as_str(), t)).collect();

    // Track which nodes we've added
    let mut added_nodes = HashSet::new();

    // Helper function to get status class
    let status_class = |status: TaskStatus| -> &'static str {
        match status {
            TaskStatus::Completed => "completed",
            TaskStatus::InProgress => "running",
            TaskStatus::Pending => "pending",
            TaskStatus::Failed => "failed",
            TaskStatus::Skipped => "skipped",
        }
    };

    // Helper function to get status icon
    let status_icon = |status: TaskStatus| -> &'static str {
        match status {
            TaskStatus::Completed => "✓",
            TaskStatus::InProgress => "●",
            TaskStatus::Pending => "○",
            TaskStatus::Failed => "✗",
            TaskStatus::Skipped => "⊘",
        }
    };

    // Sanitize ID for mermaid (remove special chars)
    let sanitize_id = |id: &str| -> String { id.replace('-', "_").replace('.', "_") };

    // Sanitize text for mermaid (escape quotes and special chars)
    let sanitize_text = |text: &str| -> String {
        text.replace('"', "'")
            .replace('\\', "\\\\")
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(50)
            .collect::<String>()
    };

    // Add subgraph for each depth level
    let max_depth = tasks.iter().map(|t| t.depth).max().unwrap_or(0);

    // Group tasks by parent
    let mut children_by_parent: HashMap<Option<String>, Vec<&TaskTopologyInfo>> = HashMap::new();
    for task in tasks.iter() {
        children_by_parent
            .entry(task.parent_id.clone())
            .or_default()
            .push(task);
    }

    // Add all nodes with their styles
    lines.push("".to_string());
    lines.push("    %% Task Nodes".to_string());

    for task in tasks.iter() {
        let node_id = sanitize_id(&task.id);
        if added_nodes.contains(&node_id) {
            continue;
        }
        added_nodes.insert(node_id.clone());

        let icon = status_icon(task.status);
        let title = sanitize_text(&task.title);
        let label = format!("{} {}\\n{}", icon, task.id, title);

        // Use different node shapes based on depth
        let node_def = if task.depth == 0 {
            format!("    {}[\"{}\"]", node_id, label)
        } else {
            format!("    {}(\"{}\")", node_id, label)
        };
        lines.push(node_def);
    }

    // Add dependency edges (depends_on)
    lines.push("".to_string());
    lines.push("    %% Dependency Edges (depends_on)".to_string());

    for task in tasks.iter() {
        let node_id = sanitize_id(&task.id);
        for dep_id in &task.depends_on {
            let dep_node_id = sanitize_id(dep_id);
            lines.push(format!("    {} -->|depends| {}", dep_node_id, node_id));
        }
    }

    // Add parent-child edges (subtask relationships)
    lines.push("".to_string());
    lines.push("    %% Subtask Edges (parent-child)".to_string());

    for task in tasks.iter() {
        if let Some(parent_id) = &task.parent_id {
            let parent_node_id = sanitize_id(parent_id);
            let node_id = sanitize_id(&task.id);
            lines.push(format!("    {} -.->|splits| {}", parent_node_id, node_id));
        }
    }

    // Add style classes
    lines.push("".to_string());
    lines.push("    %% Styles".to_string());
    lines.push("    classDef completed fill:#2d5a3d,stroke:#4ade80,color:#fff".to_string());
    lines.push("    classDef running fill:#5a4a2d,stroke:#facc15,color:#fff".to_string());
    lines.push("    classDef pending fill:#3d3d3d,stroke:#6b7280,color:#fff".to_string());
    lines.push("    classDef failed fill:#5a2d2d,stroke:#f87171,color:#fff".to_string());
    lines.push("    classDef skipped fill:#3d3d3d,stroke:#4b5563,color:#9ca3af".to_string());

    // Apply styles to nodes
    lines.push("".to_string());
    lines.push("    %% Apply Styles".to_string());

    for task in tasks.iter() {
        let node_id = sanitize_id(&task.id);
        let class = status_class(task.status);
        lines.push(format!("    class {} {}", node_id, class));
    }

    lines.push("```".to_string());

    lines.join("\n")
}

/// Generate ASCII tree representation of tasks
pub fn generate_ascii_tree(tasks: &[TaskTopologyInfo]) -> String {
    if tasks.is_empty() {
        return "No tasks".to_string();
    }

    // Build tree structure
    let mut result = Vec::new();
    let root_tasks: Vec<&TaskTopologyInfo> =
        tasks.iter().filter(|t| t.parent_id.is_none()).collect();

    for (idx, task) in root_tasks.iter().enumerate() {
        let is_last = idx == root_tasks.len() - 1;
        add_ascii_node(tasks, task, "", is_last, &mut result);
    }

    result.join("\n")
}

fn add_ascii_node(
    tasks: &[TaskTopologyInfo],
    task: &TaskTopologyInfo,
    prefix: &str,
    is_last: bool,
    result: &mut Vec<String>,
) {
    let status_icon = match task.status {
        TaskStatus::Completed => "✓",
        TaskStatus::InProgress => "●",
        TaskStatus::Pending => "○",
        TaskStatus::Failed => "✗",
        TaskStatus::Skipped => "⊘",
    };

    let connector = if is_last { "└── " } else { "├── " };
    let line = format!(
        "{}{}{} {} {}",
        prefix, connector, status_icon, task.id, task.title
    );
    result.push(line);

    // Find children
    let children: Vec<&TaskTopologyInfo> = tasks
        .iter()
        .filter(|t| t.parent_id.as_ref() == Some(&task.id))
        .collect();

    let child_prefix = if is_last {
        format!("{}    ", prefix)
    } else {
        format!("{}│   ", prefix)
    };

    for (idx, child) in children.iter().enumerate() {
        let child_is_last = idx == children.len() - 1;
        add_ascii_node(tasks, child, &child_prefix, child_is_last, result);
    }
}

/// Generate task topology file content
pub fn generate_topology_file(tasks: &[TaskTopologyInfo]) -> String {
    let mut content = String::new();

    content.push_str("# Task Topology\n\n");
    content.push_str(&format!("Total tasks: {}\n\n", tasks.len()));

    // ASCII tree
    content.push_str("## Task Tree\n\n```\n");
    content.push_str(&generate_ascii_tree(tasks));
    content.push_str("\n```\n\n");

    // Mermaid diagram
    content.push_str("## Dependency Graph\n\n");
    content.push_str(&generate_mermaid_diagram(tasks));
    content.push_str("\n\n");

    // Statistics
    let completed = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Completed)
        .count();
    let failed = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Failed)
        .count();
    let pending = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Pending)
        .count();
    let running = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::InProgress)
        .count();

    content.push_str("## Statistics\n\n");
    content.push_str(&format!("- Completed: {}\n", completed));
    content.push_str(&format!("- Failed: {}\n", failed));
    content.push_str(&format!("- Pending: {}\n", pending));
    content.push_str(&format!("- Running: {}\n", running));
    content.push_str(&format!("- Total: {}\n", tasks.len()));

    content
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_tasks() -> Vec<TaskTopologyInfo> {
        vec![
            TaskTopologyInfo {
                id: "task-001".to_string(),
                title: "Setup project".to_string(),
                status: TaskStatus::Completed,
                parent_id: None,
                depth: 0,
                depends_on: vec![],
            },
            TaskTopologyInfo {
                id: "task-002".to_string(),
                title: "Add features".to_string(),
                status: TaskStatus::InProgress,
                parent_id: None,
                depth: 0,
                depends_on: vec!["task-001".to_string()],
            },
            TaskTopologyInfo {
                id: "task-002-1".to_string(),
                title: "Feature A".to_string(),
                status: TaskStatus::Completed,
                parent_id: Some("task-002".to_string()),
                depth: 1,
                depends_on: vec![],
            },
            TaskTopologyInfo {
                id: "task-002-2".to_string(),
                title: "Feature B".to_string(),
                status: TaskStatus::Pending,
                parent_id: Some("task-002".to_string()),
                depth: 1,
                depends_on: vec!["task-002-1".to_string()],
            },
        ]
    }

    #[test]
    fn test_generate_mermaid_diagram() {
        let tasks = create_test_tasks();
        let diagram = generate_mermaid_diagram(&tasks);

        assert!(diagram.contains("```mermaid"));
        assert!(diagram.contains("flowchart TD"));
        assert!(diagram.contains("task_001"));
        assert!(diagram.contains("task_002"));
        assert!(diagram.contains("classDef completed"));
    }

    #[test]
    fn test_generate_ascii_tree() {
        let tasks = create_test_tasks();
        let tree = generate_ascii_tree(&tasks);

        assert!(tree.contains("task-001"));
        assert!(tree.contains("task-002"));
        assert!(tree.contains("└──") || tree.contains("├──"));
    }

    #[test]
    fn test_empty_tasks() {
        let tasks: Vec<TaskTopologyInfo> = vec![];
        assert_eq!(generate_mermaid_diagram(&tasks), "// No tasks to display");
        assert_eq!(generate_ascii_tree(&tasks), "No tasks");
    }
}
