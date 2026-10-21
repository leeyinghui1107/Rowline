#!/usr/bin/env python3
"""
Trace format + Python reference implementation of the WinLinez board model.

The trace format is deliberately line-based TSV so that the C implementation
can parse it without pulling in a JSON dependency.

TRACE (input)
    seed   <u32>
    board  <81 chars>        optional; 0-7 = colour, '.' = empty
    op move <r0> <c0> <r1> <c1>
    op click <r>
    op newgame
    op setusefigures <0|1>

RESULT (output, one line per event)
    state <step> <board81> <score>
    elim <step> <count> <r,c> <r,c> ... <delta>
    path <step> <r,c> <r,c> ...
    note <step> <text>

Rule facts pinned down by black-box runs against WinLinez v1.30 (2026-10-21):

* a ball may move to any empty cell of the same 4-connected empty region;
* the moved ball's source cell becomes empty;
* a turn that creates a straight run of >= 5 same-coloured balls along one of
  the four axes (row, column, both diagonals) clears the whole run and does
  **not** consume the next-group preview;
* a turn that creates no line places the three preview balls, one at a time,
  at uniformly random empty cells and rolls three replacement colours;
* score for a clear of total run length L:
      5 -> 10, 6 -> 12, 7 -> 18, 8 -> 28, 9 -> 42, ...
  (increments 2, 6, 10, 14, ...)
"""
from dataclasses import dataclass, field

BOARD_SIDE = 9
BOARD_CELLS = BOARD_SIDE * BOARD_SIDE
COLORS = 7
THRESHOLD = 5
#: the number of balls the game previews as the "next group"
GROUP_SIZE = 3
#: a fresh board starts with the first five balls of the same random stream
INITIAL_BALLS = 5

# ------------------------------------------------------------------ switches
@dataclass
class RuleModel:
    """Behavioural switches that are still useful for experiments.

    The original's confirmed values are the defaults.  There is deliberately
    no "linear points per ball" knob any more: the score table was recovered
    from the binary and confirmed with a black-box 5-line clear (+10).
    """
    threshold: int = THRESHOLD
    #: balls placed after a turn that creates no line
    spawn_per_move: int = GROUP_SIZE
    #: how many balls a fresh board starts with (the original uses five)
    initial_balls: int = INITIAL_BALLS
    #: kept for experiments; the original always empties the source cell
    source_keeps_colour: bool = False

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
#: The four axes scanned by the original's line detector (each axis is
#: scanned in both directions).  Order: vertical, main diagonal, horizontal,
#: anti-diagonal -- the order recovered from the binary's direction table.
AXES = [(-1, 0), (-1, -1), (0, -1), (1, -1)]
#: 4-neighbour step order used by the path search.
STEPS4 = [(-1, 0), (1, 0), (0, -1), (0, 1)]

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
def find_lines(board: Board, start: int, colour: int, threshold: int = THRESHOLD):
    """Find every straight run through `start` that is at least `threshold`.

    Returns ``(total_length, cells)`` where ``total_length`` is the sum of the
    lengths of all qualifying runs (the original feeds this sum to its score
    function) and ``cells`` is the union of the marked cells, sorted.
    """
    if board.cells[start] != colour:
        return 0, []
    r0, c0 = rc(start)
    marked = set()
    total = 0
    for dr, dc in AXES:
        run = [start]
        for sign in (1, -1):
            r, c = r0 + sign * dr, c0 + sign * dc
            while in_bounds(r, c) and board.get(r, c) == colour:
                run.append(idx(r, c))
                r += sign * dr
                c += sign * dc
        if len(run) >= threshold:
            total += len(run)
            marked.update(run)
    return total, sorted(marked)

def score_for_run(total: int) -> int:
    """Score for a clear whose qualifying run lengths sum to `total`.

    Recovered from Auto+0x1e1d: 10, 12, 18, 28, 42, ... for 5, 6, 7, 8, 9.
    """
    if total < THRESHOLD:
        return 0
    score = 10
    step = 2
    for _ in range(total - THRESHOLD):
        score += step
        step += 4
    return score

def find_path(board: Board, src: int, dst: int):
    """Shortest 4-connected path through empty cells, including source."""
    if src == dst or board.cells[dst] != 0:
        return []
    from collections import deque
    parent = [-1] * BOARD_CELLS
    seen = [False] * BOARD_CELLS
    seen[src] = True
    q = deque([src])
    while q:
        cur = q.popleft()
        if cur == dst:
            break
        r, c = rc(cur)
        for dr, dc in STEPS4:
            nr, nc = r + dr, c + dc
            if not in_bounds(nr, nc):
                continue
            n = idx(nr, nc)
            if seen[n]:
                continue
            if n != dst and board.cells[n] != 0:
                continue
            seen[n] = True
            parent[n] = cur
            q.append(n)
    if not seen[dst]:
        return []
    path = []
    cur = dst
    while cur != -1:
        path.append(cur)
        if cur == src:
            break
        cur = parent[cur]
    if not path or path[-1] != src:
        return []
    return path[::-1]

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

        The original rolls the three-ball preview first and then places the
        first five balls from that same stream: each placement consumes the
        queue head and immediately rolls a replacement colour.  After five
        placements the preview holds the next three colours.
        """
        self.roll_next_group()
        for _ in range(self.model.initial_balls):
            self._spawn_until_quiet()

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

    def _spawn_one(self) -> bool:
        """Place one preview ball at a uniformly random empty cell.

        Returns True when the placement created a line (which is cleared
        without scoring, exactly like the original's refill routine), so the
        caller must place another ball.  Returns False after a quiet
        placement, when the board is full, or when no preview remains.
        """
        empty = self.board.empty_cells()
        if not empty:
            return False
        pick = empty[self.rng.next(len(empty))]
        colour = self.take_next_ball()
        self.board.cells[pick] = colour
        total, cells = find_lines(self.board, pick, colour, self.model.threshold)
        if total == 0:
            return False
        for i in cells:
            self.board.cells[i] = 0
        return True

    def _spawn_until_quiet(self):
        """The original's refill routine keeps placing while it clears."""
        guard = 0
        while self._spawn_one():
            guard += 1
            if guard > BOARD_CELLS:
                break

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
        path = find_path(self.board, src, dst)
        if not path:
            self._emit("note", text="no path")
            return
        self.board.set(r1, c1, colour)
        if not self.model.source_keeps_colour:
            self.board.set(r0, c0, 0)
        self._emit("path", cells=[rc(i) for i in path])

        total, cells = find_lines(self.board, dst, colour, self.model.threshold)
        if total >= self.model.threshold:
            for i in cells:
                self.board.cells[i] = 0
            delta = score_for_run(total)
            self.score += delta
            self._emit("elim", cells=[rc(i) for i in cells], delta=delta)
            # A clearing move does NOT consume the preview group.
        else:
            for _ in range(self.model.spawn_per_move):
                self._spawn_until_quiet()
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
