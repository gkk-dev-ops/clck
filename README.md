# clck

A responsive cross-platform countdown alarm for the terminal.

Read the full documentation at
[gkk-dev-ops.github.io/clck](https://gkk-dev-ops.github.io/clck/).

Officially supported on macOS and Linux. Windows support is best-effort.

## Demo

- [Watch the full clck demo on YouTube](https://www.youtube.com/watch?v=5Mw-9Tpej78)
- [Watch the clck Short](https://www.youtube.com/shorts/fwHKI4ZYo0w)
- [View the documentation site](https://gkk-dev-ops.github.io/clck/)
- [View clck on crates.io](https://crates.io/crates/clck)

## Installation

Cargo/crates.io is the best option for Rust users:

```bash
cargo install clck --locked
```

GitHub Releases are best for users who want prebuilt binaries. Download the
archive for your platform from the
[latest GitHub Release](https://github.com/gkk-dev-ops/clck/releases/latest),
verify it using `SHA256SUMS`, and place the extracted executable on `PATH`.

Release archives use these names:

- `clck-vX.Y.Z-macos-aarch64.tar.gz`
- `clck-vX.Y.Z-macos-x86_64.tar.gz`
- `clck-vX.Y.Z-linux-aarch64-musl.tar.gz`
- `clck-vX.Y.Z-linux-x86_64-musl.tar.gz`
- `clck-vX.Y.Z-windows-x86_64.zip`

After either installation method, the command is `clck`.

## Usage

Pass the countdown duration directly:

```bash
clck 45
clck 45s
clck 10m
clck 1h30m
clck 01:30:00
clck --help
```

Numbers without a unit are seconds. Compact durations such as `1H30` mean one
hour and thirty minutes. Duration units are case-insensitive.

Running `clck` without a duration opens guided interactive setup. Interactive
title, font, sound, and notification choices can be saved as defaults.

## Options

```bash
clck 5m --title "Pizza"
clck 5m -t "Pizza"
clck 5m --sound ~/Music/alarm.mp3
clck 5m --sound ~/Movies/alarm.mp4
clck 5m --font banner
clck 5m --no-notification
```

MP3, WAV, FLAC, OGG, and AIFF play natively. Other formats, including MP4,
require `ffmpeg` to be installed.

The default sound is resolved by logical name from installed OS sounds.
macOS defaults to `Glass`; Linux discovers freedesktop sound themes. If no
sound is available, the terminal bell is used.

## Commands

```bash
clck fonts
clck sounds
clck config --show
clck config --reset
clck --help
```

Configuration is stored in the platform-standard application config directory.
Command-line options override saved defaults.

## Controls

- During countdown: `q`, `Esc`, or `Ctrl+C` cancels.
- While ringing: any key dismisses the alarm.

The timer selects the largest bundled ASCII font that fits and redraws when
the terminal is resized. Terminal mode and cursor visibility are restored on
normal exit, cancellation, dismissal, and errors.

## Development

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
```

See [docs/manual-testing.md](docs/manual-testing.md) for platform smoke tests
and [docs/releases.md](docs/releases.md) for GitHub Release operations.
