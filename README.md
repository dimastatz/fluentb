<div align="center">
<h1 align="center">fluentb</h1>
<h3>Fluentb: Your Personal Writing Superpower.</h3>
<a href="https://github.com/dimastatz/fluentb/actions/workflows/ci.yml"><img src="https://github.com/dimastatz/fluentb/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
<img src="https://img.shields.io/badge/status-early%20development-orange" alt="Status">
<img src="https://img.shields.io/badge/platform-macOS-lightgrey" alt="Platform">
<a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="License"></a>
<br>
<br>
<img src="docs/images/fluentb-bird-logo.png" width="256px" alt="fluentb logo">
</div>

## Table of contents

- [About the project](#about-the-project)
- [Features](#features)
- [Privacy](#privacy)
- [Project status](#project-status)
- [Getting started](#getting-started)
- [Usage](#usage)
- [Architecture](#architecture)
- [Roadmap](#roadmap)
- [Contributing](#contributing)
- [License](#license)

## About the project

fluentb makes you faster and clearer in every app you write in. Press a hotkey to dictate straight into any text field (Slack, Gmail, VS Code, Notes), or press another to rewrite what you've typed into clean, fluent, correctly styled text.

It is built in Rust, lives in your system tray, and stays out of your way: no main window, minimal UI, low idle resource use.

## Features

- **Dictate:** speak and have the transcription typed straight into the focused text field.
- **Rewrite:** select text you've written and replace it with a clean, fluent version in the style you choose (neutral, casual, formal, concise).
- **Works everywhere:** any app with a text field. No plugins or integrations needed.
- **Multilingual:** transcription and rewrite keep the language you write in.
- **System tray app:** shows its state (idle, recording, busy) and offers a small menu.

## Privacy

Everything runs on your machine:

- **Transcription** uses [Whisper](https://github.com/openai/whisper) through [whisper.cpp](https://github.com/ggerganov/whisper.cpp).
- **Rewrite** uses a local language model through [llama.cpp](https://github.com/ggerganov/llama.cpp).
- Your audio and text are never sent anywhere. The only network traffic is the one-time model download on first run and an optional update check.
- Logs record events and timings only, never what you said or wrote.

## Project status

fluentb is in **early development** and is not usable yet. The core interfaces and CI are in place. The state manager and macOS integration are next.

| Platform | Status    |
| -------- | --------- |
| macOS    | In progress |
| Windows  | Planned   |
| Linux    | Planned   |

## Getting started

### Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- macOS 13 or later (for running the app once the macOS integration lands)

### Build from source

```sh
git clone https://github.com/dimastatz/fluentb.git
cd fluentb
cargo build --release
```

### Run the checks

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

CI also enforces at least 95% line coverage with [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov):

```sh
cargo llvm-cov --fail-under-lines 95
```

## Usage

> These hotkeys are the planned defaults. The app can't be installed or run yet.

| Action  | Default hotkey | What it does                                              |
| ------- | -------------- | --------------------------------------------------------- |
| Dictate | `⌘⇧D`          | Press to start recording, press again to insert the text. |
| Rewrite | `⌘⇧R`          | Rewrites the selected text and replaces it.               |

On first launch, fluentb downloads the speech and rewrite models and asks for **Microphone** and **Accessibility** permissions. Accessibility access is needed to insert text into other apps.

Settings live in `~/Library/Application Support/fluentb/config.toml`:

```toml
[hotkeys]
dictate = "Cmd+Shift+D"
rewrite = "Cmd+Shift+R"

[dictation]
model = "base.en"
language = "auto"

[rewrite]
style = "neutral"
```

## Architecture

fluentb has a pure core with side effects at the edges:

- **State manager:** a pure function `next(state, event) -> (state, effects)` that decides what happens.
- **`Facade` trait** ([`src/facade.rs`](src/facade.rs)): the single boundary to the outside world, covering hotkeys, microphone, Whisper, rewrite engine, text insertion and tray.
- **Platform implementations:** one `Facade` per OS. Tests use a fake implementation, so the core is fully tested without real hardware.

## Roadmap

Each milestone has a PRD in [`docs/specs/`](docs/specs/README.md).

- [x] M0: Foundation (`Facade` trait, CI with coverage gate)
- [ ] M1: Pure state manager
- [ ] M2: Configuration
- [ ] M3: macOS dictation end to end
- [ ] M4: Tray app shell
- [ ] M5: Local rewrite
- [ ] M6: Model download and first-run onboarding
- [ ] M7: Polish and robustness
- [ ] M8: macOS distribution (signed DMG, Homebrew)
- [ ] M9: Windows support
- [ ] M10: Linux support

## Contributing

Contributions are welcome. Please open an issue to discuss larger changes first.

1. Fork the repository and create a branch.
2. Follow the conventions in [`CLAUDE.md`](CLAUDE.md): functional style, side effects behind the `Facade`, and no `unwrap`/`expect` outside tests.
3. Make sure `cargo fmt`, `cargo clippy -- -D warnings`, and `cargo test` pass, and that coverage stays at or above 95%.
4. Open a pull request.

## License

Distributed under the MIT License. See [`LICENSE`](LICENSE) for details.
