pub mod app;
pub mod audio;
pub mod cli;
pub mod config;
pub mod display;
pub mod fonts;
pub mod notification;
pub mod schedule;
pub mod timer;

use anyhow::{bail, Context, Result};
use chrono::Local;
use clap::Parser;
use config::{Config, Overrides, SoundSetting};
use inquire::Confirm;
use schedule::Candidate;
use std::{
    io::{BufRead, BufReader, Write},
    time::Duration,
};

pub const APP_NAME: &str = "clck";

pub fn run() -> Result<()> {
    let cli = cli::Cli::parse();
    let config = Config::load()?;
    let command_config = config.resolve(Overrides {
        font: cli.font.clone(),
        notification: cli.no_notification.then_some(false),
        sound: cli.sound.clone().map(SoundSetting::File),
    });

    if let Some(command) = cli.command {
        match command {
            cli::Command::At { value } => {
                return run_direct_schedule(value, command_config, cli.title)
            }
            cli::Command::FromText => return run_text_schedule(command_config, cli.title),
            cli::Command::Fonts => {
                let catalog = fonts::FontCatalog::default();
                for name in catalog.names() {
                    println!("{name}");
                    if let Some(font) = catalog.by_name(name) {
                        for line in font.render("01:30") {
                            println!("{line}");
                        }
                    }
                    println!();
                }
            }
            cli::Command::Sounds => {
                for (name, path) in audio::discover_sounds_in(&audio::system_sound_directories())? {
                    println!("{name}: {}", path.display());
                }
            }
            cli::Command::Stopwatch => {
                let elapsed = app::run_stopwatch(app::StopwatchRequest {
                    title: cli.title,
                    font: command_config.font,
                })?;
                println!("Elapsed: {}", display::format_elapsed(elapsed));
                return Ok(());
            }
            cli::Command::Config { show, reset } => {
                if reset {
                    if Confirm::new("Reset saved clck settings?")
                        .with_default(false)
                        .prompt()?
                    {
                        Config::default().save()?;
                        println!("Configuration reset.");
                    }
                } else if show {
                    println!("{}", toml::to_string_pretty(&config)?);
                } else {
                    println!("Configuration: {}", Config::path()?.display());
                    println!("{}", toml::to_string_pretty(&config)?);
                }
            }
        }
        return Ok(());
    }

    let (duration, effective, title) = if let Some(duration) = cli.duration {
        let duration = cli::parse_duration(&duration)?;
        (duration, command_config, cli.title)
    } else {
        let answers = cli::prompt_for_alarm(&config)?;
        let effective = Config {
            font: answers.font,
            notification: answers.notification,
            sound: answers.sound,
        };
        if answers.save_defaults {
            effective.save()?;
        }
        (answers.duration, effective, answers.title)
    };

    app::run_alarm(build_alarm_request(duration, effective, None, title)?).context("alarm failed")
}

fn run_direct_schedule(
    mut expression: String,
    effective: Config,
    title: Option<String>,
) -> Result<()> {
    let Some(mut terminal) = cli::open_controlling_terminal() else {
        let candidate = schedule::parse_direct(&expression)?;
        bail!(
            "confirmation is required before starting an alarm; detected candidate: {} -> {}",
            candidate.source,
            candidate.display_target()
        );
    };

    let mut had_error = false;
    let candidate = loop {
        match schedule::parse_direct(&expression) {
            Ok(candidate) => break candidate,
            Err(error) => {
                had_error = true;
                writeln!(terminal.writer, "{error}")?;
                write!(terminal.writer, "Enter another date and time: ")?;
                terminal.writer.flush()?;
                expression.clear();
                if terminal.reader.read_line(&mut expression)? == 0 {
                    bail!("date and time input closed");
                }
            }
        }
    };
    if had_error {
        if !cli::confirm_candidate(&candidate, &mut terminal.reader, &mut terminal.writer)? {
            return Ok(());
        }
    } else {
        writeln!(
            terminal.writer,
            "Starting alarm for {}.",
            candidate.display_target()
        )?;
    }
    drop(terminal);
    start_scheduled_alarm(candidate, effective, title)
}

fn run_text_schedule(effective: Config, title: Option<String>) -> Result<()> {
    if cli::stdin_is_interactive() {
        let stdin = std::io::stdin();
        let mut reader = stdin.lock();
        let mut writer = std::io::stdout();
        loop {
            let text = cli::read_text_source(&mut reader, &mut writer, true)?;
            let candidates = schedule::extract_candidates(&text)?;
            if candidates.is_empty() {
                writeln!(
                    writer,
                    "No explicit date and time found; try `2:50pm`, `tomorrow at 9am`, or `June 12 at 09:00`."
                )?;
                continue;
            }
            let selected = cli::select_candidate(&candidates, &mut reader, &mut writer)?;
            let candidate = candidates[selected].clone();
            writeln!(writer, "Starting alarm for {}.", candidate.display_target())?;
            return start_scheduled_alarm(candidate, effective, title);
        }
    }

    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let mut sink = std::io::sink();
    let text = cli::read_text_source(&mut reader, &mut sink, false)?;
    let candidates = schedule::extract_candidates(&text)?;
    if candidates.is_empty() {
        bail!(
            "no explicit date and time found; try `2:50pm`, `tomorrow at 9am`, or `June 12 at 09:00`"
        );
    }
    let Some(mut terminal) = cli::open_controlling_terminal() else {
        let detected = candidates
            .iter()
            .map(|candidate| format!("{} -> {}", candidate.source, candidate.display_target()))
            .collect::<Vec<_>>()
            .join("\n");
        bail!("confirmation is required before starting an alarm; rerun interactively\n{detected}");
    };
    let selected = cli::select_candidate(&candidates, &mut terminal.reader, &mut terminal.writer)?;
    let candidate = candidates[selected].clone();
    writeln!(
        terminal.writer,
        "Starting alarm for {}.",
        candidate.display_target()
    )?;
    drop(terminal);
    start_scheduled_alarm(candidate, effective, title)
}

fn start_scheduled_alarm(
    candidate: Candidate,
    effective: Config,
    title: Option<String>,
) -> Result<()> {
    let duration = schedule::duration_until(Local::now().fixed_offset(), candidate.target)?;
    app::run_alarm(build_alarm_request(
        duration,
        effective,
        Some(candidate.display_target()),
        title,
    )?)
    .context("alarm failed")
}

fn build_alarm_request(
    duration: Duration,
    effective: Config,
    target: Option<String>,
    title: Option<String>,
) -> Result<app::AlarmRequest> {
    let (sound_name, sound) = match &effective.sound {
        SoundSetting::System(name) => (name.clone(), audio::resolve_system_sound(name)?),
        SoundSetting::File(path) => (
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("custom sound")
                .to_owned(),
            audio::resolve_custom_sound(path)?,
        ),
        SoundSetting::TerminalBell => ("terminal bell".into(), audio::ResolvedSound::TerminalBell),
    };
    if duration.is_zero() {
        bail!("duration must be greater than zero");
    }

    Ok(app::AlarmRequest {
        duration,
        title,
        font: effective.font,
        sound_name,
        sound,
        notification: effective.notification,
        target,
    })
}

#[cfg(test)]
mod tests {
    use super::build_alarm_request;
    use crate::config::Config;
    use std::time::Duration;

    #[test]
    fn package_name_is_stable() {
        assert_eq!(super::APP_NAME, "clck");
    }

    #[test]
    fn scheduled_request_keeps_target_metadata() {
        let request = build_alarm_request(
            Duration::from_secs(60),
            Config::default(),
            Some("2026-06-11 09:00:00 -04:00 (America/New_York)".into()),
            Some("Resuming".into()),
        )
        .unwrap();
        assert_eq!(
            request.target.as_deref(),
            Some("2026-06-11 09:00:00 -04:00 (America/New_York)")
        );
        assert_eq!(request.title.as_deref(), Some("Resuming"));
    }
}
