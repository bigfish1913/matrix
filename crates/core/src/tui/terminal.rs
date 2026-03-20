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

/// Guard that restores terminal on drop
pub struct TerminalGuard {
    terminal: Option<MatrixTerminal>,
}

impl TerminalGuard {
    pub fn new(terminal: MatrixTerminal) -> Self {
        Self {
            terminal: Some(terminal),
        }
    }

    pub fn get_mut(&mut self) -> &mut MatrixTerminal {
        self.terminal.as_mut().unwrap()
    }

    pub fn into_inner(mut self) -> MatrixTerminal {
        self.terminal.take().unwrap()
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if let Some(mut term) = self.terminal.take() {
            // Try to restore terminal - ignore errors since we're in drop
            let _ = disable_raw_mode();
            let _ = execute!(
                term.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            );
            let _ = term.show_cursor();
        }
    }
}

/// Initialize the terminal for TUI mode
pub fn init_terminal() -> Result<MatrixTerminal> {
    enable_raw_mode().map_err(|e| Error::Config(format!("Failed to enable raw mode: {}", e)))?;

    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .map_err(|e| Error::Config(format!("Failed to enter alternate screen: {}", e)))?;

    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend).map_err(|e| Error::Config(format!("Failed to create terminal: {}", e)))
}

/// Restore the terminal to normal mode
pub fn restore_terminal(mut terminal: MatrixTerminal) -> Result<()> {
    disable_raw_mode().map_err(|e| Error::Config(format!("Failed to disable raw mode: {}", e)))?;

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
fn keycode_to_key(code: KeyCode, modifiers: KeyModifiers) -> Option<Key> {
    // Handle Ctrl+C and other Ctrl combinations first
    if modifiers.contains(KeyModifiers::CONTROL) {
        match code {
            KeyCode::Char('c') => return None, // Signal to quit
            KeyCode::Char('q') => return None, // Signal to quit
            _ => {}
        }
    }

    match code {
        KeyCode::Tab => {
            if modifiers.contains(KeyModifiers::SHIFT) {
                Some(Key::BackTab)
            } else {
                Some(Key::Tab)
            }
        }
        KeyCode::Backspace => Some(Key::Backspace),
        KeyCode::Left => Some(Key::Left),
        KeyCode::Right => Some(Key::Right),
        KeyCode::Up => Some(Key::Up),
        KeyCode::Down => Some(Key::Down),
        KeyCode::Esc => Some(Key::Esc),
        KeyCode::Enter => Some(Key::Enter),
        KeyCode::Char('?') => Some(Key::Question),
        KeyCode::Char(c) => Some(Key::Char(c)),
        _ => None,
    }
}

/// Create event stream for TUI
pub fn event_stream() -> impl Stream<Item = TuiEvent> {
    let tick_rate = Duration::from_millis(200);

    async_stream::stream! {
        let mut reader = EventStream::new();
        let mut tick = tokio::time::interval(tick_rate);
        let mut last_key: Option<(KeyCode, std::time::Instant)> = None;
        let key_repeat_delay = Duration::from_millis(150);

        loop {
            tokio::select! {
                // Keyboard events with timeout
                result = async {
                    tokio::time::timeout(Duration::from_millis(50), reader.next()).await
                } => {
                    match result {
                        Ok(Some(Ok(event))) => {
                            if let Event::Key(key) = event {
                                // Handle Ctrl+C to quit
                                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                                    yield TuiEvent::Key(Key::Char('q'));
                                    continue;
                                }

                                // Debounce
                                let now = std::time::Instant::now();
                                if let Some((last_code, last_time)) = last_key {
                                    if last_code == key.code && now.duration_since(last_time) < key_repeat_delay {
                                        continue;
                                    }
                                }
                                last_key = Some((key.code, now));

                                if let Some(k) = keycode_to_key(key.code, key.modifiers) {
                                    yield TuiEvent::Key(k);
                                }
                            }
                        }
                        _ => {} // Timeout or error, continue
                    }
                }

                // Tick events - always responsive
                _ = tick.tick() => {
                    yield TuiEvent::Tick;
                }
            }
        }
    }
}
