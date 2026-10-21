/*
 * wl_layout.h -- client-area geometry.
 *
 * The formulas are transcribed from the original's WM_SIZE handler, which
 * appears twice (0x402EA7 and 0x4036E8) with identical arithmetic:
 *
 *   0x40B060 = (clientW - 0x148) / 2                       board x
 *   0x40B064 = (clientH - 0x178) / 2 + 0x2E                board y
 *   0x40B070 = clientW - 0x60                              board area width
 *   0x40B078 = (clientW - 0x6C) / 2                        preview group x
 *   0x40B088 = (((clientW - 0x148) / 2) - 0x3A) / 2        left figure x
 *   0x40B08C = (clientH - 0xEA) / 2 + 0xEA                 figure base y
 *   0x40B098 = t + 0x148 + (((t - 0x3A) / 2)), t = (clientW - 0x148) / 2
 *                                                          right figure x
 *
 * The portrait starts 0x86 pixels above the figure base, hence
 * figure_y = (clientH - 0xEA) / 2 + 0xEA - 0x86.
 *
 * A live client area of 604x371 gives the board origin (138, 44), which
 * matches the black-box screenshots exactly.
 *
 * The layout never overflows: wl_layout_clamp() keeps every region inside
 * the client area, which is the one place where the re-implementation
 * deliberately does NOT inherit the original's behaviour (docs/04, D3).
 */
#ifndef WL_LAYOUT_H
#define WL_LAYOUT_H

#define WL_CELL_PX        36
#define WL_SPRITE_INSET   2
#define WL_BOARD_FRAME_W  327   /* 0x147, the drawn frame */
#define WL_BOARD_FRAME_H  328   /* 0x148 */
#define WL_BOARD_SPAN_W   328   /* 0x148, the centring span */
#define WL_PANEL_W        125   /* 0x7D  */
#define WL_HUD_H          64
#define WL_MENU_BAR_H     32
#define WL_FIGURE_W       58    /* 0x3A  */
#define WL_FIGURE_H       134   /* 0x86  */
#define WL_CLIENT_MARGIN  96    /* 0x60; board area width = clientW - 96 */
#define WL_BOARD_V_BIAS   376   /* 0x178 */
#define WL_BOARD_V_OFFSET 46    /* 0x2E  */
#define WL_FIGURE_V_BIAS  234   /* 0xEA  */
#define WL_FIGURE_V_OFFSET 100  /* 0xEA - 0x86 */

typedef struct {
    int client_w, client_h;
    int board_x, board_y;          /* the board frame's origin        */
    int left_figure_x;             /* the CPU ("国王")                */
    int right_figure_x;            /* the player ("挑战者")           */
    int figure_y;                  /* top of the figure slot          */
    int panel_x, panel_y;          /* the right figure's slot         */
    int panel_w, panel_h;
    int board_area_w;
    int cell_px;
} wl_layout;

/* The original's formulas with no clamping (used by the acceptance gate). */
void wl_layout_compute_raw(wl_layout *L, int client_w, int client_h);

/* The original's formulas, clamped to the client area. */
void wl_layout_compute(wl_layout *L, int client_w, int client_h);

/*
 * Smallest client size for which every region fits without clamping.
 */
void wl_layout_min_client(int *w, int *h);

/* Clamp a computed layout so nothing leaves the client area. */
void wl_layout_clamp(wl_layout *L);

/* Pixel -> cell. Returns 1 when the point is on the board, else 0. */
int  wl_layout_hit_cell(const wl_layout *L, int px, int py, int *r, int *c);

#endif /* WL_LAYOUT_H */
