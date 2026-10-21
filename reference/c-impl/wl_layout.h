/*
 * wl_layout.h -- client-area geometry.
 *
 * The model combines two independent sources:
 *
 *   * the original's layout code (constants 0x60, 0x6C, 0x148, 0x3A, 0xEA,
 *     0x7D), re-disassembled at 0x402EA7;
 *   * the original's own screenshots (docs/11), which show the board
 *     horizontally centred and vertically centred in the area below the
 *     HUD bar, with the two figures flanking it.
 *
 * The two agree on the figure placement -- the right figure uses the
 * original's own formula, confirmed to within ~1.5% by the screenshots --
 * and disagree on the board's origin, so the board follows the screenshots
 * (direct behavioural evidence).
 *
 * The layout never overflows: wl_layout_clamp() keeps every region inside
 * the client area, which is the one place where the re-implementation
 * deliberately does NOT inherit the original's behaviour (docs/04, D3).
 */
#ifndef WL_LAYOUT_H
#define WL_LAYOUT_H

#define WL_CELL_PX        36
#define WL_SPRITE_INSET   2
#define WL_BOARD_FRAME_W  327   /* 0x147 */
#define WL_BOARD_FRAME_H  328   /* 0x148 */
#define WL_PANEL_W        125   /* 0x7D */
#define WL_HUD_H          64
#define WL_MENU_BAR_H     32
#define WL_FIGURE_W       58
#define WL_CLIENT_MARGIN  96    /* board area width = clientW - 96  */
#define WL_PANEL_BASE_BIAS 328  /* panel layout base                */
#define WL_PANEL_HALF_BIAS 58   /* panel half-width base           */

typedef struct {
    int client_w, client_h;
    int board_x, board_y;          /* the board frame's origin        */
    int left_figure_x;             /* the CPU ("国王")                */
    int right_figure_x;            /* the player ("挑战者")           */
    int figure_y;
    int panel_x, panel_y;          /* the right figure's slot         */
    int panel_w, panel_h;
    int board_area_w;
    int cell_px;
} wl_layout;

void wl_layout_compute(wl_layout *L, int client_w, int client_h);

/*
 * Smallest client size for which every region fits.
 */
void wl_layout_min_client(int *w, int *h);

/* Clamp a computed layout so nothing leaves the client area. */
void wl_layout_clamp(wl_layout *L);

/* Pixel -> cell. Returns 1 when the point is on the board, else 0. */
int  wl_layout_hit_cell(const wl_layout *L, int px, int py, int *r, int *c);

#endif /* WL_LAYOUT_H */
