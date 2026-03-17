// crates/core/src/tui/terminal.rs

use crate::error::{Error, Result};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::{Stream, StreamExt};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;

use super::render::MatrixTerminal;
use super::{Key, TuiEvent};

/// Initialize the terminal for TUI mode
pub fn init_terminal() -> Result<MatrixTerminal> {
    enable_raw_mode()
        .map_err(|e| Error::Config(format!("Failed to enable raw mode: {}", e)))?;

    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .map_err(|e| Error::Config(format!("Failed to enter alternate screen: {}", e)))?;

    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
        .map_err(|e| Error::Config(format!("Failed to create terminal: {}", e)))
}

/// Restore the terminal to normal mode
pub fn restore_terminal(mut terminal: MatrixTerminal) -> Result<()> {
    disable_raw_mode()
        .map_err(|e| Error::Config(format!("Failed to disable raw mode: {}", e)))?;

    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .map_err(|e| Error::Config(format!("Failed to leave alternate screen: {}", e)))?;

    terminal
        .show_cursor()
        .map_err(|e| Error::Config(format!("Failed to show cursor: {}", e)))?;

    Ok(())
}

/// Convert crossterm event to our Key type
fn keycode_to_key(code: KeyCode, modifiers: KeyModifiers) -> Key {
    match code {
        KeyCode::Tab => {
            if modifiers.contains(KeyModifiers::SHIFT) {
                Key::BackTab
            } else {
                Key::Tab
            }
        }
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Esc => Key::Esc,
        KeyCode::Enter => Key::Enter,
        KeyCode::Char('?') => Key::Question,
        KeyCode::Char(c) => Key::Char(c),
        _ => Key::Char(' '),
    }
}

/// Create event stream for TUI
pub fn event_stream() -> impl Stream<Item = TuiEvent> {
    let tick_rate = Duration::from_millis(250);

    async_stream::stream! {
        let mut reader = EventStream::new();
        let mut tick = tokio::time::interval(tick_rate);

        loop {
            tokio::select! {
                // Keyboard events
                Some(Ok(event)) = reader.next() => {
                    if let Event::Key(key) = event {
                        yield TuiEvent::Key(keycode_to_key(key.code, key.modifiers));
                    }
                }

                // Tick events
                _ = tick.tick() => {
                    yield TuiEvent::Tick;
                }
            }
        }
    }
}