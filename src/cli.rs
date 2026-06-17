use crate::{
    audio::{discover_sounds_in, system_sound_directories},
    config::{Config, SoundSetting},
    fonts::FontCatalog,
    schedule::Candidate,
};
use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use inquire::{Confirm, Select, Text};
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, IsTerminal, Write},
    path::PathBuf,
    time::Duration,
};
use thiserror::Error;

#[derive(Parser, Debug)]
#[command(name = "clck", about = "Responsive cross-platform countdown alarm")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
    #[arg(
        value_name = "DURATION",
        help = "Examples: 45, 45s, 1h30m, 1H30, 01:30:00"
    )]
    pub duration: Option<String>,
    #[arg(long, global = true)]
    pub sound: Option<PathBuf>,
    #[arg(long, global = true)]
    pub font: Option<String>,
    #[arg(long, global = true)]
    pub no_notification: bool,
    #[arg(long, short, global = true)]
    pub title: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    At {
        #[arg(value_name = "VALUE")]
        value: String,
    },
    FromText,
    Config {
        #[arg(long, conflicts_with = "reset")]
        show: bool,
        #[arg(long, conflicts_with = "show")]
        reset: bool,
    },
    Fonts,
    Sounds,
    Stopwatch,
}

pub struct ControllingTerminal {
    pub reader: BufReader<File>,
    pub writer: File,
}

pub fn stdin_is_interactive() -> bool {
    std::io::stdin().is_terminal()
}

pub fn choose_candidate(count: usize, input: &str) -> Result<usize> {
    let selected: usize = input.trim().parse().context("enter a candidate number")?;
    if selected == 0 || selected > count {
        bail!("choose a number from 1 to {count}");
    }
    Ok(selected - 1)
}

pub fn parse_confirmation(input: &str) -> Result<bool> {
    match input.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => Ok(true),
        "n" | "no" => Ok(false),
        _ => bail!("enter yes or no"),
    }
}

pub fn read_text_source<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    interactive: bool,
) -> Result<String> {
    if !interactive {
        let mut text = String::new();
        reader.read_to_string(&mut text)?;
        return Ok(text);
    }

    writeln!(
        writer,
        "Paste or enter text. Finish with a single '.' on its own line."
    )?;
    writer.flush()?;
    let mut text = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line.trim_end() == "." {
            break;
        }
        text.push_str(&line);
    }
    Ok(text)
}

pub fn select_candidate<R: BufRead, W: Write>(
    candidates: &[Candidate],
    reader: &mut R,
    writer: &mut W,
) -> Result<usize> {
    if candidates.len() == 1 {
        return Ok(0);
    }
    for (index, candidate) in candidates.iter().enumerate() {
        writeln!(
            writer,
            "{}. {} -> {}",
            index + 1,
            candidate.source,
            candidate.display_target()
        )?;
    }
    loop {
        write!(writer, "Choose an alarm time: ")?;
        writer.flush()?;
        let mut input = String::new();
        if reader.read_line(&mut input)? == 0 {
            bail!("selection input closed");
        }
        match choose_candidate(candidates.len(), &input) {
            Ok(selected) => return Ok(selected),
            Err(error) => writeln!(writer, "{error}")?,
        }
    }
}

pub fn confirm_candidate<R: BufRead, W: Write>(
    candidate: &Candidate,
    reader: &mut R,
    writer: &mut W,
) -> Result<bool> {
    writeln!(writer, "Resolved target: {}", candidate.display_target())?;
    loop {
        write!(writer, "Start this alarm? [yes/no]: ")?;
        writer.flush()?;
        let mut input = String::new();
        if reader.read_line(&mut input)? == 0 {
            bail!("confirmation input closed");
        }
        match parse_confirmation(&input) {
            Ok(confirmed) => return Ok(confirmed),
            Err(error) => writeln!(writer, "{error}")?,
        }
    }
}

#[cfg(unix)]
pub fn open_controlling_terminal() -> Option<ControllingTerminal> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()?;
    Some(ControllingTerminal {
        reader: BufReader::new(file.try_clone().ok()?),
        writer: file,
    })
}

#[cfg(windows)]
pub fn open_controlling_terminal() -> Option<ControllingTerminal> {
    let input = OpenOptions::new().read(true).open("CONIN$").ok()?;
    let output = OpenOptions::new().write(true).open("CONOUT$").ok()?;
    Some(ControllingTerminal {
        reader: BufReader::new(input),
        writer: output,
    })
}

#[derive(Clone, Debug)]
pub struct InteractiveAnswers {
    pub duration: String,
    pub title: Option<String>,
    pub font: String,
    pub notification: bool,
    pub sound: SoundSetting,
    pub save_defaults: bool,
}

#[derive(Clone, Debug)]
pub struct ValidatedInteractiveAnswers {
    pub duration: Duration,
    pub title: Option<String>,
    pub font: String,
    pub notification: bool,
    pub sound: SoundSetting,
    pub save_defaults: bool,
}

impl InteractiveAnswers {
    pub fn validate(self) -> Result<ValidatedInteractiveAnswers> {
        Ok(ValidatedInteractiveAnswers {
            duration: parse_duration(&self.duration)?,
            title: self.title,
            font: self.font,
            notification: self.notification,
            sound: self.sound,
            save_defaults: self.save_defaults,
        })
    }
}

pub fn prompt_for_alarm(config: &Config) -> Result<ValidatedInteractiveAnswers> {
    let duration = Text::new("Duration (for example 45, 1H30, 10m, or 45s):")
        .prompt()
        .context("interactive duration prompt was cancelled")?;
    let title = Text::new("Title (optional):").prompt().ok();
    let title = title.filter(|s| !s.trim().is_empty());

    let fonts: Vec<_> = FontCatalog::default().names().map(str::to_owned).collect();
    let default_font = fonts
        .iter()
        .position(|font| font == &config.font)
        .unwrap_or(0);
    let font = Select::new("ASCII font:", fonts)
        .with_starting_cursor(default_font)
        .prompt()?;
    let sounds = discover_sounds_in(&system_sound_directories())?;
    let sound_names: Vec<_> = sounds.keys().cloned().collect();
    let sound = if sound_names.is_empty() {
        SoundSetting::TerminalBell
    } else {
        let default_sound = match &config.sound {
            SoundSetting::System(saved) => sound_names
                .iter()
                .position(|name| name.eq_ignore_ascii_case(saved))
                .unwrap_or(0),
            _ => 0,
        };
        SoundSetting::System(
            Select::new("Alarm sound:", sound_names)
                .with_starting_cursor(default_sound)
                .prompt()?,
        )
    };
    let notification = Confirm::new("Show desktop notification?")
        .with_default(config.notification)
        .prompt()?;
    let save_defaults = Confirm::new("Save these settings as defaults?")
        .with_default(true)
        .prompt()?;
    InteractiveAnswers {
        duration,
        title,
        font,
        notification,
        sound,
        save_defaults,
    }
    .validate()
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid duration `{input}`; use formats such as 45, 45s, 1h30m, 1H30, or 01:30:00")]
pub struct DurationParseError {
    input: String,
}

pub fn parse_duration(input: &str) -> Result<Duration, DurationParseError> {
    let input = input.trim();
    parse_seconds(input)
        .or_else(|| parse_colon(input))
        .or_else(|| parse_units(input))
        .or_else(|| parse_compact_hours(input))
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| DurationParseError {
            input: input.to_owned(),
        })
}

fn parse_seconds(input: &str) -> Option<Duration> {
    input.parse::<u64>().ok().map(Duration::from_secs)
}

fn parse_colon(input: &str) -> Option<Duration> {
    let parts: Vec<_> = input.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let hours = parts[0].parse::<u64>().ok()?;
    let minutes = parts[1].parse::<u64>().ok()?;
    let seconds = parts[2].parse::<u64>().ok()?;
    (minutes < 60 && seconds < 60)
        .then(|| Duration::from_secs(hours * 3_600 + minutes * 60 + seconds))
}

fn parse_units(input: &str) -> Option<Duration> {
    let lower = input.to_ascii_lowercase();
    let mut number = String::new();
    let mut seconds = 0_u64;
    let mut seen = [false; 3];

    for character in lower.chars() {
        if character.is_ascii_digit() {
            number.push(character);
            continue;
        }
        let index = match character {
            'h' => 0,
            'm' => 1,
            's' => 2,
            _ => return None,
        };
        if seen[index] || number.is_empty() {
            return None;
        }
        seen[index] = true;
        let value = number.parse::<u64>().ok()?;
        seconds = seconds.checked_add(value.checked_mul([3_600, 60, 1][index])?)?;
        number.clear();
    }

    (number.is_empty() && seen.iter().any(|seen| *seen)).then(|| Duration::from_secs(seconds))
}

fn parse_compact_hours(input: &str) -> Option<Duration> {
    let lower = input.to_ascii_lowercase();
    let (hours, minutes) = lower.split_once('h')?;
    if hours.is_empty() || minutes.is_empty() || minutes.len() > 2 {
        return None;
    }
    let hours = hours.parse::<u64>().ok()?;
    let minutes = minutes.parse::<u64>().ok()?;
    (minutes < 60).then(|| Duration::from_secs(hours * 3_600 + minutes * 60))
}

#[cfg(test)]
mod tests {
    use super::{
        choose_candidate, parse_confirmation, parse_duration, Cli, Command, InteractiveAnswers,
    };
    use crate::config::SoundSetting;
    use clap::Parser;
    use std::time::Duration;

    #[test]
    fn parses_supported_duration_formats() {
        assert_eq!(parse_duration("12").unwrap(), Duration::from_secs(12));
        assert_eq!(parse_duration("45s").unwrap(), Duration::from_secs(45));
        assert_eq!(parse_duration("10m").unwrap(), Duration::from_secs(600));
        assert_eq!(parse_duration("1h30m").unwrap(), Duration::from_secs(5_400));
        assert_eq!(parse_duration("1H30").unwrap(), Duration::from_secs(5_400));
        assert_eq!(
            parse_duration("01:30:00").unwrap(),
            Duration::from_secs(5_400)
        );
    }

    #[test]
    fn rejects_invalid_duration_formats() {
        assert!(parse_duration("1H75").is_err());
        assert!(parse_duration("nonsense").is_err());
        assert!(parse_duration("00:75:00").is_err());
    }

    #[test]
    fn validates_interactive_duration() {
        let answers = InteractiveAnswers {
            duration: "1H30".into(),
            title: Some("Lunch".into()),
            font: "standard".into(),
            notification: true,
            sound: SoundSetting::System("Glass".into()),
            save_defaults: false,
        };
        let validated = answers.validate().unwrap();
        assert_eq!(validated.duration, Duration::from_secs(5_400));
        assert_eq!(validated.title.as_deref(), Some("Lunch"));
    }

    #[test]
    fn parses_scheduling_commands_with_existing_options() {
        let cli = Cli::try_parse_from(["clck", "at", "tomorrow at 9am", "--font", "box"]).unwrap();
        assert!(matches!(cli.command, Some(Command::At { .. })));
        assert_eq!(cli.font.as_deref(), Some("box"));

        let cli = Cli::try_parse_from(["clck", "from-text", "--no-notification"]).unwrap();
        assert!(matches!(cli.command, Some(Command::FromText)));
        assert!(cli.no_notification);

        let cli = Cli::try_parse_from(["clck", "10s", "--title", "Hello"]).unwrap();
        assert_eq!(cli.title.as_deref(), Some("Hello"));
        let cli = Cli::try_parse_from(["clck", "10s", "-t", "World"]).unwrap();
        assert_eq!(cli.title.as_deref(), Some("World"));
    }

    #[test]
    fn selection_and_confirmation_are_explicit() {
        assert_eq!(choose_candidate(1, "1").unwrap(), 0);
        assert_eq!(choose_candidate(3, "2").unwrap(), 1);
        assert!(choose_candidate(3, "4").is_err());
        assert!(parse_confirmation("yes").unwrap());
        assert!(!parse_confirmation("n").unwrap());
        assert!(parse_confirmation("").is_err());
    }
}
