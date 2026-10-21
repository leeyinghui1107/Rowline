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
    int threshold;             /* cells needed to clear                */
    int source_keeps_colour;   /* moved ball's old cell stays coloured */
    int max_turns;             /* -1 = unlimited                       */
    int refill;                /* balls added after an elimination     */
    int initial_balls;         /* balls placed on a fresh board (the original uses 5) */
    int points_per_ball;       /* score per eliminated ball            */
    int min_score_delta;       /* minimum delta for a line to count    */
    int allow_revisit_source;  /* path may pass through the source cell */
} wl_rule_model;

void wl_rule_model_default(wl_rule_model *m);

/* Largest connected same-colour component containing `start`, or 0. */
int wl_connected_group(const wl_board *b, int start, int colour, int threshold,
                       int *out_cells);

/*
 * Find a path from `src` to `dst` over empty cells (8-connected).
 * Returns the number of cells written to `out_path` (excluding src, including
 * dst), or 0 if no path exists.  Mirrors the reference implementation's
 * breadth-first ordering exactly.
 */
int wl_find_path(const wl_board *b, int src, int dst, const wl_rule_model *m,
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
