#!/usr/bin/env python3
"""
Trace format + Python reference implementation of the WinLinez board model.

The trace format is deliberately line-based TSV so that the C implementation
can parse it without pulling in a JSON dependency.

TRACE (input)
    seed   <u32>
    board  <81 chars>        optional; 0-7 = colour, '.' = empty
    op move <r0> <c0> <r1> <c1>
    op click <r> <c>
    op newgame
    op setusefigures <0|1>

RESULT (output, one line per event)
    state <step> <board81> <score>
    elim <step> <count> <r,c> <r,c> ... <delta>
    path <step> <r,c> <r,c> ...
    note <step> <text>

The rule model is *parameterised* because several semantics are still TBD
(see docs/03 Feature Spec, section 7).  The switches below are the knobs the
black-box phase (docs/01, BB-G03..G08) will pin down.
"""
from dataclasses import dataclass, field

BOARD_SIDE = 9
BOARD_CELLS = BOARD_SIDE * BOARD_SIDE
COLORS = 7
THRESHOLD = 5
#: the number of balls the game previews as the "next group"
GROUP_SIZE = 3

# ------------------------------------------------------------------ switches
@dataclass
class RuleModel:
    """Behavioural switches for the (still partly unknown) rule semantics."""
    threshold: int = THRESHOLD
    #: the source cell of a moved ball keeps its colour (typical "Lines" rule)
    source_keeps_colour: bool = True
    #: a path may turn at most this many times (-1 = unlimited)
    max_turns: int = -1
    #: refill count after an elimination
    refill: int = 0
    #: how many balls a fresh board starts with (the original uses five)
    initial_balls: int = 5
    #: score awarded per eliminated ball
    points_per_ball: int = 1
    #: minimum score delta for a line to count
    min_score_delta: int = 0
    #: allow the path to pass through the source cell
    allow_revisit_source: bool = True

# ------------------------------------------------------------------ RNG
class Lcg:
    """Borland/ANSI rand(): state = state*0x41C64E6D + 0x3039 (mod 2^32)."""
    MUL = 0x41C64E6D
    INC = 0x3039

    def __init__(self, seed=0):
        self.state = seed & 0xFFFFFFFF

    def next(self, n: int) -> int:
        self.state = (self.state * self.MUL + self.INC) & 0xFFFFFFFF
        v = (self.state >> 16) & 0x7FFF
        return v % n if n else 0

    def colour(self) -> int:
        return 1 + self.next(COLORS)

# ------------------------------------------------------------------ board
NEIGHBOURS = [(-1, 0), (-1, -1), (0, -1), (1, -1)]   # left, up-left, up, up-right

def idx(r: int, c: int) -> int:
    return r * BOARD_SIDE + c

def rc(i: int):
    return divmod(i, BOARD_SIDE)

def in_bounds(r: int, c: int) -> bool:
    return 0 <= r < BOARD_SIDE and 0 <= c < BOARD_SIDE

def fmt_board(board) -> str:
    return "".join("." if v == 0 else str(v) for v in board)

def parse_board(s: str):
    s = s.strip()
    if len(s) != BOARD_CELLS:
        raise ValueError("board must be %d cells, got %d" % (BOARD_CELLS, len(s)))
    out = []
    for ch in s:
        out.append(0 if ch in "._" else int(ch, 16))
    return out

class Board:
    def __init__(self, cells=None):
        self.cells = list(cells) if cells else [0] * BOARD_CELLS

    def copy(self):
        return Board(self.cells)

    def get(self, r, c):
        return self.cells[idx(r, c)] if in_bounds(r, c) else 0

    def set(self, r, c, v):
        if in_bounds(r, c):
            self.cells[idx(r, c)] = v

    def full(self):
        return all(v != 0 for v in self.cells)

    def empty_cells(self):
        return [i for i, v in enumerate(self.cells) if v == 0]

# ------------------------------------------------------------------ rules
def connected_set(board: Board, start: int, colour: int, threshold: int):
    """Flood-fill the connected component of `colour` containing `start`."""
    if board.cells[start] != colour:
        return []
    seen = {start}
    stack = [start]
    out = []
    while stack:
        cur = stack.pop()
        out.append(cur)
        r, c = rc(cur)
        for dr, dc in ((-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (-1, 1), (1, -1), (1, 1)):
            nr, nc = r + dr, c + dc
            if not in_bounds(nr, nc):
                continue
            n = idx(nr, nc)
            if n in seen or board.cells[n] != colour:
                continue
            seen.add(n)
            stack.append(n)
    return out if len(out) >= threshold else []

def find_path(board: Board, src: int, dst: int, colour: int, model: RuleModel):
    """
    Search a path from `src` to `dst` over empty cells (8-connected).
    Returns the list of visited cell indices (excluding src, including dst), or [].
    Turn counting uses the 4-neighbour table recovered from the binary.
    """
    sr, sc = rc(src)
    dr, dc = rc(dst)
    if not (in_bounds(dr, dc) and board.cells[idx(dr, dc)] == 0):
        return []
    # BFS over states (cell, incoming direction) to count turns
    start = src
    best = {}
    from collections import deque
    q = deque()
    for d in range(8):
        q.append((start, d, [start]))
        best[(start, d)] = None
    while q:
        cur, d, path = q.popleft()
        r, c = rc(cur)
        if cur == idx(dr, dc) and cur != start:
            return path
        for nd in range(8):
            ndr = r + (nd // 3 - 1) if False else r + [-1, -1, 0, 1, 1, 1, 0, -1][nd]
            ndc = c + [-1, 0, 1, 1, 0, -1, -1, -1][nd]
            if not in_bounds(ndr, ndc):
                continue
            n = idx(ndr, ndc)
            if n == start and not model.allow_revisit_source:
                continue
            if n != dst and board.cells[n] != 0:
                continue
            turns = path_turns(path, nd)
            if model.max_turns >= 0 and turns > model.max_turns:
                continue
            key = (n, nd)
            if key in best:
                continue
            best[key] = (cur, d, path)
            q.append((n, nd, path + [n]))
    return []

DIRS8 = [(-1, 0), (-1, -1), (0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1)]

def path_turns(path, incoming_dir):
    """Number of direction changes along `path` when arriving with `incoming_dir`."""
    if len(path) < 2:
        return 0
    turns = 0
    prev_dir = None
    for i in range(1, len(path) + 1):
        if i == len(path):
            cur = path[-1]
            d = incoming_dir
        else:
            cur = path[i]
            r0, c0 = rc(path[i - 1])
            r1, c1 = rc(cur)
            d = DIRS8.index((r1 - r0, c1 - c0))
        if prev_dir is not None and d != prev_dir:
            turns += 1
        prev_dir = d
    return turns

# ------------------------------------------------------------------ engine
@dataclass
class Event:
    step: int
    kind: str
    payload: dict = field(default_factory=dict)

class Engine:
    def __init__(self, board: Board, score=0, seed=0, model: RuleModel = None,
                 use_figures=0):
        self.board = board
        self.score = score
        self.rng = Lcg(seed)
        self.model = model or RuleModel()
        self.use_figures = use_figures
        self.events = []
        self.step = 0
        self.next_group = [0] * GROUP_SIZE

    # -- helpers -----------------------------------------------------
    def _emit(self, kind, **kw):
        self.events.append(Event(self.step, kind, kw))

    def _new_round(self):
        """Generate a fresh board.

        The original's new game puts **five** random balls on the empty
        board (docs/11 section 5c) and then rolls the next group of three.
        The RNG is consumed in exactly this order by the Python, C and Rust
        implementations, so all three stay bit-identical.
        """
        placed = 0
        guard = 0
        while placed < self.model.initial_balls and guard < BOARD_CELLS * 4:
            guard += 1
            pick = self.rng.next(BOARD_CELLS)
            if self.board.cells[pick] != 0:
                continue
            self.board.cells[pick] = self.rng.colour()
            placed += 1
        self.roll_next_group()

    def roll_next_group(self):
        """Roll a fresh group of balls to be placed next."""
        for i in range(GROUP_SIZE):
            self.next_group[i] = self.rng.colour()

    def take_next_ball(self):
        """Take the next ball from the queue and roll a replacement."""
        ball = self.next_group[0]
        for i in range(GROUP_SIZE - 1):
            self.next_group[i] = self.next_group[i + 1]
        self.next_group[GROUP_SIZE - 1] = self.rng.colour()
        return ball

    # -- operations --------------------------------------------------
    def op_newgame(self):
        self.board = Board()
        self._new_round()
        self.score = 0
        self._emit("state", board=fmt_board(self.board.cells), score=self.score)

    def op_move(self, r0, c0, r1, c1):
        src, dst = idx(r0, c0), idx(r1, c1)
        colour = self.board.get(r0, c0)
        if colour == 0 or self.board.get(r1, c1) != 0:
            self._emit("note", text="illegal move")
            return
        path = find_path(self.board, src, dst, colour, self.model)
        if not path:
            self._emit("note", text="no path")
            return
        self.board.set(r1, c1, colour)
        if not self.model.source_keeps_colour:
            self.board.set(r0, c0, 0)
        self._emit("path", cells=[rc(i) for i in path])
        group = connected_set(self.board, dst, colour, self.model.threshold)
        if not group:
            self._emit("state", board=fmt_board(self.board.cells), score=self.score)
            return
        delta = len(group) * self.model.points_per_ball
        if delta < self.model.min_score_delta:
            self._emit("state", board=fmt_board(self.board.cells), score=self.score)
            return
        for i in group:
            self.board.set(*rc(i), 0)
        self.score += delta
        self._emit("elim", cells=[rc(i) for i in sorted(group)], delta=delta)
        for _ in range(self.model.refill):
            if self.board.is_full():
                break
            pick = self.rng.next(BOARD_CELLS)
            colour = self.take_next_ball()
            self.board.set(*rc(pick), colour)
        self._emit("state", board=fmt_board(self.board.cells), score=self.score)

    def op_click(self, r, c):
        self._emit("note", text="click %d,%d" % (r, c))

    def op_setusefigures(self, v):
        self.use_figures = v
        self._emit("note", text="usefigures %d" % v)

    # -- driver ------------------------------------------------------
    def run(self, ops):
        for op in ops:
            self.step += 1
            t = op[0]
            if t == "move":
                self.op_move(*op[1:])
            elif t == "click":
                self.op_click(*op[1:])
            elif t == "newgame":
                self.op_newgame()
            elif t == "setusefigures":
                self.op_setusefigures(*op[1:])
        return self.events

    # -- serialisation ----------------------------------------------
    def dump(self):
        lines = []
        for e in self.events:
            if e.kind == "state":
                lines.append("state %d %s %d" % (e.step, e.payload["board"], e.payload["score"]))
            elif e.kind == "elim":
                cs = " ".join("%d,%d" % tuple(rc) for rc in e.payload["cells"])
                lines.append("elim %d %d %s %d" % (e.step, len(e.payload["cells"]), cs, e.payload["delta"]))
            elif e.kind == "path":
                cs = " ".join("%d,%d" % tuple(rc) for rc in e.payload["cells"])
                lines.append("path %d %s" % (e.step, cs))
            else:
                lines.append("note %d %s" % (e.step, e.payload.get("text", "")))
        return "\n".join(lines)

# ------------------------------------------------------------------ trace IO
def parse_trace(text):
    seed, board, ops = 0, None, []
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        k = parts[0]
        if k == "seed":
            seed = int(parts[1], 0)
        elif k == "board":
            board = parse_board(parts[1])
        elif k == "op":
            parts = parts[1:]
            kind = parts[0]
            nums = tuple(int(x) for x in parts[1:])
            ops.append((kind,) + nums)
    return seed, board, ops

def load_trace(path):
    return parse_trace(open(path, encoding="utf-8").read())

def run_trace(seed, board, ops, model=None):
    eng = Engine(Board(board) if board else Board(), score=0, seed=seed, model=model)
    if board is None:
        eng.op_newgame()
    eng.run(ops)
    return eng
