/*
 * wl_model.h -- WinLinez board model and rules (behavioural re-implementation).
 *
 * This is an INDEPENDENT re-implementation written from the behaviour spec
 * (docs/03_FEATURE_SPEC.md).  It contains no original code.
 *
 * The rule semantics are parameterised because several details are still TBD
 * (docs/01, BB-G03..BB-G08).  See wl_rule_model below.
 */
#ifndef WL_MODEL_H
#define WL_MODEL_H

#include <stdint.h>

#define WL_SIDE        9
#define WL_CELLS       (WL_SIDE * WL_SIDE)
#define WL_COLORS      7
#define WL_THRESHOLD   5
/* The number of balls the game previews as the "next group". */
#define WL_GROUP_SIZE   3

/* Cell codes: 0 = empty, 1..7 = colour. */
#define WL_EMPTY 0

/* ------------------------------------------------------------------ RNG */
/* Borland/ANSI rand(): state = state * 0x41C64E6D + 0x3039 (mod 2^32). */
typedef struct { uint32_t state; } wl_rng;

void     wl_rng_seed(wl_rng *r, uint32_t seed);
uint32_t wl_rng_next(wl_rng *r);
int      wl_rng_below(wl_rng *r, int n);
int      wl_rng_colour(wl_rng *r);

/* ------------------------------------------------------------------ board */
typedef struct {
    uint8_t cell[WL_CELLS];
} wl_board;

int  wl_idx(int r, int c);
int  wl_rc(int i, int *r, int *c);
int  wl_in_bounds(int r, int c);
int  wl_board_get(const wl_board *b, int r, int c);
void wl_board_set(wl_board *b, int r, int c, int v);
int  wl_board_full(const wl_board *b);

/* ------------------------------------------------------------------ rules */
typedef struct {
    int threshold;             /* cells needed in a straight run       */
    int spawn_per_move;        /* balls placed after a non-clearing turn */
    int initial_balls;         /* balls placed on a fresh board (the original uses 5) */
    int source_keeps_colour;   /* experiments only; the original clears the source */
} wl_rule_model;

void wl_rule_model_default(wl_rule_model *m);

/*
 * Every straight run of `colour` through `start` that is at least
 * `threshold` cells long.  Writes the union of the marked cells (sorted
 * ascending) to `out_cells` and returns the sum of the qualifying run
 * lengths, or 0 when no run qualifies.
 *
 * The original scans four axes (vertical, main diagonal, horizontal,
 * anti-diagonal) in both directions; a connected blob that is not a
 * straight line does not clear.
 */
int wl_find_lines(const wl_board *b, int start, int colour, int threshold,
                  int *out_cells, int *out_count);

/* Score for a clear whose qualifying run lengths sum to `total`. */
int wl_score_for_run(int total);

/*
 * Find a shortest 4-connected path from `src` to `dst` through empty cells.
 * Returns the full path length (including source and destination), or 0 if
 * no path exists.  The path is written to `out_path` up to `out_cap` cells.
 */
int wl_find_path(const wl_board *b, int src, int dst,
                 int *out_path, int out_cap);

/* ------------------------------------------------------------------ engine */
typedef struct {
    int status;         /* 0 = illegal move, 1 = no path, 2 = moved     */
    int path_len;       /* cells on the travelled path (incl. source)  */
    int path[WL_CELLS * 2];
    int group_len;      /* cells eliminated (0 if none)                */
    int group[WL_CELLS]; /* sorted ascending                           */
    int delta;          /* score delta                                 */
} wl_move_result;

typedef struct {
    wl_board board;
    unsigned char next_group[WL_GROUP_SIZE];
    int      score;
    int      use_figures;
    wl_rng   rng;
    wl_rule_model model;
    int      step;
} wl_engine;

void wl_engine_init(wl_engine *e, uint32_t seed, const wl_rule_model *m);
void wl_engine_new_game(wl_engine *e);
void wl_engine_move_ex(wl_engine *e, int r0, int c0, int r1, int c1,
                       wl_move_result *res);
void wl_engine_move(wl_engine *e, int r0, int c0, int r1, int c1,
                    int *out_group, int group_cap, int *out_delta,
                    int *out_path, int path_cap);
void wl_engine_click(wl_engine *e, int r, int c);
void wl_engine_set_use_figures(wl_engine *e, int v);

#endif /* WL_MODEL_H */
