# Verbatim — Project Summary

## Overview

Verbatim is a cross-platform (Linux/macOS) speech-to-text desktop application built in Rust. It captures microphone input via a push-to-talk global hotkey and transcribes speech using either a local whisper.cpp model or cloud APIs. Transcribed text is typed into the focused window and/or copied to the clipboard.

## Architecture

### Thread Model

- **Main thread**: iced GUI event loop
- **STT runtime thread**: Dedicated tokio runtime running the `SttService` (hotkey listening, audio capture, transcription, typing/clipboard)
- **evdev thread** (Linux): Dedicated OS thread for global hotkey polling via `/dev/input`
- **cpal callback thread**: Audio capture managed by the cpal library

### Communication

The GUI and STT service are decoupled via channels:

- `SttEvent` (service → GUI): state changes, transcription results, errors
- `SttCommand` (GUI → service): config updates, shutdown

### State Machine

```
Idle → [hotkey pressed] → Recording → [hotkey released] → Processing → [text ready] → Idle
```

### Audio Pipeline

```
cpal mic capture (native rate, N channels)
  → Resampler (linear interpolation → 16kHz mono f32)
    → Pre-allocated ring buffer (~60s capacity)
      → On hotkey release, drain buffer → STT backend
```

## Technology Stack

| Concern | Crate | Notes |
|---|---|---|
| GUI | `iced` 0.13 | Pure Rust, cross-platform, Elm architecture |
| Audio capture | `cpal` | PipeWire/PulseAudio/CoreAudio backends |
| Local STT | `whisper-rs` | Bindings to whisper.cpp |
| Cloud STT | `reqwest` | OpenAI Whisper API (Deepgram/Google planned) |
| Database | `rusqlite` (bundled) | SQLite for transcription history |
| System tray | `tray-icon` + `muda` | Cross-platform tray (stubbed) |
| Global hotkey | `evdev` (Linux) | Reads `/dev/input` devices directly |
| Keyboard sim | `enigo` | Types text into focused window |
| Clipboard | `arboard` | Wayland data-control + X11 support |
| Config | `serde` + `toml` | `~/.config/verbatim/config.toml` |
| CLI | `clap` | Subcommands for model management |
| Async runtime | `tokio` | Multi-threaded, powers STT service |
| Logging | `tracing` | Structured, filterable via `RUST_LOG` |
| Timestamps | `chrono` | History entry timestamps |
| IDs | `uuid` v4 | Unique transcription record IDs |
| WAV encoding | `hound` | In-memory WAV for cloud API uploads |
| Packaging | `cargo-deb` | Debian `.deb` generation |

## Module Structure

```
src/
  main.rs              # CLI parsing, GUI launch, STT service spawn
  app.rs               # SttService: hotkey → audio → transcribe → output
  config.rs            # TOML config with serde defaults + env var overrides
  errors.rs            # Typed error enums (SttError, AudioError, etc.)
  clipboard.rs         # arboard clipboard wrapper
  model_manager.rs     # Whisper model download from HuggingFace
  audio/
    capture.rs         # cpal mic capture, shared buffer, device listing
    resampler.rs       # Linear interpolation resampler to 16kHz mono
  stt/
    mod.rs             # SttBackend trait (async, Send + Sync)
    whisper_local.rs   # whisper-rs inference backend
    openai.rs          # OpenAI Whisper API backend
  hotkey/
    mod.rs             # HotkeyEvent enum, platform cfg gates
    evdev_listener.rs  # Linux evdev push-to-talk (own OS thread)
  input/
    mod.rs             # InputMethod trait
    enigo_backend.rs   # Keyboard simulation via enigo
  tray/
    mod.rs             # TrayCommand/TrayState enums (implementation pending)
  db/
    mod.rs             # Database struct: open, insert, search, delete, stats
    schema.rs          # SQLite CREATE TABLE statements
  gui/
    mod.rs             # iced Application: state, update, view, subscriptions
    theme.rs           # Dark theme colors, spacing, font sizes
    views/
      dashboard.rs     # Status indicator, recent transcriptions, stats cards
      settings.rs      # Full settings form with save
      api_keys.rs      # API key inputs with visibility toggles
      history.rs       # Searchable paginated transcription history
      models.rs        # Model download/delete with progress bars
    widgets/
      sidebar.rs       # Navigation sidebar (5 pages)
      status_indicator.rs  # Colored dot for Idle/Recording/Processing
```

## GUI Pages

- **Dashboard**: Live status indicator, recent transcriptions (last 5), stats cards (today/week/total)
- **Settings**: Backend selector, language, hotkey, clipboard-only toggle, whisper model/threads, audio device, input method
- **API Keys**: OpenAI/Deepgram/Google key management with show/hide and env var fallback
- **History**: Full-text search, paginated list with timestamps/word counts, copy/delete per entry
- **Models**: View downloaded models, download new ones with progress bar, delete unused models

## Database

SQLite at `~/.local/share/verbatim/verbatim.db` with WAL mode:

- `transcriptions` table: id, text, word_count, char_count, duration_secs, backend, language, created_at, soft-delete flag
- `daily_stats` table: aggregated daily word/transcription/duration counts

## What's Implemented

- [x] Audio capture with resampling (cpal → 16kHz mono)
- [x] Local STT via whisper.cpp (whisper-rs)
- [x] OpenAI Whisper API backend
- [x] Push-to-talk hotkey via evdev (Linux)
- [x] Keyboard simulation (enigo) + clipboard output
- [x] TOML config with defaults and env var overrides
- [x] Model download from HuggingFace with progress
- [x] iced GUI with 5 pages (dashboard, settings, API keys, history, models)
- [x] SQLite transcription history with search and stats
- [x] STT ↔ GUI decoupled via channels

## What's Pending

- [ ] System tray integration (tray-icon + muda wired into iced lifecycle)
- [ ] macOS hotkey listener (CGEventTap or rdev)
- [ ] Deepgram STT backend
- [ ] Google Cloud STT backend
- [ ] Streaming upload for cloud backends (reduce latency)
- [ ] CUDA/GPU acceleration for whisper.cpp
- [ ] Single-instance enforcement (Unix socket IPC)
- [ ] Config file watching for live reload
