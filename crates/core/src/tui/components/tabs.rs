//! Tab switcher component.

use crate::tui::{app::Tab, VerbosityLevel};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Tabs},
};

/// Tab switcher component
pub struct TabSwitcher;

impl TabSwitcher {
    /// Render tab switcher
    /// In normal mode, hide "Claude Output" tab (only show in verbose mode)
    pub fn render(current_tab: Tab, verbosity: VerbosityLevel) -> Tabs<'static> {
        let titles = if verbosity >= VerbosityLevel::Verbose {
            vec!["Logs", "Tasks", "Claude Output", "Questions"]
        } else {
            vec!["Logs", "Tasks", "Questions"]
        };

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

        // Calculate select index based on visibility
        let select_index = match current_tab {
            Tab::Logs => 0,
            Tab::Tasks => 1,
            Tab::Output => {
                if verbosity >= VerbosityLevel::Verbose {
                    2
                } else {
                    // Output tab not visible, stay on Tasks or handle gracefully
                    1
                }
            }
            Tab::Questions => {
                if verbosity >= VerbosityLevel::Verbose {
                    3
                } else {
                    2 // Questions is at index 2 when Output is hidden
                }
            }
        };

        Tabs::new(tabs)
            .block(Block::default().borders(Borders::BOTTOM))
            .select(select_index)
            .style(Style::default().fg(Color::White))
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::UNDERLINED),
            )
    }
}
