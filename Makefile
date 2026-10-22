# Rowline re-implementation -- build
#
#   make replay   : headless replay CLI (no external deps) -- used by the
#                    differential replay gate (tools/replay/diff_replay.py)
#   make app      : SDL2 application (requires SDL2 dev headers)
#   make sheet    : regenerate the placeholder sprite sheet
#   make test     : run the differential replay gate
#   make package  : release build of the current platform + dist zip
#   make dist     : same, plus GitHub Actions for the other platforms (if gh)
#   make dist-ci  : build every platform on GitHub Actions
#
# On Windows all Python and cargo calls go through tools/win/*.ps1, so no
# global PATH / RUSTUP_HOME setup is required (the WindowsApps `python.exe`
# alias exits with 9009 -- the wrappers skip it).

CC      ?= cc
CFLAGS  ?= -std=c99 -O2 -Wall -Wextra -Wno-unused-parameter
SRC     := reference/c-impl
BUILD   := build

# On Windows every executable carries an .exe suffix, including the ones the
# C compiler and cargo produce.  Without this the recipes below work under an
# MSYS/Git-Bash shell but fail when make falls back to cmd.exe.
#
# Python: Windows does not ship `python3`, and the `python.exe` in
# WindowsApps is a Store alias that exits with code 9009.  On Windows every
# Python tool therefore goes through tools/win/py.ps1, which finds a real
# interpreter; `make dist` goes straight to tools/build_all.ps1, which also
# locates cargo (it is often installed outside PATH).
ifeq ($(OS),Windows_NT)
EXE     := .exe
PY      := powershell -NoProfile -ExecutionPolicy Bypass -File tools/win/py.ps1
CARGO   := powershell -NoProfile -ExecutionPolicy Bypass -File tools/win/cargo.ps1
DIST    := powershell -NoProfile -ExecutionPolicy Bypass -File tools/build_all.ps1 --mode local
DIST_CI := powershell -NoProfile -ExecutionPolicy Bypass -File tools/build_all.ps1 --mode ci
CROSS   := $(PY) tools/build_cross.py
else
EXE     :=
PYTHON  ?= python3
PY      := "$(PYTHON)"
CARGO   := cargo
DIST    := "$(PYTHON)" tools/build_all.py
DIST_CI := "$(PYTHON)" tools/build_all.py --mode ci
CROSS   := "$(PYTHON)" tools/build_cross.py
endif

# ------------------------------------------------------------------ replay
REPLAY_SRC := $(SRC)/wl_model.c $(SRC)/wl_replay.c
REPLAY_BIN := $(BUILD)/rowline-replay$(EXE)

# ------------------------------------------------------------------ app
APP_SRC := $(SRC)/wl_model.c $(SRC)/wl_layout.c $(SRC)/wl_render.c $(SRC)/wl_main.c
APP_BIN := $(BUILD)/rowline$(EXE)

# ------------------------------------------------------------------ checks
CHECK_SRC := $(SRC)/wl_model.c $(SRC)/wl_layout.c $(SRC)/wl_layout_check.c
CHECK_BIN := $(BUILD)/rowline-layout-check$(EXE)

# The C implementation under reference/c-impl is the *reference
# implementation* of the behaviour spec.  It is kept because the differential
# replay gate compares four independent implementations against each other;
# it is not the product.

# ------------------------------------------------------------------ rust
RUST_DIR  := rust
RUST_BIN  := $(RUST_DIR)/target/release/rowline-replay$(EXE)
SHOT_BIN  := $(RUST_DIR)/target/release/rowline-shot$(EXE)

# Decide at parse time whether the four-way gate can include Rust.  Doing this
# in make (instead of `if [ -x ... ]` in the recipe) keeps `make test` working
# when make uses cmd.exe rather than sh.
ifneq ($(strip $(wildcard $(RUST_BIN))),)
RUST_GATE := $(RUST_BIN)
else
RUST_GATE :=
endif

SDL_CFLAGS ?= $(shell sdl2-config --cflags 2>/dev/null)
SDL_LIBS   ?= $(shell sdl2-config --libs 2>/dev/null || echo -lSDL2)

.PHONY: all replay app check layout-check rust rust-test rust-replay sheet icons package dist dist-ci cross shot test clean

all: replay check

replay: $(REPLAY_BIN)

$(REPLAY_BIN): $(REPLAY_SRC) $(SRC)/wl_model.h | $(BUILD)
	$(CC) $(CFLAGS) -I$(SRC) -o $@ $(REPLAY_SRC)

check: $(CHECK_BIN)

$(CHECK_BIN): $(CHECK_SRC) $(SRC)/wl_model.h $(SRC)/wl_layout.h | $(BUILD)
	$(CC) $(CFLAGS) -I$(SRC) -o $@ $(CHECK_SRC)

layout-check: $(CHECK_BIN)
	$(CHECK_BIN)

# The C implementation under reference/c-impl is the *reference
# implementation* of the behaviour spec.  It is kept because the differential
# replay gate compares four independent implementations against each other;
# it is not the product.

# ------------------------------------------------------------------ rust
#
# Always pass --manifest-path instead of `cd $(RUST_DIR) && ...`.  On Windows
# `$(CARGO)` is `powershell ... -File tools/win/cargo.ps1`, a *relative* path:
# after a `cd rust` PowerShell would look for `rust/tools/win/cargo.ps1` and
# fail.  --manifest-path keeps the working directory at the repository root,
# so the wrapper resolves on every platform.
rust:
	$(CARGO) build --release --manifest-path $(RUST_DIR)/Cargo.toml

rust-test:
	$(CARGO) test --release --manifest-path $(RUST_DIR)/Cargo.toml

# Refresh an existing replay binary before the differential gate: trusting a
# stale binary made `make test` report a false divergence.
rust-replay:
	$(CARGO) build --release -p rowline-tools --manifest-path $(RUST_DIR)/Cargo.toml

app: $(APP_BIN)

$(APP_BIN): $(APP_SRC) $(SRC)/wl_model.h $(SRC)/wl_layout.h $(SRC)/wl_render.h | $(BUILD)
	$(CC) $(CFLAGS) -I$(SRC) $(SDL_CFLAGS) -o $@ $(APP_SRC) $(SDL_LIBS)

$(SHOT_BIN):
	$(CARGO) build --release -p rowline-tools --manifest-path $(RUST_DIR)/Cargo.toml

$(BUILD):
	$(PY) -c "import os; os.makedirs(r'$(BUILD)', exist_ok=True)"

sheet:
	$(PY) tools/make_sheet.py

icons:
	$(PY) tools/make_icons.py

# Package the current platform (rebuilds the release binary, adds a zip).
package:
	$(DIST)

# Three-platform build.  The host is built locally; the other platforms are
# built on native GitHub Actions runners (see .github/workflows/release.yml).
#
#   make dist      host locally + GitHub Actions for the rest when `gh` exists
#   make dist-ci   every platform on GitHub Actions (needs `gh` + a pushed commit)
dist:
	$(DIST)

dist-ci:
	$(DIST_CI)

# Build all three single-file downloads locally (macOS host only).
cross:
	$(CROSS)

# Render a frame into a PNG without a GPU or a display.
shot: $(SHOT_BIN)
	$(SHOT_BIN) --out $(BUILD)/shot.png --width 1280 --height 900
	@echo "wrote $(BUILD)/shot.png"

test: $(REPLAY_BIN) $(CHECK_BIN) $(if $(RUST_GATE),rust-replay)
	$(CHECK_BIN)
	@echo "$(if $(RUST_GATE),--- four-way differential gate (python / c / rust / golden) ---,--- rust not built; run 'make rust' for the four-way gate ---)"
	$(PY) tools/replay/diff_replay.py --all

clean:
	$(PY) -c "import shutil; shutil.rmtree(r'$(BUILD)', ignore_errors=True)"
	$(CARGO) clean --manifest-path $(RUST_DIR)/Cargo.toml
