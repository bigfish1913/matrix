//! AI Prompts for//!
//! This module contains all prompt templates used by the orchestrator.
//! Keeping prompts in one place makes them easier to review and modify.

/// Language instruction for Chinese
pub const LANG_ZH: &str = "请用中文提问，选项也用中文。优缺点和推荐理由也用中文。";

/// Language instruction for English
pub const LANG_EN: &str = "Please ask questions and provide options in English. Pros, cons and recommendations also in English.";

/// JSON format reminder for clarification
pub const JSON_FORMAT_REMINDER: &str = r#"
CRITICAL: You MUST respond with ONLY a valid JSON array. No markdown, no code blocks, no explanation.
Start your response with '[' and end with ']'. Do not include any text before or after the JSON.
"#;

/// Clarification prompt template
/// Placeholders: {GOAL}, {DOCUMENT}, {LANG_INSTRUCTION}
pub const CLARIFICATION_PROMPT: &str = r#"You are helping plan a software development project.

GOAL: {GOAL}
{DOCUMENT}

{LANG_INSTRUCTION}

Generate 3-5 concise, targeted clarifying questions.
For each question, provide 3-4 common options with their pros and cons.
Also recommend the best option with a reason.
{}
JSON format:
[
  {{
    "question": "Question text?",
    "options": ["Option 1", "Option 2", "Option 3"],
    "pros": ["Pro for option 1", "Pro for option 2", "Pro for option 3"],
    "cons": ["Con for option 1", "Con for option 2", "Con for option 3"],
    "recommended": 0,
    "recommendation_reason": "Why this option is recommended"
  }}
]

Example response:
[
  {{
    "question": "项目使用什么编程语言?",
    "options": ["Rust", "Python", "JavaScript", "Go"],
    "pros": ["高性能，内存安全", "开发快速，生态丰富", "前后端通用", "简洁高效，并发强"],
    "cons": ["学习曲线陡峭", "性能较低", "类型不严格", "生态较小"],
    "recommended": 0,
    "recommendation_reason": "Rust提供最佳的性能和安全性，适合长期维护的项目"
  }},
  {{
    "question": "是否需要数据库支持?",
    "options": ["是，SQLite", "是，PostgreSQL", "不需要", "不确定"],
    "pros": ["轻量，零配置", "功能强大，可扩展", "简单，无依赖", "稍后决定"],
    "cons": ["不适合高并发", "需要额外部署", "数据无法持久化", "可能延迟决策"],
    "recommended": 0,
    "recommendation_reason": "SQLite简单易用，适合中小型项目快速启动"
  }}
]

Remember: Output ONLY the JSON array, nothing else!"#;

/// Get language instruction based on language code
pub fn get_lang_instruction(lang: &str) -> &'static str {
    match lang {
        "en" => LANG_EN,
        _ => LANG_ZH,
    }
}
