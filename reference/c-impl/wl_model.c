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
    m->spawn_per_move      = WL_GROUP_SIZE;
    m->initial_balls       = 5;
    m->source_keeps_colour = 0;
}

/*
 * The four axes recovered from the binary's direction table at 0x40A000:
 * vertical, main diagonal, horizontal, anti-diagonal.  Each axis is scanned
 * in both directions from the placed ball.
 */
static const int AXIS_R[4] = { -1, -1,  0,  1 };
static const int AXIS_C[4] = {  0, -1, -1, -1 };

/* 4-neighbour step order used by the original path search. */
static const int STEP_R[4] = { -1,  1,  0,  0 };
static const int STEP_C[4] = {  0,  0, -1,  1 };

int wl_find_lines(const wl_board *b, int start, int colour, int threshold,
                  int *out_cells, int *out_count)
{
    uint8_t marked[WL_CELLS];
    int r0, c0, axis, i, total = 0, count = 0;

    if (b->cell[start] != colour) {
        if (out_count) *out_count = 0;
        return 0;
    }
    memset(marked, 0, sizeof marked);
    wl_rc(start, &r0, &c0);

    for (axis = 0; axis < 4; axis++) {
        int run[WL_CELLS];
        int run_len = 0, sign;
        run[run_len++] = start;
        for (sign = 1; sign >= -1; sign -= 2) {
            int r = r0 + sign * AXIS_R[axis];
            int c = c0 + sign * AXIS_C[axis];
            while (wl_in_bounds(r, c) && b->cell[wl_idx(r, c)] == colour) {
                run[run_len++] = wl_idx(r, c);
                r += sign * AXIS_R[axis];
                c += sign * AXIS_C[axis];
            }
        }
        if (run_len >= threshold) {
            total += run_len;
            for (i = 0; i < run_len; i++) marked[run[i]] = 1;
        }
    }
    if (total == 0) {
        if (out_count) *out_count = 0;
        return 0;
    }
    for (i = 0; i < WL_CELLS; i++)
        if (marked[i]) out_cells[count++] = i;
    if (out_count) *out_count = count;
    return total;
}

int wl_score_for_run(int total)
{
    int score = 10, step = 2, extra;
    if (total < WL_THRESHOLD) return 0;
    extra = total - WL_THRESHOLD;
    while (extra-- > 0) {
        score += step;
        step += 4;
    }
    return score;
}

int wl_find_path(const wl_board *b, int src, int dst,
                 int *out_path, int out_cap)
{
    static uint8_t seen[WL_CELLS];
    static int parent[WL_CELLS];
    int queue[WL_CELLS], qh = 0, qt = 0;
    int tmp[WL_CELLS], len = 0, cur, i;

    if (src == dst) return 0;
    if (b->cell[dst] != 0) return 0;

    memset(seen, 0, sizeof seen);
    seen[src] = 1;
    parent[src] = -1;
    queue[qt++] = src;

    while (qh < qt) {
        int r, c, k;
        cur = queue[qh++];
        if (cur == dst) break;
        wl_rc(cur, &r, &c);
        for (k = 0; k < 4; k++) {
            int nr = r + STEP_R[k], nc = c + STEP_C[k], n;
            if (!wl_in_bounds(nr, nc)) continue;
            n = wl_idx(nr, nc);
            if (seen[n]) continue;
            if (n != dst && b->cell[n] != 0) continue;
            seen[n] = 1;
            parent[n] = cur;
            queue[qt++] = n;
        }
    }
    if (!seen[dst]) return 0;

    cur = dst;
    while (cur != -1 && len < WL_CELLS) {
        tmp[len++] = cur;
        if (cur == src) break;
        cur = parent[cur];
    }
    if (cur != src) return 0;
    for (i = 0; i < len && i < out_cap; i++)
        out_path[i] = tmp[len - 1 - i];
    return len;
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
 * Place one preview ball at a uniformly random empty cell (uniform over the
 * empty cells, not over the 81 squares with retries).  If the placement
 * completes a line the original clears it without scoring and immediately
 * places another ball; the return value tells the caller to do the same.
 */
static int wl_spawn_one(wl_engine *e)
{
    int empty_count = 0, pick, nth, i, colour, total, count = 0;
    int cells[WL_CELLS];

    for (i = 0; i < WL_CELLS; i++)
        if (e->board.cell[i] == 0) empty_count++;
    if (empty_count == 0) return 0;

    pick = (int)wl_rng_below(&e->rng, empty_count);
    nth = 0;
    for (i = 0; i < WL_CELLS; i++) {
        if (e->board.cell[i] != 0) continue;
        if (nth++ == pick) break;
    }
    colour = (int)wl_take_next_ball(e);
    e->board.cell[i] = (uint8_t)colour;

    total = wl_find_lines(&e->board, i, colour, e->model.threshold,
                          cells, &count);
    if (total == 0) return 0;
    for (i = 0; i < count; i++)
        e->board.cell[cells[i]] = 0;
    return 1;
}

static void wl_spawn_until_quiet(wl_engine *e)
{
    int guard = 0;
    while (wl_spawn_one(e)) {
        if (++guard > WL_CELLS) break;
    }
}

/*
 * A fresh board holds the first five balls of the same random stream as the
 * preview: roll three colours, then place five, consuming the queue head and
 * rolling a replacement each time.  The remaining queue is the next group.
 */
void wl_engine_new_game(wl_engine *e)
{
    int placed;
    memset(&e->board, 0, sizeof e->board);
    e->score = 0;
    wl_roll_next_group(e);
    for (placed = 0; placed < e->model.initial_balls; placed++)
        wl_spawn_until_quiet(e);
}

void wl_engine_move_ex(wl_engine *e, int r0, int c0, int r1, int c1,
                       wl_move_result *res)
{
    int src = wl_idx(r0, c0), dst = wl_idx(r1, c1);
    int colour = wl_board_get(&e->board, r0, c0);
    int group[WL_CELLS], total, count = 0, n, i, delta;

    memset(res, 0, sizeof *res);
    res->status = 0;
    if (colour == 0 || wl_board_get(&e->board, r1, c1) != 0) return;  /* illegal */

    n = wl_find_path(&e->board, src, dst, res->path, WL_CELLS * 2);
    if (n == 0) { res->status = 1; return; }                          /* no path */
    res->path_len = n;
    res->status = 2;

    wl_board_set(&e->board, r1, c1, colour);
    if (!e->model.source_keeps_colour)
        wl_board_set(&e->board, r0, c0, 0);

    total = wl_find_lines(&e->board, dst, colour, e->model.threshold,
                          group, &count);
    if (total > 0) {
        n = count;
        for (i = 0; i < n; i++) {
            wl_board_set(&e->board, group[i] / WL_SIDE, group[i] % WL_SIDE, 0);
            res->group[i] = group[i];
        }
        res->group_len = n;
        delta = wl_score_for_run(total);
        e->score += delta;
        res->delta = delta;
        /* A clearing move does not consume the preview group. */
        return;
    }

    for (i = 0; i < e->model.spawn_per_move; i++)
        wl_spawn_until_quiet(e);
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
