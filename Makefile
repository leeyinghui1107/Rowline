# Rowline re-implementation -- build
#
#   make replay   : headless replay CLI (no external deps) -- used by the
#                    differential replay gate (tools/replay/diff_replay.py)
#   make app      : SDL2 application (requires SDL2 dev headers)
#   make sheet    : regenerate the placeholder sprite sheet
#   make test     : run the differential replay gate

CC      ?= cc
# The Python interpreter: Windows installs it as `python`, not `python3`.
PYTHON  ?= python3
CFLAGS  ?= -std=c99 -O2 -Wall -Wextra -Wno-unused-parameter
SRC     := reference/c-impl
BUILD   := build

# On Windows every executable carries an .exe suffix, including the ones the
# C compiler and cargo produce.  Without this the recipes below work under an
# MSYS/Git-Bash shell but fail when make falls back to cmd.exe.
ifeq ($(OS),Windows_NT)
EXE     := .exe
else
EXE     :=
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

.PHONY: all replay app check layout-check rust rust-test sheet icons package shot test clean

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
rust:
	cd $(RUST_DIR) && cargo build --release

rust-test:
	cd $(RUST_DIR) && cargo test --release

app: $(APP_BIN)

$(APP_BIN): $(APP_SRC) $(SRC)/wl_model.h $(SRC)/wl_layout.h $(SRC)/wl_render.h | $(BUILD)
	$(CC) $(CFLAGS) -I$(SRC) $(SDL_CFLAGS) -o $@ $(APP_SRC) $(SDL_LIBS)

$(SHOT_BIN):
	cd $(RUST_DIR) && cargo build --release -p rowline-tools

$(BUILD):
	"$(PYTHON)" -c "import os; os.makedirs(r'$(BUILD)', exist_ok=True)"

sheet:
	"$(PYTHON)" tools/make_sheet.py

icons:
	"$(PYTHON)" tools/make_icons.py

package:
	"$(PYTHON)" tools/package.py

# Render a frame into a PNG without a GPU or a display.
shot: $(SHOT_BIN)
	$(SHOT_BIN) --out $(BUILD)/shot.png --width 1280 --height 900
	@echo "wrote $(BUILD)/shot.png"

test: $(REPLAY_BIN) $(CHECK_BIN)
	$(CHECK_BIN)
	@echo "$(if $(RUST_GATE),--- four-way differential gate (python / c / rust / golden) ---,--- rust not built; run 'make rust' for the four-way gate ---)"
	"$(PYTHON)" tools/replay/diff_replay.py --all

clean:
	"$(PYTHON)" -c "import shutil; shutil.rmtree(r'$(BUILD)', ignore_errors=True)"
	cd $(RUST_DIR) && cargo clean
