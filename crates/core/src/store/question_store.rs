//! QuestionStore - persistent storage for questions.

use crate::error::{Error, Result};
use crate::models::{Question, QuestionStatus};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::debug;

/// Questions file structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct QuestionsFile {
    questions: Vec<Question>,
}

/// Question storage manager
pub struct QuestionStore {
    questions_path: PathBuf,
    decisions_path: PathBuf,
}

impl QuestionStore {
    /// Create a new QuestionStore
    pub async fn new(questions_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&questions_dir).await?;
        let questions_path = questions_dir.join("questions.json");
        let decisions_path = questions_dir.join("decisions.log");

        let store = Self {
            questions_path,
            decisions_path,
        };

        // Initialize questions.json if not exists
        if !store.questions_path.exists() {
            store.save_questions(&QuestionsFile::default()).await?;
        }

        Ok(store)
    }

    /// Load questions from file
    async fn load_questions(&self) -> Result<QuestionsFile> {
        if !self.questions_path.exists() {
            return Ok(QuestionsFile::default());
        }
        let content = fs::read_to_string(&self.questions_path).await?;
        let file: QuestionsFile = serde_json::from_str(&content)?;
        Ok(file)
    }

    /// Save questions to file
    async fn save_questions(&self, file: &QuestionsFile) -> Result<()> {
        let content = serde_json::to_string_pretty(file)?;
        fs::write(&self.questions_path, content).await?;
        debug!(path = %self.questions_path.display(), "Questions saved");
        Ok(())
    }

    /// Create a new question
    pub async fn create(&self, question: &Question) -> Result<()> {
        let mut file = self.load_questions().await?;
        file.questions.push(question.clone());
        self.save_questions(&file).await?;
        debug!(question_id = %question.id, "Question created");
        Ok(())
    }

    /// Get all questions
    pub async fn all_questions(&self) -> Result<Vec<Question>> {
        let file = self.load_questions().await?;
        Ok(file.questions)
    }

    /// Get all pending questions
    pub async fn pending_questions(&self) -> Result<Vec<Question>> {
        let questions = self.all_questions().await?;
        Ok(questions
            .into_iter()
            .filter(|q| q.status == QuestionStatus::Pending)
            .collect())
    }

    /// Get pending count
    pub async fn pending_count(&self) -> Result<usize> {
        let pending = self.pending_questions().await?;
        Ok(pending.len())
    }

    /// Get questions for a specific task
    pub async fn questions_for_task(&self, task_id: &str) -> Result<Vec<Question>> {
        let questions = self.all_questions().await?;
        Ok(questions
            .into_iter()
            .filter(|q| q.task_id == task_id)
            .collect())
    }

    /// Get question by ID
    pub async fn get(&self, id: &str) -> Result<Option<Question>> {
        let questions = self.all_questions().await?;
        Ok(questions.into_iter().find(|q| q.id == id))
    }

    /// Answer a question
    pub async fn answer(&self, id: &str, answer: &str) -> Result<()> {
        let mut file = self.load_questions().await?;

        if let Some(question) = file.questions.iter_mut().find(|q| q.id == id) {
            question.answer(answer.to_string());
            self.save_questions(&file).await?;
            debug!(question_id = %id, answer = %answer, "Question answered");
            Ok(())
        } else {
            Err(Error::TaskNotFound(format!("Question not found: {}", id)))
        }
    }

    /// Record auto-decision for non-blocking question
    pub async fn record_auto_decision(&self, id: &str, decision: &str, reason: &str) -> Result<()> {
        let mut file = self.load_questions().await?;

        if let Some(question) = file.questions.iter_mut().find(|q| q.id == id) {
            question.auto_decide(decision.to_string(), reason.to_string());

            // Clone needed data for decision log before saving
            let task_id = question.task_id.clone();
            let question_text = question.question.clone();

            self.save_questions(&file).await?;

            // Append to decision log
            self.append_decision_log(&task_id, &question_text, decision, reason)
                .await?;

            debug!(question_id = %id, decision = %decision, "Auto-decision recorded");
            Ok(())
        } else {
            Err(Error::TaskNotFound(format!("Question not found: {}", id)))
        }
    }

    /// Append decision to log file
    async fn append_decision_log(
        &self,
        task_id: &str,
        question: &str,
        decision: &str,
        reason: &str,
    ) -> Result<()> {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S");
        let entry = format!(
            "[{}] {} | {}\n  Decision: {}\n  Reason: {}\n\n",
            timestamp, task_id, question, decision, reason
        );

        // Create file if not exists, append otherwise
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.decisions_path)
            .await?
            .write_all(entry.as_bytes())
            .await?;

        debug!(path = %self.decisions_path.display(), "Decision log appended");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_question_store_create_and_get() {
        let dir = tempdir().unwrap();
        let store = QuestionStore::new(dir.path().to_path_buf()).await.unwrap();

        let question = Question::new(
            "task-001".to_string(),
            "Which database?".to_string(),
            vec!["SQLite".to_string(), "PostgreSQL".to_string()],
            vec!["Lightweight".to_string(), "Powerful".to_string()],
            vec!["Limited scale".to_string(), "Complex setup".to_string()],
            Some(0),
            Some("Simplest option".to_string()),
            true,
        );

        let question_id = question.id.clone();
        store.create(&question).await.unwrap();

        let loaded = store.get(&question_id).await.unwrap().unwrap();
        assert_eq!(loaded.question, "Which database?");
        assert_eq!(loaded.options.len(), 2);
        assert_eq!(loaded.status, QuestionStatus::Pending);
    }

    #[tokio::test]
    async fn test_question_store_answer() {
        let dir = tempdir().unwrap();
        let store = QuestionStore::new(dir.path().to_path_buf()).await.unwrap();

        let question = Question::new(
            "task-001".to_string(),
            "Test question?".to_string(),
            vec!["A".to_string(), "B".to_string()],
            vec![],
            vec![],
            None,
            None,
            true,
        );

        let question_id = question.id.clone();
        store.create(&question).await.unwrap();

        store.answer(&question_id, "A").await.unwrap();

        let loaded = store.get(&question_id).await.unwrap().unwrap();
        assert_eq!(loaded.status, QuestionStatus::Answered);
        assert_eq!(loaded.answer, Some("A".to_string()));
        assert!(loaded.answered_at.is_some());
    }

    #[tokio::test]
    async fn test_question_store_pending_count() {
        let dir = tempdir().unwrap();
        let store = QuestionStore::new(dir.path().to_path_buf()).await.unwrap();

        // Create 3 questions
        for i in 0..3 {
            let question = Question::new(
                format!("task-00{}", i),
                format!("Question {}?", i),
                vec!["A".to_string()],
                vec![],
                vec![],
                None,
                None,
                true,
            );
            store.create(&question).await.unwrap();
        }

        assert_eq!(store.pending_count().await.unwrap(), 3);

        // Answer one
        let all = store.all_questions().await.unwrap();
        store.answer(&all[0].id, "A").await.unwrap();

        assert_eq!(store.pending_count().await.unwrap(), 2);
    }
}
