# Verbatim

Real-time speech-to-text for Linux and macOS. Hold a hotkey to record, release to transcribe. Text is typed into your focused window and copied to clipboard.

Supports local inference via [whisper.cpp](https://github.com/ggerganov/whisper.cpp) and cloud APIs (OpenAI Whisper). GPU acceleration via CUDA, Vulkan, or Metal (macOS). Falls back to CPU when no GPU backend is enabled.

## Features

- Push-to-talk global hotkey (configurable)
- Local STT with whisper.cpp (no internet required)
- Cloud STT via OpenAI Whisper API
- Types transcription into the focused window
- Clipboard integration (with clipboard-only mode)
- GUI for settings, transcription history, model management
- SQLite-backed history with search and word count stats
- Automatic model download from HuggingFace
- TOML configuration with environment variable overrides
- GPU acceleration via CUDA, Vulkan (Linux), or Metal (macOS) with automatic CPU fallback
- Local LLM post-processing for punctuation, formatting, and emoji insertion

## Requirements

### Build Dependencies

**Linux (Debian/Ubuntu):**

Base packages (all build types):
```bash
sudo apt-get install -y \
  build-essential \
  cmake \
  clang \
  pkg-config \
  libasound2-dev \
  libxdo-dev \
  libssl-dev \
  libwebkit2gtk-4.1-dev \
  libjavascriptcoregtk-4.1-dev \
  libsoup-3.0-dev
```

Additional packages depending on your GPU build type:

| Build type | Command | Extra packages |
|---|---|---|
| CPU (default) | `cargo tauri build` | *(none)* |
| CUDA (NVIDIA) | `cargo tauri build --features cuda` | `nvidia-cuda-toolkit` |
| Vulkan (NVIDIA + AMD) | `cargo tauri build --features vulkan` | `libvulkan-dev glslc` |
| ROCm (AMD) | `cargo tauri build --features rocm` | `rocm-dev hipblas-dev` |

```bash
# CUDA (NVIDIA) — best performance on NVIDIA GPUs
sudo apt-get install -y nvidia-cuda-toolkit

# Vulkan — portable GPU backend for NVIDIA + AMD
sudo apt-get install -y libvulkan-dev glslc

# ROCm (AMD) — native AMD GPU backend
sudo apt-get install -y rocm-dev hipblas-dev
```

Optional (for PipeWire audio backend):
```bash
sudo apt-get install -y libpipewire-0.3-dev
```

**macOS:**

```bash
xcode-select --install
brew install cmake
```

**Rust toolchain** (1.75+ required):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

**Tauri CLI:**

```bash
cargo install tauri-cli
```

**Node.js** (18+ required for the UI):

```bash
# Install via your preferred method (nvm, apt, etc.)
cd ui && npm install
```

### Runtime Requirements

**Linux:**

- The user must be in the `input` group for the global hotkey to work:
  ```bash
  sudo usermod -aG input $USER
  ```
  Log out and back in after running this.

- A working audio input device (PulseAudio or PipeWire)

- **GPU drivers** depend on how the app was built:

  | Build type | Runtime dependencies | Notes |
  |---|---|---|
  | Default (Vulkan) | `libvulkan1` + GPU drivers | NVIDIA: `nvidia-driver-*`; AMD: `mesa-vulkan-drivers` |
  | CUDA (NVIDIA) | NVIDIA drivers (`nvidia-driver-*`) | CUDA toolkit is **not** needed at runtime (statically linked) |
  | ROCm (AMD) | ROCm runtime + AMD drivers | `rocm-libs` or equivalent |

  No GPU? The app runs on CPU automatically — no configuration needed.

**macOS:**

- Accessibility permissions for keyboard simulation (System Settings > Privacy & Security > Accessibility)
- Microphone permissions

## Building

### Development

```bash
git clone https://github.com/yourname/verbatim-linux.git
cd verbatim-linux
cd ui && npm install && cd ..
cargo tauri dev
```

This starts the Vite dev server with hot reload for the React UI and launches the Tauri app pointing to it.

### Release Build

```bash
cargo tauri build
```

The binary is at `src-tauri/target/release/verbatim`. Bundled `.deb` and `.appimage` packages are in `src-tauri/target/release/bundle/`.

### Verbose Builds

GPU builds (CUDA, Vulkan) can take a while with no visible output. Use verbose flags to see progress:

```bash
cargo tauri build -v                # Tauri verbose
cargo tauri build -v -- -vv         # Tauri + Cargo verbose (shows each compiler invocation)
cargo build --release -vv           # Cargo only, max verbosity (no bundling)
```

### Build Core Library Only

```bash
cargo build -p verbatim-core
```

### Run Tests

```bash
cargo test
```

## Installation

### Homebrew (macOS & Linux)

```bash
brew tap mkamran67/verbatim
brew install verbatim                  # macOS (Metal) or Linux CPU
brew install verbatim --with-cuda      # Linux NVIDIA (CUDA)
brew install verbatim --with-vulkan    # Linux NVIDIA + AMD (Vulkan)
```

To switch GPU variant later:
```bash
brew reinstall verbatim --with-cuda
```

### Debian Package

```bash
cargo tauri build
sudo dpkg -i src-tauri/target/release/bundle/deb/verbatim_*.deb
```

## Usage

### Launch the App

```bash
cargo tauri dev
```

This opens the Verbatim window with a sidebar for navigating between Dashboard, Recordings, Word Count, Settings, and API Keys & Models pages.

### First Run

1. Launch the app — a default config is created automatically on first run.
2. Go to **API Keys & Models** and download a whisper model (e.g. `base.en`).
3. Hold **Right Ctrl** (default hotkey) to record, release to transcribe.

### Configuration

Config file location: `~/.config/verbatim/config.toml`

```toml
[general]
backend = "whisper-local"    # "whisper-local" or "openai"
language = "en"
clipboard_only = false       # true = clipboard only, don't type
hotkey = "KEY_RIGHTCTRL"

[whisper]
model = "base.en"
model_dir = "~/.local/share/verbatim/models"
threads = 0                  # 0 = auto-detect

[openai]
api_key = ""                 # or set OPENAI_API_KEY env var

[deepgram]
api_key = ""                 # or set DEEPGRAM_API_KEY env var

[google]
credentials_path = ""        # or set GOOGLE_APPLICATION_CREDENTIALS env var

[audio]
device = ""                  # empty = default input device

[input]
method = "auto"              # "auto" or "enigo"
```

Settings can also be changed in the GUI via the Settings and API Keys pages.

### Environment Variables

API keys can be set via environment variables instead of the config file:

```bash
export OPENAI_API_KEY="sk-..."
export DEEPGRAM_API_KEY="dg-..."
export GOOGLE_APPLICATION_CREDENTIALS="/path/to/credentials.json"
```

### Available Hotkeys

```
KEY_RIGHTCTRL, KEY_LEFTCTRL, KEY_RIGHTALT, KEY_LEFTALT,
KEY_RIGHTSHIFT, KEY_LEFTSHIFT, KEY_F1-KEY_F12,
KEY_CAPSLOCK, KEY_SCROLLLOCK, KEY_PAUSE, KEY_INSERT
```

### Whisper Models

| Model | Size | Speed | Quality |
|---|---|---|---|
| tiny / tiny.en | ~75 MB | Fastest | Lower |
| base / base.en | ~142 MB | Fast | Good |
| small / small.en | ~466 MB | Moderate | Better |
| medium / medium.en | ~1.5 GB | Slow | High |
| large-v3 | ~2.9 GB | Slowest | Highest |

`.en` variants are English-only and faster/more accurate for English.

### GPU Acceleration

GPU backends are **mutually exclusive** feature flags. The default build is CPU-only.

| Platform | Backend | Flag | GPUs Supported |
|----------|---------|------|----------------|
| Linux | CPU *(default)* | *(none)* | — |
| Linux | CUDA | `--features cuda` | NVIDIA |
| Linux | Vulkan | `--features vulkan` | NVIDIA + AMD |
| Linux | ROCm | `--features rocm` | AMD |
| macOS | Metal *(automatic)* | *(none)* | Apple Silicon / Intel |

You can check GPU status in **Settings > Debug > GPU Status**.

**CUDA vs Vulkan on NVIDIA hardware:**

| Component | Vulkan | CUDA |
|-----------|--------|------|
| STT (Whisper) | GPU | GPU |
| LLM post-processing | CPU | GPU |

CUDA gives full GPU acceleration for both components and is ~6x faster on NVIDIA hardware. Vulkan's LLM runs on CPU because Vulkan conflicts with WebKitGTK's GPU compositor on Linux.

**Building with CUDA (NVIDIA)**

```bash
sudo apt install nvidia-cuda-toolkit

cargo tauri dev --features cuda       # Development
cargo tauri build --features cuda     # Release
```

**Building with Vulkan (NVIDIA + AMD)**

```bash
sudo apt install libvulkan-dev glslc

cargo tauri dev --features vulkan     # Development
cargo tauri build --features vulkan   # Release
```

**Building with ROCm (AMD)**

```bash
cargo tauri build --features rocm     # Requires ROCm/HIP SDK
```

### Logging

Control log verbosity with `RUST_LOG`:

```bash
RUST_LOG=debug verbatim          # verbose
RUST_LOG=warn verbatim           # quiet
RUST_LOG=verbatim=debug verbatim # debug only for verbatim code
```

## Releasing

### Versioning

Update the version in `Cargo.toml`:

```toml
[package]
version = "X.Y.Z"
```

### Build Release Artifacts

```bash
cargo tauri build
# Binary: src-tauri/target/release/verbatim
# .deb:   src-tauri/target/release/bundle/deb/verbatim_X.Y.Z_amd64.deb
```

### GitHub Release

1. Tag the release:
   ```bash
   git tag -a vX.Y.Z -m "Release vX.Y.Z"
   git push origin vX.Y.Z
   ```

2. Create a GitHub release with the tag, attaching:
   - `target/release/verbatim` (Linux binary)
   - `target/debian/verbatim_X.Y.Z_amd64.deb` (Debian package)

### Cross-Compilation

For building on one platform and targeting another, use [cross](https://github.com/cross-rs/cross):

```bash
cargo install cross
cross build --release --target x86_64-unknown-linux-gnu
cross build --release --target aarch64-unknown-linux-gnu
```

## Project Structure

```
verbatim-linux/
├── Cargo.toml              # Workspace root
├── verbatim-core/          # Rust library — STT, audio, config, DB, hotkey, input
├── src-tauri/              # Tauri app — IPC commands, window, packaging
│   └── src/
│       ├── main.rs         # Entry point, spawns STT service, forwards events
│       ├── commands.rs     # Tauri IPC command handlers
│       └── state.rs        # Shared app state
├── ui/                     # React frontend (Vite + Tailwind CSS)
│   └── src/
│       ├── lib/tauri.ts    # Typed Tauri invoke() wrappers
│       ├── lib/types.ts    # TypeScript types matching Rust structs
│       ├── pages/          # Dashboard, Recordings, Word Count, Settings, API Keys
│       └── components/     # Layout, Sidebar, TopBar
└── assets/                 # Desktop entry file
```

See [Summary.md](Summary.md) for a detailed architecture overview.

## License

MIT
