# Releasing Verbatim

This guide covers how to create GitHub Releases and distribute Verbatim via Homebrew for both macOS and Linux.

## macOS signing setup (required for local releases)

macOS keys permission grants (Accessibility, Microphone, Input Monitoring) by the binary's code-signing Designated Requirement. Unsigned/ad-hoc builds get a fresh Designated Requirement on every build, so users must re-grant permissions on every update. To avoid this, every release must be signed with the **same Developer ID Application identity**.

**One-time setup:**

1. In your Apple Developer account, generate an app-specific password at https://appleid.apple.com (Sign-In and Security → App-Specific Passwords).
2. Find your signing identity:
   ```bash
   security find-identity -v -p codesigning
   ```
   Look for `Developer ID Application: Your Name (TEAMID)`.
3. Copy the env template and fill in your values:
   ```bash
   cp scripts/.macos-signing.env.example scripts/.macos-signing.env
   $EDITOR scripts/.macos-signing.env
   ```
   The real `.macos-signing.env` is gitignored — never commit it.

**On every release:** `scripts/build-release.sh` automatically loads the env file, signs with the Developer ID identity, applies the Hardened Runtime entitlements from `src-tauri/entitlements.plist`, notarizes via `notarytool`, and staples the ticket. After the build it runs `spctl` to verify Gatekeeper acceptance and fails the build if anything regressed.

> ⚠️ **Never change the bundle identifier (`com.verbatim.app`) or the signing identity.** Either change invalidates every existing user's permission grants — exactly the bug this signing setup is designed to prevent.

## Prerequisites

- GitHub repo: `MK-Devices/verbatim-linux`
- A separate GitHub repo for the Homebrew tap: `MK-Devices/homebrew-tap`
- A [Personal Access Token](https://github.com/settings/tokens) with `repo` scope (for the CI to push to the tap repo). Add it as a repository secret named `TAP_GITHUB_TOKEN` in `MK-Devices/verbatim-linux`.

---

## Part 1: GitHub Releases via CI

### Step 1: Create the release workflow

Create `.github/workflows/release.yml` in the `verbatim-linux` repo. This workflow triggers on version tags and builds artifacts for all platforms.

```yaml
name: Release

on:
  push:
    tags: ['v*']

env:
  CARGO_TERM_COLOR: always

jobs:
  build-macos-arm64:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: '20'
      - uses: dtolnay/rust-toolchain@stable
      - run: brew install cmake
      - run: cd ui && npm ci
      - run: cargo install tauri-cli --locked
      - run: cargo tauri build
      - uses: actions/upload-artifact@v4
        with:
          name: macos-arm64
          path: src-tauri/target/release/bundle/dmg/*.dmg

  build-macos-x64:
    runs-on: macos-13
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: '20'
      - uses: dtolnay/rust-toolchain@stable
      - run: brew install cmake
      - run: cd ui && npm ci
      - run: cargo install tauri-cli --locked
      - run: cargo tauri build
      - uses: actions/upload-artifact@v4
        with:
          name: macos-x64
          path: src-tauri/target/release/bundle/dmg/*.dmg

  build-linux-x64:
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: '20'
      - uses: dtolnay/rust-toolchain@stable
      - name: Install system dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y \
            build-essential cmake clang pkg-config \
            libasound2-dev libxdo-dev libssl-dev \
            libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev \
            libsoup-3.0-dev
      - run: cd ui && npm ci
      - run: cargo install tauri-cli --locked
      - run: cargo tauri build
      - uses: actions/upload-artifact@v4
        with:
          name: linux-x64
          path: |
            src-tauri/target/release/bundle/deb/*.deb
            src-tauri/target/release/bundle/appimage/*.AppImage

  create-release:
    needs: [build-macos-arm64, build-macos-x64, build-linux-x64]
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/download-artifact@v4
        with:
          merge-multiple: true
          path: artifacts
      - name: List artifacts
        run: find artifacts -type f
      - uses: softprops/action-gh-release@v2
        with:
          files: artifacts/**/*
          generate_release_notes: true
```

### Step 2: Bump the version

Before creating a release, update the version in two places:

1. **`src-tauri/Cargo.toml`** — `version = "X.Y.Z"`
2. **`src-tauri/tauri.conf.json`** — `"version": "X.Y.Z"`

Commit the version bump:

```bash
git add src-tauri/Cargo.toml src-tauri/tauri.conf.json
git commit -m "Bump version to X.Y.Z"
```

### Step 3: Tag and push

```bash
git tag -a vX.Y.Z -m "Release vX.Y.Z"
git push origin master --tags
```

The CI will automatically:
1. Build on macOS arm64, macOS x64, and Linux x64
2. Create a GitHub Release with all artifacts attached

### Release artifacts produced

| Platform | Artifact | Filename pattern |
|----------|----------|-----------------|
| macOS arm64 | DMG installer | `Verbatim_X.Y.Z_aarch64.dmg` |
| macOS x64 | DMG installer | `Verbatim_X.Y.Z_x64.dmg` |
| Linux x64 | Debian package | `verbatim_X.Y.Z_amd64.deb` |
| Linux x64 | AppImage | `verbatim_X.Y.Z_amd64.AppImage` |

---

## Part 2: Homebrew Distribution

Homebrew uses two mechanisms:
- **Cask** — for macOS GUI apps (installs `.app` from `.dmg`)
- **Formula** — for Linux binaries (installs binary from `.deb`)

### Step 4: Create the Homebrew tap repository

Create a new GitHub repo: **`MK-Devices/homebrew-tap`**

The repo structure:

```
homebrew-tap/
  Casks/
    verbatim.rb      # macOS cask
  Formula/
    verbatim.rb      # Linux formula
```

### Step 5: Create the macOS Cask

Create `Casks/verbatim.rb` in the tap repo:

```ruby
cask "verbatim" do
  version "0.1.0"

  on_arm do
    url "https://github.com/MK-Devices/verbatim-linux/releases/download/v#{version}/Verbatim_#{version}_aarch64.dmg"
    sha256 "REPLACE_WITH_ARM64_SHA256"
  end

  on_intel do
    url "https://github.com/MK-Devices/verbatim-linux/releases/download/v#{version}/Verbatim_#{version}_x64.dmg"
    sha256 "REPLACE_WITH_X64_SHA256"
  end

  name "Verbatim"
  desc "Real-time speech-to-text with push-to-talk hotkey"
  homepage "https://github.com/MK-Devices/verbatim-linux"

  app "Verbatim.app"

  zap trash: [
    "~/.config/verbatim",
    "~/.local/share/verbatim",
  ]
end
```

### Step 6: Create the Linux Formula

Create `Formula/verbatim.rb` in the tap repo:

```ruby
class Verbatim < Formula
  desc "Real-time speech-to-text with push-to-talk hotkey"
  homepage "https://github.com/MK-Devices/verbatim-linux"
  version "0.1.0"
  license "MIT"

  on_linux do
    url "https://github.com/MK-Devices/verbatim-linux/releases/download/v#{version}/verbatim_#{version}_amd64.deb"
    sha256 "REPLACE_WITH_LINUX_SHA256"
  end

  depends_on :linux

  def install
    # Extract binary from .deb archive
    safe_system "ar", "x", cached_download
    mkdir "extracted"
    if File.exist?("data.tar.xz")
      safe_system "tar", "xf", "data.tar.xz", "-C", "extracted"
    elsif File.exist?("data.tar.gz")
      safe_system "tar", "xf", "data.tar.gz", "-C", "extracted"
    end
    bin.install "extracted/usr/bin/verbatim"
    (share/"applications").install "extracted/usr/share/applications/verbatim.desktop" if File.exist?("extracted/usr/share/applications/verbatim.desktop")
  end

  def caveats
    <<~EOS
      To use global hotkeys, add your user to the input group:
        sudo usermod -aG input $USER
      Then log out and back in.

      Runtime dependencies (install if not already present):
        sudo apt-get install libayatana-appindicator3-1 libgtk-3-0 libwebkit2gtk-4.1-0
    EOS
  end
end
```

### Step 7: Compute SHA256 hashes

After the GitHub Release is created, download each artifact and compute its hash:

```bash
# Download the release artifacts
gh release download vX.Y.Z -R MK-Devices/verbatim-linux

# Compute SHA256 for each file
shasum -a 256 Verbatim_X.Y.Z_aarch64.dmg
shasum -a 256 Verbatim_X.Y.Z_x64.dmg
shasum -a 256 verbatim_X.Y.Z_amd64.deb
```

Replace the `REPLACE_WITH_*_SHA256` placeholders in both `.rb` files with the actual hashes.

### Step 8: Push the tap

```bash
cd homebrew-tap
git add Casks/verbatim.rb Formula/verbatim.rb
git commit -m "Add verbatim v0.1.0"
git push origin main
```

---

## Part 3: User Installation

Once everything is set up, users install Verbatim like this:

```bash
# Add the tap (one-time)
brew tap MK-Devices/tap

# macOS — installs the .app bundle
brew install --cask verbatim

# Linux (Linuxbrew) — installs the binary
brew install verbatim
```

To upgrade:

```bash
brew upgrade verbatim          # Linux
brew upgrade --cask verbatim   # macOS
```

---

## Part 4: Automating Tap Updates (Optional)

To avoid manually updating SHA256 hashes on every release, add a second workflow to `verbatim-linux` that runs after a release is published.

Create `.github/workflows/bump-homebrew.yml`:

```yaml
name: Update Homebrew Tap

on:
  release:
    types: [published]

jobs:
  bump-tap:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout tap repo
        uses: actions/checkout@v4
        with:
          repository: MK-Devices/homebrew-tap
          token: ${{ secrets.TAP_GITHUB_TOKEN }}

      - name: Download release assets and update formulas
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          VERSION: ${{ github.event.release.tag_name }}
        run: |
          VER="${VERSION#v}"

          # Download assets
          gh release download "$VERSION" \
            -R MK-Devices/verbatim-linux \
            -p "*.dmg" -p "*.deb" \
            -D /tmp/assets

          # Compute SHA256
          ARM64_SHA=$(shasum -a 256 /tmp/assets/Verbatim_${VER}_aarch64.dmg | awk '{print $1}')
          X64_SHA=$(shasum -a 256 /tmp/assets/Verbatim_${VER}_x64.dmg | awk '{print $1}')
          DEB_SHA=$(shasum -a 256 /tmp/assets/verbatim_${VER}_amd64.deb | awk '{print $1}')

          # Update Cask
          sed -i "s/version \".*\"/version \"${VER}\"/" Casks/verbatim.rb
          sed -i "/on_arm/,/end/{s/sha256 \".*\"/sha256 \"${ARM64_SHA}\"/}" Casks/verbatim.rb
          sed -i "/on_intel/,/end/{s/sha256 \".*\"/sha256 \"${X64_SHA}\"/}" Casks/verbatim.rb

          # Update Formula
          sed -i "s/version \".*\"/version \"${VER}\"/" Formula/verbatim.rb
          sed -i "/on_linux/,/end/{s/sha256 \".*\"/sha256 \"${DEB_SHA}\"/}" Formula/verbatim.rb

      - name: Commit and push
        run: |
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          git add -A
          git commit -m "Bump verbatim to ${VERSION}"
          git push
```

With this in place, the full release flow becomes:

1. Bump version in `Cargo.toml` + `tauri.conf.json`
2. Commit, tag `vX.Y.Z`, push
3. CI builds artifacts and creates GitHub Release
4. Tap auto-updates with correct SHA256 hashes
5. Users run `brew upgrade` to get the new version

---

## Checklist for First Release

- [ ] Create `.github/workflows/release.yml` in this repo
- [ ] Create `MK-Devices/homebrew-tap` repo on GitHub
- [ ] Add `Casks/verbatim.rb` and `Formula/verbatim.rb` to the tap repo
- [ ] Add `TAP_GITHUB_TOKEN` secret to `MK-Devices/verbatim-linux` repo settings
- [ ] Bump version to `0.1.0` in `src-tauri/Cargo.toml` and `tauri.conf.json`
- [ ] Tag `v0.1.0` and push — watch the CI run
- [ ] After release is created, compute SHA256 hashes and update the tap (or set up the auto-bump workflow)
- [ ] Test: `brew tap MK-Devices/tap && brew install --cask verbatim` (macOS)
- [ ] Test: `brew tap MK-Devices/tap && brew install verbatim` (Linux)
- [ ] Optionally create `.github/workflows/bump-homebrew.yml` for full automation
