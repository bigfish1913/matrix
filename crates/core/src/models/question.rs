//! Question model for agent interactive Q&A.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

static QUESTION_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Question status
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionStatus {
    #[default]
    Pending, // Waiting for answer
    Answered,    // User answered
    AutoDecided, // Agent decided (non-blocking)
    Expired,     // Cancelled/outdated
}

impl std::fmt::Display for QuestionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Answered => write!(f, "answered"),
            Self::AutoDecided => write!(f, "auto-decided"),
            Self::Expired => write!(f, "expired"),
        }
    }
}

/// Question model for agent interactive Q&A
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    /// Question ID, e.g. "q-001"
    pub id: String,
    /// Associated task ID
    pub task_id: String,
    /// Question text
    pub question: String,
    /// Multiple choice options
    pub options: Vec<String>,
    /// Pros for each option (parallel to options)
    #[serde(default)]
    pub pros: Vec<String>,
    /// Cons for each option (parallel to options)
    #[serde(default)]
    pub cons: Vec<String>,
    /// Agent's recommended option index
    pub recommended: Option<usize>,
    /// Reason for recommendation
    pub recommendation_reason: Option<String>,
    /// Whether this blocks task execution
    pub blocking: bool,
    /// Current status
    #[serde(default)]
    pub status: QuestionStatus,
    /// User's answer (option text or custom input)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    /// Decision log (for auto-decided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_log: Option<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Answer timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answered_at: Option<DateTime<Utc>>,
}

impl Question {
    /// Create a new question
    pub fn new(
        task_id: String,
        question: String,
        options: Vec<String>,
        pros: Vec<String>,
        cons: Vec<String>,
        recommended: Option<usize>,
        recommendation_reason: Option<String>,
        blocking: bool,
    ) -> Self {
        let id = Self::generate_id();
        Self {
            id,
            task_id,
            question,
            options,
            pros,
            cons,
            recommended,
            recommendation_reason,
            blocking,
            status: QuestionStatus::default(),
            answer: None,
            decision_log: None,
            created_at: Utc::now(),
            answered_at: None,
        }
    }

    /// Generate unique question ID
    fn generate_id() -> String {
        let num = QUESTION_COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("q-{:03}", num)
    }

    /// Check if question is still pending
    pub fn is_pending(&self) -> bool {
        self.status == QuestionStatus::Pending
    }

    /// Mark as answered
    pub fn answer(&mut self, answer: String) {
        self.status = QuestionStatus::Answered;
        self.answer = Some(answer);
        self.answered_at = Some(Utc::now());
    }

    /// Mark as auto-decided
    pub fn auto_decide(&mut self, decision: String, reason: String) {
        self.status = QuestionStatus::AutoDecided;
        self.answer = Some(decision);
        self.decision_log = Some(reason);
        self.answered_at = Some(Utc::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_question_new() {
        let q = Question::new(
            "task-001".to_string(),
            "Which DB?".to_string(),
            vec!["SQLite".to_string(), "PostgreSQL".to_string()],
            vec!["Lightweight".to_string(), "Powerful".to_string()],
            vec!["Limited".to_string(), "Complex".to_string()],
            Some(0),
            Some("Simplest option".to_string()),
            true,
        );
        assert!(q.id.starts_with("q-"));
        assert_eq!(q.task_id, "task-001");
        assert_eq!(q.status, QuestionStatus::Pending);
        assert!(q.blocking);
    }

    #[test]
    fn test_question_answer() {
        let mut q = Question::new(
            "task-001".to_string(),
            "Test?".to_string(),
            vec!["A".to_string()],
            vec![],
            vec![],
            None,
            None,
            true,
        );
        q.answer("A".to_string());
        assert_eq!(q.status, QuestionStatus::Answered);
        assert_eq!(q.answer, Some("A".to_string()));
        assert!(q.answered_at.is_some());
    }

    #[test]
    fn test_question_auto_decide() {
        let mut q = Question::new(
            "task-001".to_string(),
            "Test?".to_string(),
            vec!["A".to_string()],
            vec![],
            vec![],
            Some(0),
            Some("Recommended".to_string()),
            false,
        );
        q.auto_decide("A".to_string(), "Because recommended".to_string());
        assert_eq!(q.status, QuestionStatus::AutoDecided);
        assert!(q.decision_log.is_some());
    }
}
