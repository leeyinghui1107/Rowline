/* wl_layout.c -- client-area geometry.  See wl_layout.h for the sources. */
#include "wl_layout.h"

#include <stdlib.h>

static int idiv2(int v)
{
    return v < 0 ? -((-v + 1) / 2) : v / 2;
}

void wl_layout_compute(wl_layout *L, int client_w, int client_h)
{
    int top, avail, t;

    L->client_w = client_w;
    L->client_h = client_h;
    L->cell_px  = WL_CELL_PX;

    /* The board frame is centred horizontally ... */
    L->board_x = idiv2(client_w - WL_BOARD_FRAME_W);
    /* ... and centred vertically in the area below the HUD bar. */
    top  = WL_MENU_BAR_H + WL_HUD_H;
    avail = client_h - top;
    if (avail < 0) avail = 0;
    L->board_y = top + idiv2(avail - WL_BOARD_FRAME_H);

    /* The right figure uses the original's own formula. */
    t = idiv2(client_w - WL_BOARD_FRAME_W);
    L->right_figure_x = t + WL_BOARD_FRAME_W + idiv2(t - WL_PANEL_HALF_BIAS);
    L->left_figure_x  = client_w - L->right_figure_x - WL_FIGURE_W;
    L->figure_y = L->board_y;

    L->panel_x = L->right_figure_x;
    L->panel_y = L->board_y;
    L->panel_w = WL_PANEL_W;
    L->panel_h = WL_BOARD_FRAME_H;

    L->board_area_w = client_w - WL_CLIENT_MARGIN;

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
}

static int layout_fits(int w, int h)
{
    wl_layout L;
    wl_layout_compute(&L, w, h);
    return L.board_x >= 0 && L.board_y >= 0 &&
           L.board_x + WL_BOARD_FRAME_W <= w &&
           L.board_y + WL_BOARD_FRAME_H <= h &&
           L.left_figure_x >= 0 &&
           L.right_figure_x + WL_FIGURE_W <= w;
}

void wl_layout_min_client(int *w, int *h)
{
    int cw = 320, ch = 240;
    while (cw < 8192 && !layout_fits(cw, 8192)) cw += 4;
    while (ch < 8192 && !layout_fits(8192, ch)) ch += 4;
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
