#!/usr/bin/env python3
"""
Build Rowline for Linux, macOS and Windows.

A desktop host can only link binaries for its own operating system:

* macOS needs the Apple SDK and linker, which only exist on a Mac;
* Linux cross builds need a matching sysroot/toolchain (cross, zigbuild, ...).

The reliable way to produce all three from any one machine is therefore to let
GitHub Actions build each one on a native runner.  This script does that for
you: it builds the host platform locally and, when another platform is
requested, triggers `.github/workflows/release.yml`, waits for it and downloads
the bundles into `dist/`.

Usage:
    python tools/build_all.py --list        # show the plan, build nothing
    python tools/build_all.py               # auto: host locally + rest on CI
    python tools/build_all.py --mode local  # this machine only
    python tools/build_all.py --mode ci     # all requested platforms on CI
    python tools/build_all.py --platforms linux,macos --mode ci

CI mode requires the GitHub CLI (`gh`) and pushes nothing itself: commit and
push first, because Actions builds the *pushed* revision, not your working
tree.  Pass --allow-dirty only when you are sure the pushed revision already
contains what you want to build.

Prerequisites: Python 3.9+, Git, Rust (for a local build), GitHub CLI (CI).
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORKFLOW = "release.yml"
PLATFORMS = ("linux", "macos", "windows")

# The bundle directory names come from `sys.platform` in tools/package.py.
BUNDLES = {
    "linux": "rowline-linux",
    "macos": "rowline-darwin",
    "windows": "rowline-win32",
}


def say(msg):
    print(msg, flush=True)


def fatal(msg, code=1):
    print("error: %s" % msg, file=sys.stderr, flush=True)
    raise SystemExit(code)


def host_platform():
    if sys.platform.startswith("win"):
        return "windows"
    if sys.platform == "darwin":
        return "macos"
    if sys.platform.startswith("linux"):
        return "linux"
    return sys.platform


def parse_platforms(value):
    names = [p.strip().lower() for p in value.split(",") if p.strip()]
    if "all" in names:
        return list(PLATFORMS)
    unknown = [p for p in names if p not in PLATFORMS]
    if unknown:
        fatal("unknown platform(s): %s (choose from %s)"
              % (", ".join(unknown), ", ".join(PLATFORMS)))
    return [p for p in PLATFORMS if p in names]


def find_cargo():
    """Return (cargo path, environment) for a working toolchain.

    `cargo` is not always on PATH (rustup installed under a non-ASCII profile,
    or a custom CARGO_HOME), and the rustup shim needs RUSTUP_HOME when it has
    no default toolchain.  Try the usual locations and keep the first one that
    actually reports a version.
    """
    exe = "cargo.exe" if os.name == "nt" else "cargo"
    homes = []
    if os.environ.get("CARGO_HOME"):
        homes.append(Path(os.environ["CARGO_HOME"]))
    homes.extend([Path.home() / ".cargo", Path("C:/cargo"), Path("D:/cargo")])

    candidates = []
    on_path = shutil.which("cargo")
    if on_path:
        candidates.append(Path(on_path))
    candidates.extend(home / "bin" / exe for home in homes)

    for cargo in candidates:
        if not cargo.is_file():
            continue
        env = os.environ.copy()
        env["PATH"] = str(cargo.parent) + os.pathsep + env.get("PATH", "")
        if not env.get("CARGO_HOME"):
            env["CARGO_HOME"] = str(cargo.parent.parent)
        if not env.get("RUSTUP_HOME"):
            siblings = [
                cargo.parent.parent.parent / "rustup",
                Path.home() / ".rustup",
                Path("C:/rustup"),
                Path("D:/rustup"),
            ]
            for rustup in siblings:
                if (rustup / "toolchains").is_dir():
                    env["RUSTUP_HOME"] = str(rustup)
                    break
        try:
            proc = subprocess.run([str(cargo), "--version"], env=env,
                                  capture_output=True, text=True, timeout=30)
        except OSError:
            continue
        if proc.returncode == 0 and proc.stdout.strip():
            return cargo, env
    return None, None


def git(*args):
    return subprocess.check_output(["git", *args], cwd=str(ROOT), text=True).strip()


def current_ref():
    try:
        ref = git("symbolic-ref", "--short", "HEAD")
        return ref or "main"
    except subprocess.CalledProcessError:
        return "main"


def flatten_artifacts(out):
    """Move bundles out of the per-artifact directories created by gh."""
    for top in list(out.iterdir()):
        if not top.is_dir() or (top / "README.txt").exists():
            continue
        children = list(top.iterdir())
        for child in children:
            dest = out / child.name
            if dest.is_dir():
                shutil.rmtree(dest)
            elif dest.exists():
                dest.unlink()
            shutil.move(str(child), str(dest))
        top.rmdir()


def keep_only(out, platforms):
    """Drop bundles for platforms the caller did not ask for."""
    keep = {BUNDLES[p] for p in platforms}
    for entry in list(out.iterdir()):
        stem = entry.name[:-4] if entry.name.endswith(".zip") else entry.name
        if stem.startswith("rowline-") and stem not in keep:
            if entry.is_dir():
                shutil.rmtree(entry)
            else:
                entry.unlink()


def build_local(out, cargo, env):
    package = ROOT / "tools" / "package.py"
    cmd = [sys.executable, str(package), "--out", str(out), "--zip"]
    say("building the host platform: %s" % host_platform())
    say("  $ " + " ".join(cmd))
    subprocess.check_call(cmd, cwd=str(ROOT), env=env)


def wait_for_run(gh, sha, timeout=300):
    deadline = time.time() + timeout
    while time.time() < deadline:
        time.sleep(4)
        try:
            raw = subprocess.check_output(
                [gh, "run", "list", "--workflow", WORKFLOW, "--limit", "10",
                 "--json", "databaseId,headSha,status,conclusion"],
                cwd=str(ROOT), text=True)
        except subprocess.CalledProcessError as exc:
            fatal("gh could not list workflow runs (are you signed in?): %s" % exc)
        for run in json.loads(raw):
            if run.get("headSha") == sha:
                return run["databaseId"]
    return None


def build_ci(out, platforms, ref, allow_dirty):
    gh = shutil.which("gh")
    if not gh:
        fatal("GitHub CLI not found.  Install it (https://cli.github.com/) and "
              "run `gh auth login`, or pass --mode local to build only this host.")

    if git("status", "--porcelain") and not allow_dirty:
        fatal("the working tree has uncommitted changes.\n"
              "       Actions builds the pushed revision, not this tree.\n"
              "       Commit and push, or pass --allow-dirty to build the "
              "remote revision anyway.")

    sha = git("rev-parse", ref)
    say("starting %s on GitHub Actions (ref %s, %s)" % (WORKFLOW, ref, sha[:12]))
    try:
        subprocess.check_call([gh, "workflow", "run", WORKFLOW, "--ref", ref],
                              cwd=str(ROOT))
    except subprocess.CalledProcessError:
        fatal("could not start the workflow.  Push .github/workflows/%s to "
              "'%s' first, and check `gh auth status`." % (WORKFLOW, ref))

    run_id = wait_for_run(gh, sha)
    if run_id is None:
        fatal("the workflow run did not appear; open the repository's Actions tab.")
    say("watching run %s ..." % run_id)
    code = subprocess.call([gh, "run", "watch", str(run_id), "--exit-status",
                            "--interval", "15"], cwd=str(ROOT))
    if code != 0:
        fatal("the workflow failed (run %s).\n"
              "       Inspect it with: gh run view %s --log-failed"
              % (run_id, run_id))

    os.makedirs(out, exist_ok=True)
    say("downloading artifacts into %s" % out)
    subprocess.check_call([gh, "run", "download", str(run_id), "--dir", str(out)],
                          cwd=str(ROOT))
    flatten_artifacts(out)
    keep_only(out, platforms)
    say("done: %s" % out)


def main():
    ap = argparse.ArgumentParser(
        description="Build Rowline for Linux, macOS and Windows.")
    ap.add_argument("--mode", choices=("auto", "local", "ci"), default="auto",
                    help="auto = host locally + GitHub Actions for the rest")
    ap.add_argument("--platforms", default=None,
                    help="comma-separated: linux,macos,windows "
                         "(default: all; the host only in --mode local)")
    ap.add_argument("--out", default=str(ROOT / "dist"),
                    help="output directory (default: dist/)")
    ap.add_argument("--ref", default=None,
                    help="git branch/tag for CI (default: current branch)")
    ap.add_argument("--allow-dirty", action="store_true",
                    help="let CI run even with uncommitted changes")
    ap.add_argument("--list", action="store_true",
                    help="print the plan and exit")
    args = ap.parse_args()

    host = host_platform()
    if args.platforms:
        platforms = parse_platforms(args.platforms)
    elif args.mode == "local":
        platforms = [host]
    else:
        platforms = list(PLATFORMS)
    out = Path(args.out).resolve()
    ref = args.ref or current_ref()
    missing = [p for p in platforms if p != host]

    if args.list:
        say("host platform : %s" % host)
        say("requested     : %s" % ", ".join(platforms))
        say("output        : %s" % out)
        say("")
        say("plan:")
        for platform in platforms:
            if platform == host:
                say("  %-8s local build (tools/package.py)" % platform)
            else:
                say("  %-8s GitHub Actions (%s, ref %s)" % (platform, WORKFLOW, ref))
        if missing and not shutil.which("gh"):
            say("")
            say("note: `gh` is not installed; CI platforms will be skipped in auto mode.")
        return 0

    if args.mode == "local":
        if missing:
            fatal("local mode builds only the host (%s); requested: %s.\n"
                  "       Use --mode ci for %s." % (host, ", ".join(platforms),
                                                    ", ".join(missing)))
        cargo, env = find_cargo()
        if cargo is None:
            fatal("cargo not found.  Install Rust (https://rustup.rs/).")
        say("using %s" % cargo)
        build_local(out, cargo, env)
        keep_only(out, platforms)
        return 0

    if args.mode == "ci":
        build_ci(out, platforms, ref, args.allow_dirty)
        return 0

    # auto
    if not missing:
        cargo, env = find_cargo()
        if cargo is None:
            fatal("cargo not found.  Install Rust (https://rustup.rs/).")
        say("using %s" % cargo)
        build_local(out, cargo, env)
    elif shutil.which("gh"):
        build_ci(out, platforms, ref, args.allow_dirty)
    else:
        say("warning: `gh` is not installed; building only the host (%s)." % host)
        say("         Install GitHub CLI to build %s automatically, or run the"
            % ", ".join(missing))
        say("         Release build workflow from the repository's Actions tab.")
        cargo, env = find_cargo()
        if cargo is None:
            fatal("cargo not found.  Install Rust (https://rustup.rs/).")
        say("using %s" % cargo)
        build_local(out, cargo, env)
    keep_only(out, platforms)
    return 0


if __name__ == "__main__":
    sys.exit(main())
