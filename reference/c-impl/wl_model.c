/*
 * wl_model.c -- board model and rules.  Independent re-implementation.
 * Behaviour source: docs/03_FEATURE_SPEC.md (sections 4, 7, 11).
 */
#include "wl_model.h"
#include <string.h>
#include <stdlib.h>

/* ------------------------------------------------------------------ RNG */
void wl_rng_seed(wl_rng *r, uint32_t seed) { r->state = seed; }

uint32_t wl_rng_next(wl_rng *r)
{
    r->state = r->state * 0x41C64E6Du + 0x3039u;
    return (r->state >> 16) & 0x7FFFu;
}

int wl_rng_below(wl_rng *r, int n)
{
    if (n <= 0) return 0;
    return (int)(wl_rng_next(r) % (uint32_t)n);
}

int wl_rng_colour(wl_rng *r) { return 1 + wl_rng_below(r, WL_COLORS); }

/* ------------------------------------------------------------------ board */
int wl_idx(int r, int c) { return r * WL_SIDE + c; }

int wl_rc(int i, int *r, int *c) { *r = i / WL_SIDE; *c = i % WL_SIDE; return i; }

int wl_in_bounds(int r, int c)
{
    return r >= 0 && r < WL_SIDE && c >= 0 && c < WL_SIDE;
}

int wl_board_get(const wl_board *b, int r, int c)
{
    if (!wl_in_bounds(r, c)) return 0;
    return b->cell[r * WL_SIDE + c];
}

void wl_board_set(wl_board *b, int r, int c, int v)
{
    if (wl_in_bounds(r, c)) b->cell[r * WL_SIDE + c] = (uint8_t)v;
}

int wl_board_full(const wl_board *b)
{
    int i;
    for (i = 0; i < WL_CELLS; i++)
        if (b->cell[i] == 0) return 0;
    return 1;
}

/* ------------------------------------------------------------------ rules */
void wl_rule_model_default(wl_rule_model *m)
{
    m->threshold           = WL_THRESHOLD;
    m->source_keeps_colour = 1;
    m->max_turns           = -1;
    m->refill              = 0;
    m->initial_balls       = 5;
    m->points_per_ball     = 1;
    m->min_score_delta     = 0;
    m->allow_revisit_source= 1;
}

static const int NBR8_R[8] = { -1,  1,  0,  0, -1, -1,  1,  1 };
static const int NBR8_C[8] = {  0,  0, -1,  1, -1,  1, -1,  1 };

/* 8 directions in the canonical order used by the reference implementation. */
static const int DIR_R[8] = { -1, -1,  0,  1,  1,  1,  0, -1 };
static const int DIR_C[8] = { -1,  0,  1,  1,  0, -1, -1, -1 };

int wl_connected_group(const wl_board *b, int start, int colour, int threshold,
                       int *out_cells)
{
    int stack[WL_CELLS], sp = 0;
    uint8_t seen[WL_CELLS];
    int count = 0, i;

    memset(seen, 0, sizeof seen);
    if (b->cell[start] != colour) return 0;

    stack[sp++] = start;
    seen[start] = 1;
    while (sp > 0) {
        int cur = stack[--sp];
        int r, c, k;
        wl_rc(cur, &r, &c);
        out_cells[count++] = cur;
        for (k = 0; k < 8; k++) {
            int nr = r + NBR8_R[k], nc = c + NBR8_C[k], n;
            if (!wl_in_bounds(nr, nc)) continue;
            n = wl_idx(nr, nc);
            if (seen[n] || b->cell[n] != colour) continue;
            seen[n] = 1;
            stack[sp++] = n;
        }
    }
    (void)i;
    return (count >= threshold) ? count : 0;
}

/* Direction index between two adjacent cells, or -1. */
static int dir_between(int r0, int c0, int r1, int c1)
{
    int k;
    for (k = 0; k < 8; k++)
        if (DIR_R[k] == (r1 - r0) && DIR_C[k] == (c1 - c0)) return k;
    return -1;
}

/*
 * Number of direction changes along `path`, where the final edge arrives with
 * direction `incoming`.  Kept byte-for-byte equivalent to the reference
 * implementation so that a non-default max_turns produces identical results.
 */
static int path_turns(const int *path, int len, int incoming)
{
    int turns = 0, prev_dir = -1, i;
    if (len < 2) return 0;
    for (i = 1; i <= len; i++) {
        int d;
        if (i == len) {
            d = incoming;
        } else {
            int r0, c0, r1, c1;
            wl_rc(path[i - 1], &r0, &c0);
            wl_rc(path[i], &r1, &c1);
            d = dir_between(r0, c0, r1, c1);
        }
        if (prev_dir >= 0 && d != prev_dir) turns++;
        prev_dir = d;
    }
    return turns;
}

typedef struct {
    int cell;
    int dir;
    int path[WL_CELLS * 2];
    int len;
} wl_qnode;

int wl_find_path(const wl_board *b, int src, int dst, const wl_rule_model *m,
                 int *out_path, int out_cap)
{
    static uint8_t visited[WL_CELLS * 8];
    static wl_qnode q[WL_CELLS * 8 * 8];
    int qh = 0, qt = 0, d;
    int dr = dst / WL_SIDE, dc = dst % WL_SIDE;

    if (!wl_in_bounds(dr, dc)) return 0;
    if (b->cell[dst] != 0) return 0;

    memset(visited, 0, sizeof visited);
    for (d = 0; d < 8; d++) {
        wl_qnode *n = &q[qt++];
        n->cell = src; n->dir = d; n->len = 1; n->path[0] = src;
        visited[src * 8 + d] = 1;
    }

    while (qh < qt) {
        wl_qnode cur = q[qh++];
        int r, c, nd;
        wl_rc(cur.cell, &r, &c);
        if (cur.cell == dst && cur.cell != src) {
            int i;
            for (i = 0; i < cur.len && i < out_cap; i++) out_path[i] = cur.path[i];
            return cur.len;
        }
        for (nd = 0; nd < 8; nd++) {
            int nr = r + DIR_R[nd], nc = c + DIR_C[nd], n, key, turns;
            wl_qnode *nn;
            if (!wl_in_bounds(nr, nc)) continue;
            n = wl_idx(nr, nc);
            if (n == src && !m->allow_revisit_source) continue;
            if (n != dst && b->cell[n] != 0) continue;
            turns = path_turns(cur.path, cur.len, nd);
            if (m->max_turns >= 0 && turns > m->max_turns) continue;
            key = n * 8 + nd;
            if (visited[key]) continue;
            visited[key] = 1;
            nn = &q[qt++];
            nn->cell = n; nn->dir = nd; nn->len = cur.len;
            memcpy(nn->path, cur.path, sizeof(int) * (size_t)cur.len);
            nn->path[cur.len] = n;
            nn->len = cur.len + 1;
        }
    }
    return 0;
}

/* ------------------------------------------------------------------ engine */
void wl_engine_init(wl_engine *e, uint32_t seed, const wl_rule_model *m)
{
    memset(&e->board, 0, sizeof e->board);
    e->score = 0;
    e->use_figures = 0;
    e->step = 0;
    wl_rng_seed(&e->rng, seed);
    if (m) e->model = *m; else wl_rule_model_default(&e->model);
}

void wl_roll_next_group(wl_engine *e)
{
    int i;
    for (i = 0; i < WL_GROUP_SIZE; i++)
        e->next_group[i] = wl_rng_colour(&e->rng);
}

unsigned char wl_take_next_ball(wl_engine *e)
{
    unsigned char ball = e->next_group[0];
    int i;
    for (i = 0; i < WL_GROUP_SIZE - 1; i++)
        e->next_group[i] = e->next_group[i + 1];
    e->next_group[WL_GROUP_SIZE - 1] = wl_rng_colour(&e->rng);
    return ball;
}

/*
 * A fresh board holds a handful of random balls, not a full one: the
 * original's new game places five (docs/11 section 5c) and then rolls the
 * next group of three.  The RNG is consumed in exactly this order by the
 * Python, C and Rust implementations, so all three stay bit-identical.
 */
void wl_engine_new_game(wl_engine *e)
{
    int placed = 0;
    int guard = 0;
    memset(&e->board, 0, sizeof e->board);
    e->score = 0;
    while (placed < e->model.initial_balls && guard < WL_CELLS * 4) {
        int pick;
        guard++;
        pick = (int)wl_rng_below(&e->rng, WL_CELLS);
        if (e->board.cell[pick] != 0)
            continue;
        e->board.cell[pick] = (uint8_t)wl_rng_colour(&e->rng);
        placed++;
    }
    wl_roll_next_group(e);
}

void wl_engine_move_ex(wl_engine *e, int r0, int c0, int r1, int c1,
                       wl_move_result *res)
{
    int src = wl_idx(r0, c0), dst = wl_idx(r1, c1);
    int colour = wl_board_get(&e->board, r0, c0);
    int group[WL_CELLS], n, i, delta;

    memset(res, 0, sizeof *res);
    res->status = 0;
    if (colour == 0 || wl_board_get(&e->board, r1, c1) != 0) return;  /* illegal */

    n = wl_find_path(&e->board, src, dst, &e->model, res->path, WL_CELLS * 2);
    if (n == 0) { res->status = 1; return; }                          /* no path */
    res->path_len = n;
    res->status = 2;

    wl_board_set(&e->board, r1, c1, colour);
    if (!e->model.source_keeps_colour)
        wl_board_set(&e->board, r0, c0, 0);

    n = wl_connected_group(&e->board, dst, colour, e->model.threshold, group);
    if (n == 0) return;

    delta = n * e->model.points_per_ball;
    if (delta < e->model.min_score_delta) return;

    for (i = 1; i < n; i++) {                 /* deterministic ordering */
        int key = group[i], j = i - 1;
        while (j >= 0 && group[j] > key) { group[j + 1] = group[j]; j--; }
        group[j + 1] = key;
    }
    for (i = 0; i < n; i++) res->group[i] = group[i];
    res->group_len = n;

    for (i = 0; i < n; i++)
        wl_board_set(&e->board, group[i] / WL_SIDE, group[i] % WL_SIDE, 0);

    e->score += delta;
    res->delta = delta;

    for (i = 0; i < e->model.refill; i++) {
        int pick;
        unsigned char colour;
        if (wl_board_full(&e->board)) break;
        pick = (int)wl_rng_below(&e->rng, WL_CELLS);
        colour = wl_take_next_ball(e);
        wl_board_set(&e->board, pick / WL_SIDE, pick % WL_SIDE, colour);
    }
}

void wl_engine_move(wl_engine *e, int r0, int c0, int r1, int c1,
                    int *out_group, int group_cap, int *out_delta,
                    int *out_path, int path_cap)
{
    wl_move_result res;
    int i, n;
    wl_engine_move_ex(e, r0, c0, r1, c1, &res);
    if (out_delta) *out_delta = res.delta;
    n = res.group_len;
    if (out_group)
        for (i = 0; i < n && i < group_cap; i++) out_group[i] = res.group[i];
    if (out_path)
        for (i = 0; i < res.path_len && i < path_cap; i++) out_path[i] = res.path[i];
}

void wl_engine_click(wl_engine *e, int r, int c) { (void)e; (void)r; (void)c; }

void wl_engine_set_use_figures(wl_engine *e, int v) { e->use_figures = v; }
