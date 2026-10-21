/*
 * wl_layout_check.c -- the M1 layout acceptance gate.
 *
 * Verifies, for a set of client sizes, that every region fits, that the
 * board is centred, that the figures flank it, and that the pixel <-> cell
 * mapping round-trips for all 81 cells.
 */
#include <stdarg.h>
#include <stdio.h>

#include "wl_layout.h"
#include "wl_model.h"

static int failures;

static void check(int cond, const char *fmt, ...)
{
    va_list ap;
    if (!cond) failures++;
    printf("  %s ", cond ? "OK  " : "FAIL");
    va_start(ap, fmt);
    vprintf(fmt, ap);
    va_end(ap);
    printf("\n");
}

static void run(int w, int h)
{
    wl_layout L;
    int r, c;

    wl_layout_compute(&L, w, h);

    printf("\nclient %dx%d\n", w, h);
    printf("  board origin  = (%d, %d)   frame = %dx%d\n",
           L.board_x, L.board_y, WL_BOARD_FRAME_W, WL_BOARD_FRAME_H);
    printf("  left figure   = %d\n", L.left_figure_x);
    printf("  right figure  = %d\n", L.right_figure_x);

    check(L.board_x >= 0 && L.board_y >= 0,
          "board origin inside the client rect");
    check(L.board_x + WL_BOARD_FRAME_W <= w,
          "board frame fits horizontally (%d + %d <= %d)",
          L.board_x, WL_BOARD_FRAME_W, w);
    check(L.board_y + WL_BOARD_FRAME_H <= h,
          "board frame fits vertically (%d + %d <= %d)",
          L.board_y, WL_BOARD_FRAME_H, h);
    check(L.left_figure_x >= 0 && L.left_figure_x + WL_FIGURE_W <= w,
          "the left figure fits");
    check(L.right_figure_x >= 0 && L.right_figure_x + WL_FIGURE_W <= w,
          "the right figure fits");
    check(L.left_figure_x + WL_FIGURE_W <= L.board_x,
          "the left figure does not overlap the board");
    check(L.right_figure_x >= L.board_x + WL_BOARD_FRAME_W,
          "the right figure does not overlap the board");
    check(L.cell_px == WL_CELL_PX, "cell size is %d px", WL_CELL_PX);
    check(L.board_x == (w - 328) / 2 && L.board_y == (h - 376) / 2 + 46,
          "board origin matches the original's formula");
    check(L.figure_y == (h - 234) / 2 + 100,
          "figure y matches the original's formula (%d)", L.figure_y);

    /* the board is centred horizontally */
    {
        int mid = L.board_x + WL_BOARD_FRAME_W / 2;
        int delta = mid - w / 2;
        if (delta < 0) delta = -delta;
        check(delta <= 2, "the board is centred horizontally (delta %d)", delta);
    }

    /* every cell round-trips */
    {
        int ok = 1;
        for (r = 0; r < 9; r++) {
            for (c = 0; c < 9; c++) {
                int px = L.board_x + WL_SPRITE_INSET + c * L.cell_px;
                int py = L.board_y + WL_SPRITE_INSET + r * L.cell_px;
                int hr, hc;
                if (px < 0 || py < 0 || px + L.cell_px > w || py + L.cell_px > h) {
                    ok = 0;
                    printf("       cell (%d,%d) out of bounds\n", r, c);
                    continue;
                }
                if (!wl_layout_hit_cell(&L, px + L.cell_px / 2, py + L.cell_px / 2,
                                        &hr, &hc) || hr != r || hc != c) {
                    ok = 0;
                    printf("       cell (%d,%d) does not round-trip\n", r, c);
                }
            }
        }
        check(ok, "all 81 cells map to distinct in-bounds rects and round-trip");
    }

    {
        int hr, hc;
        check(!wl_layout_hit_cell(&L, L.board_x - 1, L.board_y - 1, &hr, &hc),
              "a point outside the board is rejected");
    }
}

int main(void)
{
    static const int sizes[][2] = {
        { 640, 480 }, { 800, 600 }, { 1024, 768 }, { 1280, 800 },
        { 1440, 900 }, { 1920, 1080 }
    };
    int minw, minh, i;
    wl_layout live;

    wl_layout_min_client(&minw, &minh);
    printf("=== M1 layout acceptance (C reference) ===\n");
    printf("minimum client size that fits every region: %dx%d\n", minw, minh);
    check(minw == 444 && minh == 372,
          "minimum is the smallest size that fits (got %dx%d)", minw, minh);

    /* Direct black-box evidence: the live original at 604x371. */
    wl_layout_compute_raw(&live, 604, 371);
    check(live.board_x == 138 && live.board_y == 44,
          "live 604x371 board origin is (138, 44) (got %d, %d)",
          live.board_x, live.board_y);
    check(live.left_figure_x == 40 && live.right_figure_x == 506,
          "live 604x371 figures are at 40 / 506");
    check(live.figure_y == 168, "live 604x371 figure y is 168");

    for (i = 0; i < (int)(sizeof sizes / sizeof sizes[0]); i++) {
        printf("\n--- %dx%d %s ---\n", sizes[i][0], sizes[i][1],
               (sizes[i][0] >= minw && sizes[i][1] >= minh)
                   ? "(at/above minimum)" : "(BELOW minimum: clamped)");
        run(sizes[i][0], sizes[i][1]);
    }

    printf("\n%s (%d failure(s))\n", failures ? "FAILED" : "PASSED", failures);
    return failures ? 1 : 0;
}
