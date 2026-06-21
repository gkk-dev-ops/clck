use crate::fonts::{FontCatalog, Size};
use anyhow::Result;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute, queue,
    style::Print,
    terminal::{
        self, disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use std::{
    io::{self, Write},
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayEvent {
    None,
    Cancel,
    Dismiss,
    Resize,
    TogglePause,
    Restart,
}

fn key_matches(key: KeyEvent, spec: &str) -> bool {
    let (required_mods, ch_str) = if let Some(rest) = spec.strip_prefix("ctrl+") {
        (KeyModifiers::CONTROL, rest)
    } else {
        (KeyModifiers::NONE, spec)
    };
    let Some(c) = ch_str.chars().next() else {
        return false;
    };
    key.code == KeyCode::Char(c) && key.modifiers == required_mods
}

pub fn event_from_key(key: KeyEvent, ringing: bool, restart_key: Option<&str>) -> DisplayEvent {
    if ringing {
        return DisplayEvent::Dismiss;
    }
    if let Some(rk) = restart_key {
        if key_matches(key, rk) {
            return DisplayEvent::Restart;
        }
    }
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => DisplayEvent::Cancel,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => DisplayEvent::Cancel,
        KeyCode::Char(' ') => DisplayEvent::TogglePause,
        _ => DisplayEvent::None,
    }
}

pub struct TerminalSession;

impl TerminalSession {
    pub fn enter() -> Result<Self> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, Hide)?;
        Ok(Self)
    }

    pub fn next_event(
        timeout: Duration,
        ringing: bool,
        restart_key: Option<&str>,
    ) -> Result<DisplayEvent> {
        if !event::poll(timeout)? {
            return Ok(DisplayEvent::None);
        }
        Ok(match event::read()? {
            Event::Key(key) => event_from_key(key, ringing, restart_key),
            Event::Resize(_, _) => DisplayEvent::Resize,
            _ => DisplayEvent::None,
        })
    }

    pub fn render_countdown(
        &self,
        remaining: Duration,
        preferred_font: &str,
        sound: &str,
        target: Option<&str>,
        title: Option<&str>,
        paused: bool,
    ) -> Result<()> {
        let (width, height) = terminal::size()?;
        let text = format_duration(remaining);
        let catalog = FontCatalog::default();
        let available = Size::new(width.saturating_sub(2), height.saturating_sub(4));
        let lines = catalog
            .largest_fit_preferring(preferred_font, &text, available)
            .map(|font| font.render(&text))
            .unwrap_or_else(|| vec![text]);

        let full_status = countdown_status(sound, target, title, paused);
        let title_status = countdown_status(sound, None, title, paused);
        let target_status = countdown_status(sound, target, None, paused);
        let compact_status = countdown_status(sound, None, None, paused);

        let status = if full_status.chars().count() <= usize::from(width) {
            Some(full_status)
        } else if title_status.chars().count() <= usize::from(width) {
            Some(title_status)
        } else if target_status.chars().count() <= usize::from(width) {
            Some(target_status)
        } else if compact_status.chars().count() <= usize::from(width) {
            Some(compact_status)
        } else {
            None
        };
        render_lines(&lines, status.as_deref())
    }

    pub fn render_stopwatch(
        &self,
        elapsed: Duration,
        preferred_font: &str,
        title: Option<&str>,
        paused: bool,
        restart_key: &str,
    ) -> Result<()> {
        let (width, height) = terminal::size()?;
        let text = format_elapsed(elapsed);
        let catalog = FontCatalog::default();
        let available = Size::new(width.saturating_sub(2), height.saturating_sub(4));
        let lines = catalog
            .largest_fit_preferring(preferred_font, &text, available)
            .map(|font| font.render(&text))
            .unwrap_or_else(|| vec![text]);

        let status = stopwatch_status(title, paused, restart_key);
        let status = if status.chars().count() <= usize::from(width) {
            Some(status)
        } else {
            None
        };
        render_lines(&lines, status.as_deref())
    }

    pub fn render_ringing(&self, target: Option<&str>, title: Option<&str>) -> Result<()> {
        let status = match (title, target) {
            (Some(title), Some(target)) => {
                format!("Title: {title} | Target: {target} | Press any key to dismiss")
            }
            (Some(title), None) => format!("Title: {title} | Press any key to dismiss"),
            (None, Some(target)) => format!("Target: {target} | Press any key to dismiss"),
            (None, None) => "Press any key to dismiss".to_owned(),
        };
        render_lines(&["TIME IS UP!".to_owned()], Some(&status))
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

fn render_lines(lines: &[String], status: Option<&str>) -> Result<()> {
    let mut stdout = io::stdout();
    queue!(stdout, Clear(ClearType::All), MoveTo(0, 0))?;
    for line in lines {
        queue!(stdout, Print(line), Print("\r\n"))?;
    }
    if let Some(status) = status {
        queue!(stdout, Print("\r\n"), Print(status))?;
    }
    stdout.flush()?;
    Ok(())
}

pub fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs() + u64::from(duration.subsec_nanos() > 0);
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

pub fn format_elapsed(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let secs = seconds % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes:02}:{secs:02}")
    }
}

pub fn stopwatch_status(title: Option<&str>, paused: bool, restart_key: &str) -> String {
    let mut parts = Vec::new();
    if let Some(title) = title {
        parts.push(format!("Title: {title}"));
    }
    if paused {
        parts.push(format!(
            "PAUSED | Space to resume | {restart_key} to restart | q/Esc/Ctrl+C to stop"
        ));
    } else {
        parts.push(format!(
            "Space to pause | {restart_key} to restart | q/Esc/Ctrl+C to stop"
        ));
    }
    parts.join(" | ")
}

pub fn countdown_status(
    sound: &str,
    target: Option<&str>,
    title: Option<&str>,
    paused: bool,
) -> String {
    let mut parts = Vec::new();
    if let Some(title) = title {
        parts.push(format!("Title: {title}"));
    }
    if let Some(target) = target {
        parts.push(format!("Target: {target}"));
    }
    parts.push(format!("Sound: {sound}"));
    if paused {
        parts.push("PAUSED | Space to resume | q/Esc/Ctrl+C to cancel".to_owned());
    } else {
        parts.push("Space to pause | q/Esc/Ctrl+C to cancel".to_owned());
    }
    parts.join(" | ")
}

#[cfg(test)]
mod tests {
    use super::{
        countdown_status, event_from_key, format_elapsed, key_matches, stopwatch_status,
        DisplayEvent,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::time::Duration;

    #[test]
    fn maps_countdown_cancel_keys() {
        assert_eq!(
            event_from_key(
                KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
                false,
                None
            ),
            DisplayEvent::Cancel
        );
        assert_eq!(
            event_from_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), false, None),
            DisplayEvent::Cancel
        );
        assert_eq!(
            event_from_key(
                KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
                true,
                None
            ),
            DisplayEvent::Dismiss
        );
        assert_eq!(
            event_from_key(
                KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
                false,
                None
            ),
            DisplayEvent::TogglePause
        );
    }

    #[test]
    fn restart_key_fires_restart_event() {
        assert_eq!(
            event_from_key(
                KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
                false,
                Some("r")
            ),
            DisplayEvent::Restart
        );
        assert_eq!(
            event_from_key(
                KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
                false,
                Some("ctrl+r")
            ),
            DisplayEvent::Restart
        );
        assert_eq!(
            event_from_key(
                KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
                false,
                None
            ),
            DisplayEvent::None
        );
    }

    #[test]
    fn key_matches_plain_and_ctrl() {
        assert!(key_matches(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
            "r"
        ));
        assert!(!key_matches(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
            "r"
        ));
        assert!(key_matches(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
            "ctrl+r"
        ));
        assert!(!key_matches(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
            "ctrl+r"
        ));
    }

    #[test]
    fn format_elapsed_floors_to_whole_seconds() {
        assert_eq!(format_elapsed(Duration::from_secs(0)), "00:00");
        assert_eq!(format_elapsed(Duration::from_millis(999)), "00:00");
        assert_eq!(format_elapsed(Duration::from_secs(65)), "01:05");
        assert_eq!(format_elapsed(Duration::from_secs(3661)), "01:01:01");
    }

    #[test]
    fn stopwatch_status_reflects_pause_state() {
        assert_eq!(
            stopwatch_status(None, false, "r"),
            "Space to pause | r to restart | q/Esc/Ctrl+C to stop"
        );
        assert_eq!(
            stopwatch_status(None, true, "r"),
            "PAUSED | Space to resume | r to restart | q/Esc/Ctrl+C to stop"
        );
        assert_eq!(
            stopwatch_status(Some("Build"), false, "r"),
            "Title: Build | Space to pause | r to restart | q/Esc/Ctrl+C to stop"
        );
        assert_eq!(
            stopwatch_status(None, false, "ctrl+r"),
            "Space to pause | ctrl+r to restart | q/Esc/Ctrl+C to stop"
        );
    }

    #[test]
    fn countdown_status_includes_optional_target_and_title() {
        assert_eq!(
            countdown_status("Glass", Some("2026-06-11 09:00 EDT"), Some("Lunch"), false),
            "Title: Lunch | Target: 2026-06-11 09:00 EDT | Sound: Glass | Space to pause | q/Esc/Ctrl+C to cancel"
        );
        assert_eq!(
            countdown_status("Glass", Some("2026-06-11 09:00 EDT"), None, false),
            "Target: 2026-06-11 09:00 EDT | Sound: Glass | Space to pause | q/Esc/Ctrl+C to cancel"
        );
        assert_eq!(
            countdown_status("Glass", None, Some("Lunch"), false),
            "Title: Lunch | Sound: Glass | Space to pause | q/Esc/Ctrl+C to cancel"
        );
        assert_eq!(
            countdown_status("Glass", None, None, false),
            "Sound: Glass | Space to pause | q/Esc/Ctrl+C to cancel"
        );
        assert_eq!(
            countdown_status("Glass", None, None, true),
            "Sound: Glass | PAUSED | Space to resume | q/Esc/Ctrl+C to cancel"
        );
    }
}
