#!/usr/bin/env python3
"""
Build Verbatim release bundles.

The script picks its own behavior based on the host OS:
  - Darwin (macOS) → one build (Metal auto), verbatim-<ver>-macos-<arch>.tar.gz
  - Linux          → cpu, vulkan, cuda in sequence; aborts on first failure.

Outputs to dist/ + dist/SHA256SUMS.txt.

Usage:  python3 scripts/package-release.py
        python3 scripts/package-release.py --only vulkan,cpu   # Linux only
"""

import argparse
import hashlib
import os
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DIST = ROOT / "dist"
CARGO_TOML = ROOT / "src-tauri" / "Cargo.toml"
# Cargo workspaces emit to <workspace-root>/target, not src-tauri/target.
# Check both so the script works whether or not the layout changes later.
BIN_CANDIDATES = [
    ROOT / "target" / "release" / "verbatim",
    ROOT / "src-tauri" / "target" / "release" / "verbatim",
]

LINUX_BACKENDS = ["cpu", "vulkan", "cuda"]


def log(msg: str) -> None:
    print(f"\033[1;34m[package]\033[0m {msg}", flush=True)


def find_binary() -> Path:
    for p in BIN_CANDIDATES:
        if p.exists():
            return p
    tried = "\n  ".join(str(p) for p in BIN_CANDIDATES)
    sys.exit(f"Could not find built `verbatim` binary. Looked at:\n  {tried}")


def read_version() -> str:
    text = CARGO_TOML.read_text()
    m = re.search(r'^version\s*=\s*"([^"]+)"', text, re.MULTILINE)
    if not m:
        sys.exit(f"Could not parse version from {CARGO_TOML}")
    return m.group(1)


def detect_arch(os_name: str) -> str:
    """Per-OS arch naming: macOS uses Homebrew convention (x86_64/arm64),
    Linux uses Debian convention (amd64/arm64). Matches the Homebrew formula."""
    m = platform.machine().lower()
    is_x86 = m in ("x86_64", "amd64")
    is_arm = m in ("arm64", "aarch64")
    if os_name == "macos":
        if is_x86:
            return "x86_64"
        if is_arm:
            return "arm64"
    elif os_name == "linux":
        if is_x86:
            return "amd64"
        if is_arm:
            return "arm64"
    sys.exit(f"Unsupported arch '{m}' for OS '{os_name}'")


def fmt_duration(seconds: float) -> str:
    m, s = divmod(int(seconds), 60)
    return f"{m}m{s:02d}s"


def run_build(backend: str, index: int, total: int) -> None:
    """Run `cargo tauri build` with the given backend feature. Raises on failure."""
    cmd = ["cargo", "tauri", "build"]
    if backend in ("cuda", "vulkan"):
        cmd += ["--features", backend]

    # CUDA on Debian: static libs live in /usr/lib/x86_64-linux-gnu, not
    # /usr/local/cuda/lib64 where llama-cpp-sys-2 and whisper-rs-sys look.
    env = os.environ.copy()
    if backend == "cuda":
        existing = env.get("RUSTFLAGS", "").strip()
        flag = "-L /usr/lib/x86_64-linux-gnu"
        env["RUSTFLAGS"] = f"{existing} {flag}".strip() if existing else flag

    print()
    log(f"\033[1;36m▶ [{index}/{total}] build start — backend={backend}\033[0m")
    log(f"  cwd: {ROOT}")
    if backend == "cuda":
        log(f"  env: RUSTFLAGS={env['RUSTFLAGS']!r}")
    log(f"  cmd: {' '.join(cmd)}")
    log(f"  (streaming cargo output — this usually takes 2–5 minutes)")
    start = time.monotonic()
    result = subprocess.run(cmd, cwd=ROOT, env=env)
    elapsed = time.monotonic() - start

    if result.returncode != 0:
        raise RuntimeError(
            f"Build failed for backend '{backend}' after {fmt_duration(elapsed)} "
            f"(exit code {result.returncode})"
        )
    log(f"\033[1;32m✔ [{index}/{total}] built '{backend}' in {fmt_duration(elapsed)}\033[0m")


def package(version: str, os_name: str, arch: str, backend: str | None) -> Path:
    bin_path = find_binary()
    suffix = f"-{backend}" if backend else ""
    name = f"verbatim-{version}-{os_name}-{arch}{suffix}.tar.gz"
    out = DIST / name

    size_mb = bin_path.stat().st_size / (1024 * 1024)
    log(f"  packing → {out}")
    log(f"    source binary: {bin_path} ({size_mb:.1f} MB)")
    with tarfile.open(out, "w:gz") as tar:
        tar.add(bin_path, arcname="verbatim")
    out_mb = out.stat().st_size / (1024 * 1024)
    log(f"    tarball: {out_mb:.1f} MB")
    return out


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def write_sums(artifacts: list[Path]) -> None:
    sums_path = DIST / "SHA256SUMS.txt"
    lines = []
    print()
    log(f"\033[1mComputing SHA256 for {len(artifacts)} artifact(s)\033[0m")
    for a in artifacts:
        digest = sha256(a)
        lines.append(f"{digest}  {a.name}")
        log(f"  {digest}  {a.name}")
    sums_path.write_text("\n".join(lines) + "\n")
    log(f"  wrote {sums_path}")


def run_macos(version: str, arch: str) -> int:
    log(f"\033[1mmacOS build — v{version} ({arch})\033[0m")
    log(f"  plan: 1 build (default features, Metal auto-enabled)")
    try:
        run_build("default", 1, 1)
    except RuntimeError as e:
        print(f"\n\033[1;31m✘ {e}\033[0m", file=sys.stderr)
        return 1
    artifact = package(version, "macos", arch, None)
    write_sums([artifact])
    return 0


def run_linux(version: str, arch: str, only: str | None) -> int:
    backends = LINUX_BACKENDS[:]
    if only:
        requested = [b.strip() for b in only.split(",") if b.strip()]
        invalid = [b for b in requested if b not in LINUX_BACKENDS]
        if invalid:
            sys.exit(f"Unknown backend(s): {', '.join(invalid)}")
        backends = requested

    log(f"\033[1mLinux builds — v{version} ({arch})\033[0m")
    log(f"  plan: {len(backends)} build(s) in order: {', '.join(backends)}")
    log(f"  aborts on first failure; partial artifacts still get checksummed")

    artifacts: list[Path] = []
    total = len(backends)
    for i, backend in enumerate(backends, start=1):
        try:
            run_build(backend, i, total)
        except RuntimeError as e:
            print(f"\n\033[1;31m✘ {e}\033[0m", file=sys.stderr)
            log(f"\033[1;31mStopping — {total - i} backend(s) skipped.\033[0m")
            if artifacts:
                write_sums(artifacts)
            return 1
        artifacts.append(package(version, "linux", arch, backend))
    write_sums(artifacts)
    return 0


def main() -> int:
    cargo = shutil.which("cargo")
    if cargo is None:
        sys.exit("cargo not found in PATH")

    system = platform.system()
    version = read_version()
    os_name = {"Darwin": "macos", "Linux": "linux"}.get(system)
    if os_name is None:
        sys.exit(f"Unsupported OS: {system}")
    arch = detect_arch(os_name)
    DIST.mkdir(exist_ok=True)
    overall_start = time.monotonic()

    log("──────── verbatim release packager ────────")
    log(f"  host:    {system} / {platform.machine()} → {arch}")
    log(f"  version: {version}")
    log(f"  cargo:   {cargo}")
    log(f"  root:    {ROOT}")
    log(f"  dist:    {DIST}")

    # OS-specific dispatch: argparse and code paths depend on the host.
    if system == "Darwin":
        argparse.ArgumentParser(
            description="Build the macOS release bundle."
        ).parse_args()
        rc = run_macos(version, arch)

    elif system == "Linux":
        ap = argparse.ArgumentParser(description="Build the Linux release bundles.")
        ap.add_argument(
            "--only",
            help=f"Comma-separated subset of backends to build ({','.join(LINUX_BACKENDS)}).",
        )
        args = ap.parse_args()
        rc = run_linux(version, arch, args.only)

    else:
        sys.exit(f"Unsupported OS: {system}")

    total = fmt_duration(time.monotonic() - overall_start)
    print()
    if rc == 0:
        log(f"\033[1;32m✔ All done in {total}.\033[0m")
    else:
        log(f"\033[1;31m✘ Aborted after {total}.\033[0m")
    return rc


if __name__ == "__main__":
    sys.exit(main())
