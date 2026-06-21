use crate::{
    audio::{AlarmPlayer, ResolvedSound},
    display::{DisplayEvent, TerminalSession},
    notification,
    timer::{Countdown, Stopwatch},
};
use anyhow::Result;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct AlarmRequest {
    pub duration: Duration,
    pub title: Option<String>,
    pub font: String,
    pub sound_name: String,
    pub sound: ResolvedSound,
    pub notification: bool,
    pub target: Option<String>,
}

#[derive(Clone, Debug)]
pub struct StopwatchRequest {
    pub title: Option<String>,
    pub font: String,
    pub restart_key: String,
}

pub fn run_stopwatch(request: StopwatchRequest) -> Result<Duration> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&cancelled);
    ctrlc::set_handler(move || signal.store(true, Ordering::SeqCst))?;

    let terminal = TerminalSession::enter()?;
    let mut watch = Stopwatch::new(Instant::now());
    let mut displayed_second = None;
    let mut last_paused = false;
    loop {
        let now = Instant::now();
        let elapsed = watch.elapsed(now);
        let paused = watch.is_paused();
        let current_second = elapsed.as_secs();
        if displayed_second != Some(current_second) || paused != last_paused {
            terminal.render_stopwatch(
                elapsed,
                &request.font,
                request.title.as_deref(),
                paused,
                &request.restart_key,
            )?;
            displayed_second = Some(current_second);
            last_paused = paused;
        }
        if cancelled.load(Ordering::SeqCst) {
            break;
        }
        match TerminalSession::next_event(
            Duration::from_millis(100),
            false,
            Some(&request.restart_key),
        )? {
            DisplayEvent::Cancel => break,
            DisplayEvent::Resize => displayed_second = None,
            DisplayEvent::Restart => {
                let now = Instant::now();
                watch = Stopwatch::new(now);
                displayed_second = None;
            }
            DisplayEvent::TogglePause => {
                let now = Instant::now();
                if watch.is_paused() {
                    watch.resume(now);
                } else {
                    watch.pause(now);
                }
                displayed_second = None;
            }
            _ => {}
        }
    }
    drop(terminal);
    Ok(watch.elapsed(Instant::now()))
}

pub fn run_alarm(request: AlarmRequest) -> Result<()> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&cancelled);
    ctrlc::set_handler(move || signal.store(true, Ordering::SeqCst))?;

    let terminal = TerminalSession::enter()?;
    let mut timer = Countdown::new(Instant::now(), request.duration);
    let mut displayed_second = None;
    let mut last_paused = false;
    loop {
        let now = Instant::now();
        let remaining = timer.remaining(now);
        let paused = timer.is_paused();
        let current_second = remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0);
        if displayed_second != Some(current_second) || paused != last_paused {
            terminal.render_countdown(
                remaining,
                &request.font,
                &request.sound_name,
                request.target.as_deref(),
                request.title.as_deref(),
                paused,
            )?;
            displayed_second = Some(current_second);
            last_paused = paused;
        }
        if !paused && timer.is_finished(Instant::now()) {
            break;
        }
        if cancelled.load(Ordering::SeqCst) {
            return Ok(());
        }
        match TerminalSession::next_event(Duration::from_millis(100), false, None)? {
            DisplayEvent::Cancel => return Ok(()),
            DisplayEvent::Resize => displayed_second = None,
            DisplayEvent::TogglePause => {
                let now = Instant::now();
                if timer.is_paused() {
                    timer.resume(now);
                } else {
                    timer.pause(now);
                }
                displayed_second = None;
            }
            _ => {}
        }
    }

    if request.notification {
        let _ = notification::notify_time_up(request.target.as_deref(), request.title.as_deref());
    }
    terminal.render_ringing(request.target.as_deref(), request.title.as_deref())?;
    let player = AlarmPlayer::start(&request.sound)?;
    let mut last_bell = Instant::now();
    loop {
        if cancelled.load(Ordering::SeqCst)
            || TerminalSession::next_event(Duration::from_millis(100), true, None)?
                == DisplayEvent::Dismiss
        {
            break;
        }
        if last_bell.elapsed() >= Duration::from_secs(1) {
            player.bell();
            last_bell = Instant::now();
        }
    }
    Ok(())
}
