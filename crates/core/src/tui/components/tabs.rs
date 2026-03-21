//! Tab switcher component.

use crate::tui::app::Tab;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Tabs},
};

/// Tab switcher component
pub struct TabSwitcher;

impl TabSwitcher {
    /// Render tab switcher
    pub fn render(current_tab: Tab) -> Tabs<'static> {
        let titles = vec!["Logs", "Tasks", "Claude Output", "Questions"];

        let tabs: Vec<Line<'static>> = titles
            .into_iter()
            .map(|t| {
                let (first, rest) = t.split_at(1);
                Line::from(vec![
                    Span::styled(first, Style::default().fg(Color::Yellow)),
                    Span::styled(rest, Style::default().fg(Color::White)),
                ])
            })
            .collect();

        Tabs::new(tabs)
            .block(Block::default().borders(Borders::BOTTOM))
            .select(match current_tab {
                Tab::Logs => 0,
                Tab::Tasks => 1,
                Tab::Output => 2,
                Tab::Questions => 3,
            })
            .style(Style::default().fg(Color::White))
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::UNDERLINED),
            )
    }
}
