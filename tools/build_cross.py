#!/usr/bin/env python3
"""
Build the three single-file downloads on a macOS host.

    macOS    universal (arm64 + x86_64) inside Rowline.app
    Windows  x86_64-pc-windows-gnu, linked with mingw-w64
    Linux    x86_64-unknown-linux-musl, statically linked with Rust's
             bundled LLVM linker (no Linux cross toolchain needed)

Each platform is packaged by tools/package.py into dist/:

    dist/rowline-darwin/Rowline.app
    dist/rowline-win32/rowline.exe
    dist/rowline-linux/rowline

Usage:
    python3 tools/build_cross.py
    python3 tools/build_cross.py --platforms macos,windows
    python3 tools/build_cross.py --list
"""
import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RUST = ROOT / "rust"
DIST = ROOT / "dist"
PACKAGE = ROOT / "tools" / "package.py"

PLATFORMS = ("macos", "windows", "linux")
TARGETS = {
    "macos": ["aarch64-apple-darwin", "x86_64-apple-darwin"],
    "windows": ["x86_64-pc-windows-gnu"],
    "linux": ["x86_64-unknown-linux-musl"],
}
MINGW_NAMES = ("x86_64-w64-mingw32-gcc",)


def say(msg):
    print(msg, flush=True)


def fatal(msg, code=1):
    print("error: %s" % msg, file=sys.stderr, flush=True)
    raise SystemExit(code)


def run(cmd, cwd=RUST, env=None, quiet=False):
    cmd = [str(c) for c in cmd]
    if not quiet:
        say("  $ " + " ".join(cmd))
    subprocess.check_call(cmd, cwd=str(cwd), env=env)


def parse_platforms(value):
    names = [p.strip().lower() for p in value.split(",") if p.strip()]
    if "all" in names:
        return list(PLATFORMS)
    unknown = [p for p in names if p not in PLATFORMS]
    if unknown:
        fatal("unknown platform(s): %s (choose from %s)"
              % (", ".join(unknown), ", ".join(PLATFORMS)))
    return [p for p in PLATFORMS if p in names]


def toolchain():
    """Locate cargo/rustc/rustup, reusing build_all's PATH handling."""
    sys.path.insert(0, str(ROOT / "tools"))
    import build_all  # noqa: E402  (same directory)

    cargo, env = build_all.find_cargo()
    if cargo is None:
        fatal("cargo not found. Install Rust from https://rustup.rs/")
    rustc = Path(cargo).parent / ("rustc.exe" if os.name == "nt" else "rustc")
    if not rustc.is_file():
        fatal("rustc not found next to cargo at %s" % rustc)
    rustup = Path(cargo).parent / ("rustup.exe" if os.name == "nt" else "rustup")
    if not rustup.is_file():
        found = shutil.which("rustup")
        rustup = Path(found) if found else None
    return Path(cargo), rustc, rustup, env


def ensure_targets(cargo, rustup, env, platforms):
    wanted = [t for p in platforms for t in TARGETS[p]]
    if rustup:
        installed = subprocess.check_output(
            [str(rustup), "target", "list", "--installed"],
            env=env, text=True,
        ).split()
        missing = [t for t in wanted if t not in installed]
        if missing:
            say("installing Rust targets: %s" % " ".join(missing))
            run([rustup, "target", "add", *missing], env=env)
    else:
        # Without rustup we cannot install targets; let cargo produce the
        # precise error if one is missing.
        say("rustup not found; assuming the required targets are installed")
    return wanted


def find_mingw():
    candidates = [shutil.which(n) for n in MINGW_NAMES]
    candidates += [
        str(Path(p) / MINGW_NAMES[0])
        for p in ("/opt/homebrew/bin", "/usr/local/bin", "/usr/bin")
    ]
    for c in candidates:
        if c and Path(c).is_file():
            return Path(c)
    fatal("mingw-w64 not found.  Install it with: brew install mingw-w64")


def linux_environment(cargo, rustc, env):
    """Prepare rust-lld and musl stub archives for a static Linux build."""
    host = ""
    for line in subprocess.check_output([str(rustc), "-vV"], env=env, text=True).splitlines():
        if line.startswith("host:"):
            host = line.split(":", 1)[1].strip()
            break
    if not host:
        fatal("could not determine the Rust host triple")

    sysroot = Path(subprocess.check_output(
        [str(rustc), "--print", "sysroot"], env=env, text=True).strip())
    lld = sysroot / "lib" / "rustlib" / host / "bin" / "rust-lld"
    if not lld.is_file():
        fatal("rust-lld not found at %s (reinstall the Rust toolchain)" % lld)

    support = RUST / "target" / "cross-support"
    support.mkdir(parents=True, exist_ok=True)
    wrapper = support / "rust-lld"
    wrapper.write_text('#!/bin/sh\nexec "%s" "$@"\n' % lld)
    wrapper.chmod(0o755)

    # musl keeps dlopen/pthread/rt in libc.  rustc still passes -ldl for the
    # GUI stack, so provide empty archives; the symbols resolve from libc.a.
    stub_src = support / "stub.rs"
    stub_src.write_text(
        '#![no_std]\n'
        '#[no_mangle]\n'
        'pub extern "C" fn __rowline_musl_stub() {}\n'
    )
    stub_obj = support / "stub.o"
    run([rustc, "--target", "x86_64-unknown-linux-musl",
         "--crate-type=lib", "--emit=obj", "-O", stub_src, "-o", stub_obj],
        env=env, quiet=True)

    ar = shutil.which("ar")
    if not ar:
        fatal("`ar` not found (install the Xcode command line tools)")
    stubs = support / "stubs"
    stubs.mkdir(exist_ok=True)
    for name in ("dl", "pthread", "rt", "m", "util"):
        archive = stubs / ("lib%s.a" % name)
        archive.unlink(missing_ok=True)
        subprocess.run([ar, "crs", str(archive), "stub.o"],
                       cwd=str(support), check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    env = env.copy()
    linker_var = "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER"
    flags_var = "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS"
    env[linker_var] = str(wrapper)
    flags = env.get(flags_var, "").strip()
    extra = "-C linker-flavor=ld.lld -C link-arg=-L%s" % stubs
    env[flags_var] = (flags + " " + extra).strip()
    return env


def build_macos(cargo, env):
    for target in TARGETS["macos"]:
        say("building macOS %s" % target)
        run([cargo, "build", "--release", "-p", "rowline-app",
             "--target", target], env=env)
    universal = RUST / "target" / "universal"
    universal.mkdir(parents=True, exist_ok=True)
    out = universal / "rowline"
    lipo = shutil.which("lipo") or "/usr/bin/lipo"
    run([lipo, "-create",
         RUST / "target" / "aarch64-apple-darwin" / "release" / "rowline",
         RUST / "target" / "x86_64-apple-darwin" / "release" / "rowline",
         "-output", out], cwd=ROOT)
    return out


def build_windows(cargo, env):
    mingw = find_mingw()
    say("building Windows with %s" % mingw)
    env = env.copy()
    env["CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER"] = str(mingw)
    run([cargo, "build", "--release", "-p", "rowline-app",
         "--target", "x86_64-pc-windows-gnu"], env=env)
    out = RUST / "target" / "x86_64-pc-windows-gnu" / "release" / "rowline.exe"
    return out


def build_linux(cargo, rustc, env):
    say("building static Linux (musl + rust-lld)")
    env = linux_environment(cargo, rustc, env)
    run([cargo, "build", "--release", "-p", "rowline-app",
         "--target", "x86_64-unknown-linux-musl"], env=env)
    out = RUST / "target" / "x86_64-unknown-linux-musl" / "release" / "rowline"
    return out


def check_binary(platform, path):
    if not path.is_file() or path.stat().st_size == 0:
        fatal("build produced no binary at %s" % path)
    head = path.read_bytes()[:4]
    if platform == "windows" and head[:2] != b"MZ":
        fatal("%s is not a Windows executable" % path)
    if platform == "linux" and head != b"\x7fELF":
        fatal("%s is not an ELF executable" % path)
    if platform == "macos":
        lipo = shutil.which("lipo") or "/usr/bin/lipo"
        info = subprocess.check_output([lipo, "-info", str(path)], text=True)
        if "arm64" not in info or "x86_64" not in info:
            fatal("%s is not a universal binary:\n%s" % (path, info))


def package(platform, binary):
    run([sys.executable, PACKAGE,
         "--out", DIST, "--binary", binary,
         "--platform", platform, "--zip"], cwd=ROOT)


def main():
    ap = argparse.ArgumentParser(
        description="Build macOS, Windows and Linux binaries on macOS.")
    ap.add_argument("--platforms", default="all",
                    help="comma-separated: macos,windows,linux (default: all)")
    ap.add_argument("--list", action="store_true",
                    help="show the plan and exit")
    args = ap.parse_args()

    if sys.platform != "darwin":
        fatal("this script cross-builds from macOS only; on other hosts run "
              "tools/package.py for the current platform")

    platforms = parse_platforms(args.platforms)
    if args.list:
        say("host          : macOS")
        say("platforms     : %s" % ", ".join(platforms))
        say("output        : %s" % DIST)
        say("")
        for p in platforms:
            say("  %-8s %s" % (p, ", ".join(TARGETS[p])))
        return 0

    cargo, rustc, rustup, env = toolchain()
    say("using cargo   : %s" % cargo)
    ensure_targets(cargo, rustup, env, platforms)

    artifacts = {}
    for platform in platforms:
        if platform == "macos":
            binary = build_macos(cargo, env)
        elif platform == "windows":
            binary = build_windows(cargo, env)
        else:
            binary = build_linux(cargo, rustc, env)
        check_binary(platform, binary)
        package(platform, binary)
        artifacts[platform] = binary

    say("")
    say("done:")
    for platform, binary in artifacts.items():
        say("  %-8s %s" % (platform, binary))
    return 0


if __name__ == "__main__":
    sys.exit(main())
