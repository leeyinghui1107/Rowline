/* wl_layout.c -- client-area geometry.  See wl_layout.h for the sources. */
#include "wl_layout.h"

#include <stdlib.h>

/*
 * C integer division truncates toward zero, which is exactly what the
 * original's `sar` sequence does for the constants involved, so the formulas
 * below are transcribed literally.
 */
void wl_layout_compute_raw(wl_layout *L, int client_w, int client_h)
{
    int t;

    L->client_w = client_w;
    L->client_h = client_h;
    L->cell_px  = WL_CELL_PX;

    /* 0x40B060: board x (the original centres a 0x148-wide span). */
    L->board_x = (client_w - WL_BOARD_SPAN_W) / 2;
    /* 0x40B064: board y. */
    L->board_y = (client_h - WL_BOARD_V_BIAS) / 2 + WL_BOARD_V_OFFSET;

    /* 0x40B088 / 0x40B098: the figures flank the board. */
    t = L->board_x;
    L->left_figure_x  = (t - WL_FIGURE_W) / 2;
    L->right_figure_x = t + WL_BOARD_SPAN_W + L->left_figure_x;

    /* 0x40B08C: figure base y; the portrait starts 0x86 higher. */
    L->figure_y = (client_h - WL_FIGURE_V_BIAS) / 2 + WL_FIGURE_V_OFFSET;

    L->panel_x = L->right_figure_x;
    L->panel_y = L->figure_y;
    L->panel_w = WL_PANEL_W;
    L->panel_h = WL_FIGURE_H;

    /* 0x40B070: board area width. */
    L->board_area_w = client_w - WL_CLIENT_MARGIN;
}

void wl_layout_compute(wl_layout *L, int client_w, int client_h)
{
    wl_layout_compute_raw(L, client_w, client_h);
    wl_layout_clamp(L);
}

void wl_layout_clamp(wl_layout *L)
{
    if (L->board_x < 0) L->board_x = 0;
    if (L->board_y < 0) L->board_y = 0;
    if (L->board_x + WL_BOARD_FRAME_W > L->client_w)
        L->board_x = L->client_w - WL_BOARD_FRAME_W > 0 ? L->client_w - WL_BOARD_FRAME_W : 0;
    if (L->board_y + WL_BOARD_FRAME_H > L->client_h)
        L->board_y = L->client_h - WL_BOARD_FRAME_H > 0 ? L->client_h - WL_BOARD_FRAME_H : 0;
    if (L->left_figure_x < 0) L->left_figure_x = 0;
    if (L->right_figure_x + WL_FIGURE_W > L->client_w)
        L->right_figure_x = L->client_w - WL_FIGURE_W > 0 ? L->client_w - WL_FIGURE_W : 0;
    if (L->figure_y < 0) L->figure_y = 0;
}

/*
 * Whether one axis fits, using the *unclamped* original formulas.  The other
 * axis is given a generous default so only the tested axis can fail.
 */
static int layout_fits_axis(int v, int horizontal)
{
    int w = horizontal ? v : 8192;
    int h = horizontal ? 8192 : v;
    int board_x = (w - WL_BOARD_SPAN_W) / 2;
    int board_y = (h - WL_BOARD_V_BIAS) / 2 + WL_BOARD_V_OFFSET;
    int left    = (board_x - WL_FIGURE_W) / 2;
    int right   = board_x + WL_BOARD_SPAN_W + left;
    int fig_y   = (h - WL_FIGURE_V_BIAS) / 2 + WL_FIGURE_V_OFFSET;

    return board_x >= 0 && board_y >= 0 &&
           board_x + WL_BOARD_FRAME_W <= w &&
           board_y + WL_BOARD_FRAME_H <= h &&
           left >= 0 && left + WL_FIGURE_W <= board_x &&
           right >= board_x + WL_BOARD_FRAME_W &&
           right + WL_FIGURE_W <= w &&
           fig_y >= 0;
}

void wl_layout_min_client(int *w, int *h)
{
    int cw = 0, ch = 0;
    while (cw < 8192 && !layout_fits_axis(cw, 1)) cw++;
    while (ch < 8192 && !layout_fits_axis(ch, 0)) ch++;
    if (w) *w = cw;
    if (h) *h = ch;
}

int wl_layout_hit_cell(const wl_layout *L, int px, int py, int *r, int *c)
{
    int dx = px - (L->board_x + WL_SPRITE_INSET);
    int dy = py - (L->board_y + WL_SPRITE_INSET);
    if (dx < 0 || dy < 0) return 0;
    *r = dy / L->cell_px;
    *c = dx / L->cell_px;
    if (*r >= 9 || *c >= 9) return 0;
    return 1;
}
