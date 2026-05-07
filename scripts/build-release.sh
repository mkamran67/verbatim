#!/usr/bin/env bash
# Build Verbatim release artifacts for the current OS.
#
# Usage:
#   scripts/build-release.sh                # bump minor version + build all variants
#   scripts/build-release.sh --major        # bump major version instead of minor
#   scripts/build-release.sh --patch        # bump patch version instead of minor
#   scripts/build-release.sh --no-bump      # rebuild current version (no bump)
#   scripts/build-release.sh --type cuda    # build a single variant
#   scripts/build-release.sh --skip-notarize  # macOS: skip notarize+staple
#   scripts/build-release.sh --list         # list available variants for this OS
#   scripts/build-release.sh --help
#
# Output: ./releases/verbatim-<version>-<os>-<arch>-<variant>.<ext>
#
# On macOS, after a successful build the .app is zipped, submitted to Apple for
# notarization (via `xcrun notarytool submit --wait`), and the staple ticket is
# attached so Gatekeeper accepts the artifact offline.
#
# Each variant's full cargo log is written to releases/.logs/<variant>.log and
# is only echoed to the terminal when the build fails.

set -uo pipefail

# ── Paths & metadata ──────────────────────────────────────────────────────────
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." &>/dev/null && pwd)"
RELEASES="$ROOT/releases"
LOGDIR="$RELEASES/.logs"
TAURI_CONF="$ROOT/src-tauri/tauri.conf.json"

VERSION="$(grep -m1 '"version"' "$TAURI_CONF" | sed -E 's/.*"version": *"([^"]+)".*/\1/')"
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
RUN_START=$SECONDS

# ── Colors / animation ────────────────────────────────────────────────────────
if [[ -t 1 ]] && [[ "${NO_COLOR:-}" == "" ]]; then
  C_RESET=$'\033[0m'; C_DIM=$'\033[2m'; C_BOLD=$'\033[1m'
  C_RED=$'\033[31m'; C_GREEN=$'\033[32m'; C_YELLOW=$'\033[33m'
  C_BLUE=$'\033[34m'; C_CYAN=$'\033[36m'; C_MAGENTA=$'\033[35m'
  CLR_LINE=$'\r\033[2K'; HIDE_CUR=$'\033[?25l'; SHOW_CUR=$'\033[?25h'
else
  C_RESET=""; C_DIM=""; C_BOLD=""; C_RED=""; C_GREEN=""; C_YELLOW=""
  C_BLUE=""; C_CYAN=""; C_MAGENTA=""; CLR_LINE=$'\r'; HIDE_CUR=""; SHOW_CUR=""
fi
SPINNER=(⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏)

cleanup() { printf "%s" "$SHOW_CUR"; }
trap cleanup EXIT INT TERM

# ── UI helpers ────────────────────────────────────────────────────────────────
PHASE_NUM=0
TOTAL_PHASES=3   # adjusted at runtime once we know the platform/flags

# Print a phase header. Use blank-line spacing so phases are visually distinct.
phase() {
  PHASE_NUM=$((PHASE_NUM + 1))
  local name="$1" detail="${2:-}"
  printf "\n%s┏━━ %s[%d/%d] %s%s" \
    "$C_BOLD$C_BLUE" "$C_RESET$C_BOLD" "$PHASE_NUM" "$TOTAL_PHASES" "$name" "$C_RESET"
  if [[ -n "$detail" ]]; then
    printf "  %s%s%s" "$C_DIM" "$detail" "$C_RESET"
  fi
  printf "\n"
}

ok()    { printf "  %s✓%s %s\n"  "$C_GREEN"   "$C_RESET" "$*"; }
fail()  { printf "  %s✗%s %s\n"  "$C_RED"     "$C_RESET" "$*"; }
step()  { printf "  %s→%s %s\n"  "$C_BLUE"    "$C_RESET" "$*"; }
info()  { printf "  %s·%s %s\n"  "$C_DIM"     "$C_RESET" "$*"; }
warn()  { printf "  %s!%s %s\n"  "$C_YELLOW"  "$C_RESET" "$*"; }
note()  { printf "    %s%s%s\n"  "$C_DIM"     "$*"       "$C_RESET"; }

# Format seconds as MM:SS.
fmt_duration() {
  local s=$1
  printf "%02d:%02d" $((s / 60)) $((s % 60))
}

# Format a byte count as a human-readable size.
fmt_size() {
  local bytes=$1
  if   (( bytes >= 1073741824 )); then printf "%.1f GB" "$(bc -l <<<"$bytes/1073741824")"
  elif (( bytes >= 1048576    )); then printf "%.1f MB" "$(bc -l <<<"$bytes/1048576")"
  elif (( bytes >= 1024       )); then printf "%.1f KB" "$(bc -l <<<"$bytes/1024")"
  else                                 printf "%d B"     "$bytes"
  fi
}

# ── Version bump ──────────────────────────────────────────────────────────────
sed_i() {
  if [[ "$(uname)" == "Darwin" ]]; then sed -i '' "$@"; else sed -i "$@"; fi
}

bump_version() {
  local mode="$1"
  local old="$VERSION"
  local major minor patch
  IFS='.' read -r major minor patch <<<"$old"
  if ! [[ "$major" =~ ^[0-9]+$ && "$minor" =~ ^[0-9]+$ && "$patch" =~ ^[0-9]+$ ]]; then
    fail "cannot parse current version: $old"; exit 1
  fi
  case "$mode" in
    major) major=$((major + 1)); minor=0; patch=0 ;;
    minor) minor=$((minor + 1)); patch=0 ;;
    patch) patch=$((patch + 1)) ;;
    *) fail "unknown bump mode: $mode"; exit 2 ;;
  esac
  local new="${major}.${minor}.${patch}"

  sed_i -E "s/(\"version\"[[:space:]]*:[[:space:]]*\")[0-9]+\.[0-9]+\.[0-9]+(\")/\1${new}\2/" \
    "$TAURI_CONF" "$ROOT/ui/package.json"
  sed_i -E "s/^(version[[:space:]]*=[[:space:]]*\")[0-9]+\.[0-9]+\.[0-9]+(\")/\1${new}\2/" \
    "$ROOT/src-tauri/Cargo.toml" "$ROOT/verbatim-core/Cargo.toml"

  VERSION="$new"
  ok "version bumped: ${C_DIM}${old}${C_RESET} → ${C_BOLD}${C_CYAN}${new}${C_RESET} ${C_DIM}(${mode})${C_RESET}"
}

# ── Variant catalog ───────────────────────────────────────────────────────────
declare -a VARIANTS_LINUX=(cpu cuda vulkan rocm)
declare -a VARIANTS_MACOS=(metal)

variant_features() {
  case "$1" in
    cpu|metal) echo "" ;;
    cuda)      echo "cuda" ;;
    vulkan)    echo "vulkan" ;;
    rocm)      echo "rocm" ;;
    *) return 1 ;;
  esac
}

variant_label() {
  case "$1" in
    cpu)    echo "CPU (no GPU acceleration)" ;;
    cuda)   echo "CUDA — NVIDIA" ;;
    vulkan) echo "Vulkan — NVIDIA + AMD (untested)" ;;
    rocm)   echo "ROCm — AMD (untested)" ;;
    metal)  echo "Metal — Apple Silicon / Intel" ;;
    *) echo "$1" ;;
  esac
}

applicable_variants() {
  case "$OS" in
    linux)  printf '%s\n' "${VARIANTS_LINUX[@]}" ;;
    darwin) printf '%s\n' "${VARIANTS_MACOS[@]}" ;;
    *) fail "unsupported OS: $OS" >&2; exit 1 ;;
  esac
}

# ── Help / list ───────────────────────────────────────────────────────────────
usage() {
  cat <<EOF
${C_BOLD}Verbatim release builder${C_RESET}

Usage: $(basename "$0") [bump] [--type <variant>] [--skip-notarize] [--list] [--help]

Version bump (default: minor):
  (none)             Bump minor version (0.1.0 → 0.2.0)
  --major            Bump major version (0.1.0 → 1.0.0)
  --patch            Bump patch version (0.1.0 → 0.1.1)
  --no-bump          Rebuild current version without bumping

Build options:
  --type <variant>   Build a single variant (see --list)
  --skip-notarize    macOS only: skip the notarize + staple step
  --list             List variants applicable to this OS
  --help             Show this help

Detected: ${C_CYAN}${OS}/${ARCH}${C_RESET}, version ${C_CYAN}${VERSION}${C_RESET}
EOF
}

list_variants() {
  echo "${C_BOLD}Variants for ${OS}:${C_RESET}"
  while read -r v; do
    printf "  ${C_CYAN}%-7s${C_RESET}  %s\n" "$v" "$(variant_label "$v")"
  done < <(applicable_variants)
}

# ── Argument parsing ──────────────────────────────────────────────────────────
ONLY=""
BUMP_MODE="minor"
SKIP_NOTARIZE=0
RESUME_MODE=0   # set during Setup if Apple is still processing a prior submission
while [[ $# -gt 0 ]]; do
  case "$1" in
    --type) ONLY="${2:-}"; shift 2 ;;
    --major) BUMP_MODE="major"; shift ;;
    --patch) BUMP_MODE="patch"; shift ;;
    --no-bump) BUMP_MODE="none"; shift ;;
    --skip-notarize) SKIP_NOTARIZE=1; shift ;;
    --list) list_variants; exit 0 ;;
    --help|-h) usage; exit 0 ;;
    *) fail "unknown arg: $1"; usage; exit 2 ;;
  esac
done

# ── Build runner ──────────────────────────────────────────────────────────────
# Spawns `cargo tauri build` for one variant, streams a single-line status
# updated from the latest log line, and only prints the full log on failure.
build_variant() {
  local variant="$1"
  local features; features="$(variant_features "$variant")" || {
    fail "unknown variant: $variant"; return 2; }
  local label; label="$(variant_label "$variant")"
  local log="$LOGDIR/$variant.log"
  : >"$log"

  local cmd=(cargo tauri build --verbose)
  [[ -n "$features" ]] && cmd+=(--features "$features")

  step "${C_BOLD}${C_CYAN}${variant}${C_RESET} ${C_DIM}— ${label}${C_RESET}"
  note "$ ${cmd[*]}"

  printf "%s" "$HIDE_CUR"
  local start=$SECONDS
  ( cd "$ROOT" && "${cmd[@]}" ) >>"$log" 2>&1 &
  local pid=$!

  local i=0
  while kill -0 "$pid" 2>/dev/null; do
    local frame="${SPINNER[$((i % ${#SPINNER[@]}))]}"
    local elapsed=$((SECONDS - start))
    local last; last="$(tail -n 1 "$log" 2>/dev/null | tr -d '\r')"
    local cols; cols="$(tput cols 2>/dev/null || echo 100)"
    local prefix; prefix="$(printf "    %s%s%s %s[%s]%s " "$C_MAGENTA" "$frame" "$C_RESET" "$C_DIM" "$(fmt_duration "$elapsed")" "$C_RESET")"
    local plain_prefix; plain_prefix="$(printf "    %s [%s] " "$frame" "$(fmt_duration "$elapsed")")"
    local max=$((cols - ${#plain_prefix} - 1))
    (( max < 10 )) && max=10
    if (( ${#last} > max )); then last="…${last: -$((max - 1))}"; fi
    printf "%s%s%s%s%s" "$CLR_LINE" "$prefix" "$C_DIM" "$last" "$C_RESET"
    i=$((i + 1))
    sleep 0.1
  done
  wait "$pid"; local rc=$?
  local elapsed=$((SECONDS - start))
  printf "%s" "$CLR_LINE"

  if (( rc != 0 )); then
    fail "build failed for ${C_BOLD}${variant}${C_RESET} after $(fmt_duration "$elapsed")"
    printf "    %s── full log (%s) ──%s\n" "$C_DIM" "$log" "$C_RESET"
    cat "$log" | sed 's/^/    /'
    printf "    %s── end log ──%s\n" "$C_DIM" "$C_RESET"
    return "$rc"
  fi

  collect_artifacts "$variant"
  local crc=$?
  if (( crc != 0 )); then
    fail "no artifacts found for ${C_BOLD}${variant}${C_RESET} after build (see $log)"
    return "$crc"
  fi

  ok "built ${C_BOLD}${variant}${C_RESET} in $(fmt_duration "$elapsed")"
  rm -f "$log"
  return 0
}

# ── Artifact collection ───────────────────────────────────────────────────────
collect_artifacts() {
  local variant="$1"
  local bundle_roots=(
    "$ROOT/target/release/bundle"
    "$ROOT/src-tauri/target/release/bundle"
  )
  local stem="verbatim-${VERSION}-${OS}-${ARCH}-${variant}"
  local found=0

  shopt -s nullglob
  for bundle in "${bundle_roots[@]}"; do
    [[ -d "$bundle" ]] || continue
    if [[ "$OS" == "linux" ]]; then
      for f in "$bundle/deb/"*.deb;            do cp -f "$f" "$RELEASES/${stem}.deb";        found=1; note "→ ${stem}.deb"; done
      for f in "$bundle/appimage/"*.AppImage;  do cp -f "$f" "$RELEASES/${stem}.AppImage"; chmod +x "$RELEASES/${stem}.AppImage"; found=1; note "→ ${stem}.AppImage"; done
      for f in "$bundle/rpm/"*.rpm;            do cp -f "$f" "$RELEASES/${stem}.rpm";        found=1; note "→ ${stem}.rpm"; done
    elif [[ "$OS" == "darwin" ]]; then
      # Tar the .app ourselves; the notarize phase repacks it once stapled.
      for app in "$bundle/macos/"*.app; do
        tar -czf "$RELEASES/${stem}.app.tar.gz" -C "$(dirname "$app")" "$(basename "$app")"
        found=1
        note "→ ${stem}.app.tar.gz"
      done
    fi
  done
  shopt -u nullglob

  (( found == 1 )) || return 1
  return 0
}

# ── macOS signing setup ───────────────────────────────────────────────────────
# TCC keys permissions by binary's Designated Requirement; ad-hoc signatures
# rotate cdhash on every build and silently invalidate user grants. Sign with a
# stable Developer ID identity so permissions persist across updates.
if [[ "$OS" == "darwin" ]]; then
  # shellcheck disable=SC1091
  [[ -f "$ROOT/scripts/.macos-signing.env" ]] && source "$ROOT/scripts/.macos-signing.env"
  : "${APPLE_SIGNING_IDENTITY:?Set APPLE_SIGNING_IDENTITY in scripts/.macos-signing.env (e.g. 'Developer ID Application: Your Name (TEAMID)')}"
  : "${APPLE_ID:?Set APPLE_ID for notarization}"
  : "${APPLE_PASSWORD:?Set APPLE_PASSWORD (app-specific password) for notarization}"
  : "${APPLE_TEAM_ID:?Set APPLE_TEAM_ID for notarization}"
  export APPLE_SIGNING_IDENTITY APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID
fi

# ── macOS notarize + staple ───────────────────────────────────────────────────
# Quick yes/no check used during Setup to decide whether to suppress the
# version bump (so a resumed run targets the same version Apple is processing).
# Returns 0 if any submission is "In Progress", 1 otherwise.
has_in_progress_notarization() {
  local history
  if ! history="$(xcrun notarytool history \
      --apple-id "$APPLE_ID" \
      --password "$APPLE_PASSWORD" \
      --team-id "$APPLE_TEAM_ID" 2>/dev/null)"; then
    return 1
  fi
  printf '%s\n' "$history" | awk '
    BEGIN { rc = 1 }
    /^[[:space:]]*status:[[:space:]]/ {
      sub(/^[[:space:]]*status:[[:space:]]*/, "")
      if ($0 == "In Progress") { rc = 0; exit }
    }
    END { exit rc }
  '
}

wait_for_in_progress_notarizations() {
  step "checking notary history"
  local history
  if ! history="$(xcrun notarytool history \
      --apple-id "$APPLE_ID" \
      --password "$APPLE_PASSWORD" \
      --team-id "$APPLE_TEAM_ID" 2>/dev/null)"; then
    warn "could not fetch notary history; proceeding"
    return 0
  fi

  local ids
  ids="$(printf '%s\n' "$history" | awk '
    /^[[:space:]]*id:[[:space:]]/    { id=$2 }
    /^[[:space:]]*status:[[:space:]]/ {
      sub(/^[[:space:]]*status:[[:space:]]*/, "")
      if ($0 == "In Progress" && id != "") print id
      id=""
    }
  ')"

  if [[ -z "$ids" ]]; then
    ok "no in-progress submissions"
    return 0
  fi

  local count; count="$(printf '%s\n' "$ids" | grep -c .)"
  warn "${count} in-progress submission(s) found — waiting"
  note "Ctrl-C is safe; Apple keeps processing and a rerun will resume here"
  while IFS= read -r id; do
    [[ -z "$id" ]] && continue
    step "waiting on ${C_DIM}${id}${C_RESET}"
    if ! xcrun notarytool wait "$id" \
        --apple-id "$APPLE_ID" \
        --password "$APPLE_PASSWORD" \
        --team-id "$APPLE_TEAM_ID" >/dev/null 2>&1; then
      warn "wait on ${id} returned non-zero; continuing"
    else
      ok "${C_DIM}${id}${C_RESET} resolved"
    fi
  done <<<"$ids"
}

# Notarize the .app (not the DMG): Tauri signs the .app, Apple validates the
# signature, we staple the ticket. Gatekeeper validates the .app on first
# launch whether it was extracted from a DMG, zip, or tarball.
notarize_and_staple_macos() {
  wait_for_in_progress_notarizations || return 1

  local apps=()
  shopt -s nullglob
  for app in "$ROOT"/target/release/bundle/macos/*.app \
             "$ROOT"/src-tauri/target/release/bundle/macos/*.app; do
    apps+=("$app")
  done
  shopt -u nullglob

  if (( ${#apps[@]} == 0 )); then
    warn "no .app bundles found to notarize"
    return 0
  fi

  for app in "${apps[@]}"; do
    local zip="${app%.app}.notarize.zip"
    rm -f "$zip"

    step "zipping $(basename "$app") for submission"
    if ! ditto -c -k --keepParent "$app" "$zip" 2>&1 | sed 's/^/    /'; then
      fail "ditto failed for $app"; return 1
    fi

    step "submitting to Apple notary ${C_DIM}(typically 1–5 min)${C_RESET}"
    local nstart=$SECONDS
    if ! xcrun notarytool submit "$zip" \
        --apple-id "$APPLE_ID" \
        --password "$APPLE_PASSWORD" \
        --team-id "$APPLE_TEAM_ID" \
        --wait 2>&1 | sed 's/^/      /'; then
      fail "notarytool submission failed"
      rm -f "$zip"; return 1
    fi
    rm -f "$zip"
    ok "Apple accepted submission in $(fmt_duration $((SECONDS - nstart)))"

    step "stapling $(basename "$app")"
    if ! xcrun stapler staple "$app" >/dev/null 2>&1; then
      fail "stapling failed for $app"; return 1
    fi
    ok "stapled $(basename "$app")"

    # Repack the distributable .app.tar.gz with the stapled .app.
    local stem="verbatim-${VERSION}-${OS}-${ARCH}"
    shopt -s nullglob
    for tgz in "$RELEASES/${stem}"-*.app.tar.gz; do
      tar -czf "$tgz" -C "$(dirname "$app")" "$(basename "$app")" \
        && ok "repackaged $(basename "$tgz") with stapled .app"
    done
    shopt -u nullglob

    # Build a DMG from the stapled .app for users who prefer that format.
    create_dmg_from_app "$app" || return 1
  done
}

# Resume path: a prior run already submitted to Apple. Wait for that submission
# to land, then staple the existing on-disk .app (which has the same cdhash
# Apple is processing) and repack the distributable tarball. No rebuild and no
# new submission.
staple_only_macos() {
  wait_for_in_progress_notarizations || return 1

  local apps=()
  shopt -s nullglob
  for app in "$ROOT"/target/release/bundle/macos/*.app \
             "$ROOT"/src-tauri/target/release/bundle/macos/*.app; do
    apps+=("$app")
  done
  shopt -u nullglob

  if (( ${#apps[@]} == 0 )); then
    fail "resume requested but no .app bundle on disk to staple"
    note "the prior build's .app must be present in target/release/bundle/macos/"
    return 1
  fi

  for app in "${apps[@]}"; do
    step "stapling $(basename "$app") with the resolved ticket"
    if ! xcrun stapler staple "$app" >/dev/null 2>&1; then
      fail "stapling failed for $app"
      note "the in-progress submission may have been Rejected/Invalid — check 'notarytool history'"
      return 1
    fi
    ok "stapled $(basename "$app")"

    local stem="verbatim-${VERSION}-${OS}-${ARCH}"
    shopt -s nullglob
    local tgz_count=0
    for tgz in "$RELEASES/${stem}"-*.app.tar.gz; do
      tar -czf "$tgz" -C "$(dirname "$app")" "$(basename "$app")" \
        && ok "repackaged $(basename "$tgz") with stapled .app"
      tgz_count=$((tgz_count + 1))
    done
    shopt -u nullglob
    if (( tgz_count == 0 )); then
      # Prior run was interrupted before collect_artifacts; create a fresh tarball.
      local tgz="$RELEASES/${stem}-metal.app.tar.gz"
      tar -czf "$tgz" -C "$(dirname "$app")" "$(basename "$app")"
      ok "packaged $(basename "$tgz") from stapled .app"
    fi

    create_dmg_from_app "$app" || return 1
  done
}

# Build a DMG from the stapled .app using hdiutil. The DMG is intentionally
# unsigned/unnotarized — Gatekeeper accepts the .app inside (which carries its
# own staple ticket) when the user drags it to /Applications.
create_dmg_from_app() {
  local app="$1"
  local stem="verbatim-${VERSION}-${OS}-${ARCH}-metal"
  local dmg="$RELEASES/${stem}.dmg"
  local staging
  staging="$(mktemp -d -t verbatim-dmg)" || { fail "could not create temp dir"; return 1; }

  step "assembling DMG layout"
  cp -R "$app" "$staging/"
  ln -s /Applications "$staging/Applications"

  rm -f "$dmg"
  step "creating DMG ${C_DIM}(hdiutil, compressed)${C_RESET}"
  if ! hdiutil create -volname "Verbatim" -srcfolder "$staging" \
        -ov -format UDZO -fs HFS+ "$dmg" >/dev/null 2>&1; then
    fail "hdiutil failed to create $dmg"
    rm -rf "$staging"
    return 1
  fi
  rm -rf "$staging"
  ok "created $(basename "$dmg")"
}

verify_macos_artifacts() {
  shopt -s nullglob
  local any=0
  for app in "$ROOT"/target/release/bundle/macos/*.app \
             "$ROOT"/src-tauri/target/release/bundle/macos/*.app; do
    any=1
    if ! xcrun stapler validate "$app" >/dev/null 2>&1; then
      fail "stapler validate failed for $app"; return 1
    fi
    ok "stapler validate: $(basename "$app")"

    local spctl_out
    if ! spctl_out="$(spctl -a -vv -t install "$app" 2>&1)"; then
      fail "Gatekeeper rejected $app"
      printf '%s\n' "$spctl_out" | sed 's/^/      /' >&2
      return 1
    fi
    local source_line; source_line="$(printf '%s\n' "$spctl_out" | grep -E '^source=' | head -1)"
    ok "Gatekeeper accepted ${C_DIM}(${source_line:-source=unknown})${C_RESET}"
  done
  shopt -u nullglob
  (( any == 1 )) || warn "no .app bundles found to verify"
}

# ── Main ──────────────────────────────────────────────────────────────────────
mkdir -p "$RELEASES" "$LOGDIR"

# Determine target list early so we can show it in the banner.
if [[ -n "$ONLY" ]]; then
  if ! applicable_variants | grep -qx "$ONLY"; then
    fail "variant '$ONLY' is not applicable on $OS"
    list_variants; exit 2
  fi
  TARGETS=("$ONLY")
else
  TARGETS=()
  while IFS= read -r line; do TARGETS+=("$line"); done < <(applicable_variants)
fi

# Detect resume mode early so the banner reflects it. has_in_progress_notarization
# does a quick `notarytool history` call and grep — usually under a second.
if [[ "$OS" == "darwin" && $SKIP_NOTARIZE -eq 0 ]] && has_in_progress_notarization; then
  RESUME_MODE=1
  if [[ "$BUMP_MODE" != "none" ]]; then
    BUMP_MODE_OVERRIDDEN="$BUMP_MODE"
    BUMP_MODE="none"
  fi
fi

# Phase count: Setup + Build + (Notarize + Verify on macOS) + Done
if [[ "$OS" == "darwin" && $SKIP_NOTARIZE -eq 0 ]]; then
  TOTAL_PHASES=5
else
  TOTAL_PHASES=3
fi

# ── Banner ────────────────────────────────────────────────────────────────────
printf "%s╭─ Verbatim release build ─────────────────────────────╮%s\n" "$C_BOLD" "$C_RESET"
printf "%s│%s  version : %s%s%s\n" "$C_BOLD" "$C_RESET" "$C_CYAN" "$VERSION" "$C_RESET"
printf "%s│%s  os/arch : %s%s/%s%s\n" "$C_BOLD" "$C_RESET" "$C_CYAN" "$OS" "$ARCH" "$C_RESET"
printf "%s│%s  variants: %s%s%s\n" "$C_BOLD" "$C_RESET" "$C_CYAN" "${TARGETS[*]}" "$C_RESET"
if [[ "$OS" == "darwin" ]]; then
  if (( SKIP_NOTARIZE == 1 )); then
    printf "%s│%s  notarize: %sskipped (--skip-notarize)%s\n" "$C_BOLD" "$C_RESET" "$C_YELLOW" "$C_RESET"
  elif (( RESUME_MODE == 1 )); then
    printf "%s│%s  notarize: %sresuming prior submission%s\n" "$C_BOLD" "$C_RESET" "$C_MAGENTA" "$C_RESET"
  else
    printf "%s│%s  notarize: %senabled%s\n" "$C_BOLD" "$C_RESET" "$C_GREEN" "$C_RESET"
  fi
fi
printf "%s│%s  output  : %s%s%s\n" "$C_BOLD" "$C_RESET" "$C_CYAN" "$RELEASES" "$C_RESET"
printf "%s╰──────────────────────────────────────────────────────╯%s\n" "$C_BOLD" "$C_RESET"

# ── Phase 1: Setup ────────────────────────────────────────────────────────────
phase "Setup" "preparing release directory and version"

# Sweep stale macOS DMGs from previous runs (we no longer ship a DMG).
if [[ "$OS" == "darwin" ]]; then
  shopt -s nullglob
  stale_dmgs=("$RELEASES"/*.dmg)
  if (( ${#stale_dmgs[@]} > 0 )); then
    for f in "${stale_dmgs[@]}"; do
      rm -f "$f"
      info "removed stale ${C_DIM}$(basename "$f")${C_RESET}"
    done
  else
    info "no stale DMGs to clean"
  fi
  shopt -u nullglob
fi

# Surface resume-mode decision (already made above the banner so it could appear
# in the header). In resume mode we skip the bump and rebuild and just staple
# the existing on-disk .app once Apple finishes its prior submission.
if (( RESUME_MODE == 1 )); then
  warn "resume mode: in-progress notarization detected at Apple"
  note "skipping version bump and rebuild — will wait for it, then staple existing .app"
  if [[ -n "${BUMP_MODE_OVERRIDDEN:-}" ]]; then
    note "(overrode --${BUMP_MODE_OVERRIDDEN})"
  fi
fi

if [[ "$BUMP_MODE" != "none" ]]; then
  bump_version "$BUMP_MODE"
else
  info "version unchanged: ${C_CYAN}${VERSION}${C_RESET} ${C_DIM}(--no-bump)${C_RESET}"
fi

# ── Phase 2: Build ────────────────────────────────────────────────────────────
if (( RESUME_MODE == 1 )); then
  phase "Build" "skipped — using existing .app from prior run"
  shopt -s nullglob
  found_app=0
  for app in "$ROOT"/target/release/bundle/macos/*.app \
             "$ROOT"/src-tauri/target/release/bundle/macos/*.app; do
    info "reusing $(basename "$app")"
    found_app=1
  done
  shopt -u nullglob
  if (( found_app == 0 )); then
    fail "resume mode but no .app bundle on disk"
    note "expected at target/release/bundle/macos/*.app"
    exit 1
  fi
else
  phase "Build" "${#TARGETS[@]} variant(s): ${TARGETS[*]}"

  OK=(); FAIL=()
  for v in "${TARGETS[@]}"; do
    if build_variant "$v"; then OK+=("$v"); else FAIL+=("$v"); fi
  done

  if (( ${#FAIL[@]} > 0 )); then
    printf "\n%s%d build(s) failed:%s\n" "$C_RED$C_BOLD" "${#FAIL[@]}" "$C_RESET"
    for v in "${FAIL[@]}"; do
      fail "${v}   ${C_DIM}(log: $LOGDIR/$v.log)${C_RESET}"
    done
    exit 1
  fi
fi

# ── Phase 3: Notarize (macOS) ─────────────────────────────────────────────────
if [[ "$OS" == "darwin" && $SKIP_NOTARIZE -eq 0 ]]; then
  if (( RESUME_MODE == 1 )); then
    phase "Notarize" "resuming prior submission — wait + staple only"
    if ! staple_only_macos; then
      printf "\n%sResume failed.%s See messages above.\n" \
        "$C_RED$C_BOLD" "$C_RESET" >&2
      exit 1
    fi
  else
    phase "Notarize" "Apple Developer ID submission"
    if ! notarize_and_staple_macos; then
      printf "\n%sNotarization failed.%s .app is built but not notarized.\n" \
        "$C_RED$C_BOLD" "$C_RESET" >&2
      exit 1
    fi
  fi

  # ── Phase 4: Verify ─────────────────────────────────────────────────────────
  phase "Verify" "stapler + Gatekeeper checks"
  if ! verify_macos_artifacts; then
    exit 1
  fi
fi

# ── Final phase: Done ─────────────────────────────────────────────────────────
phase "Done" "$(fmt_duration $((SECONDS - RUN_START))) total"

shopt -s nullglob
artifacts=("$RELEASES"/verbatim-"${VERSION}"-"${OS}"-"${ARCH}"-*.{deb,AppImage,rpm,app.tar.gz,dmg})
shopt -u nullglob
if (( ${#artifacts[@]} > 0 )); then
  printf "  %sartifacts in %s%s%s%s:\n" "$C_BOLD" "$C_CYAN" "$RELEASES" "$C_RESET" "$C_RESET"
  for a in "${artifacts[@]}"; do
    local_size=""
    if [[ -f "$a" ]]; then
      bytes=$(stat -f%z "$a" 2>/dev/null || stat -c%s "$a" 2>/dev/null || echo 0)
      local_size="$(fmt_size "$bytes")"
    fi
    printf "    %s•%s %s  %s%s%s\n" "$C_GREEN" "$C_RESET" "$(basename "$a")" "$C_DIM" "$local_size" "$C_RESET"
  done
else
  warn "no artifacts found in $RELEASES (unexpected)"
fi

printf "\n%sAll builds succeeded.%s\n" "$C_GREEN$C_BOLD" "$C_RESET"

if [[ "$OS" == "darwin" ]]; then
  open "$RELEASES" >/dev/null 2>&1 || true
elif [[ "$OS" == "linux" ]] && command -v xdg-open >/dev/null 2>&1; then
  xdg-open "$RELEASES" >/dev/null 2>&1 || true
fi

exit 0
