/* wl_render.c -- SDL2 presentation layer. */
#include "wl_render.h"
#include <string.h>

static const SDL_Color PANEL_BG   = { 192, 192, 192, 255 };
static const SDL_Color FRAME_HI  = { 255, 255, 255, 255 };
static const SDL_Color FRAME_LO  = { 128, 128, 128, 255 };
static const SDL_Color FRAME_LO2 = { 160, 160, 160, 255 };
static const SDL_Color TEXT_COL  = {  32,  32,  32, 255 };

int wl_render_init(wl_render *R, SDL_Renderer *ren, const char *sheet_bmp)
{
    SDL_Surface *s;
    R->ren = ren;
    R->sheet = NULL;
    R->sprite_set = 0;

    s = SDL_LoadBMP(sheet_bmp);
    if (!s) {
        SDL_Log("wl_render: cannot load %s: %s", sheet_bmp, SDL_GetError());
        return -1;
    }
    R->sheet = SDL_CreateTextureFromSurface(ren, s);
    SDL_FreeSurface(s);
    if (!R->sheet) {
        SDL_Log("wl_render: texture creation failed: %s", SDL_GetError());
        return -1;
    }
    SDL_SetTextureBlendMode(R->sheet, SDL_BLENDMODE_NONE);
    return 0;
}

void wl_render_shutdown(wl_render *R)
{
    if (R->sheet) { SDL_DestroyTexture(R->sheet); R->sheet = NULL; }
}

static void fill_rect(SDL_Renderer *ren, int x, int y, int w, int h, SDL_Color c)
{
    SDL_Rect r = { x, y, w, h };
    SDL_SetRenderDrawColor(ren, c.r, c.g, c.b, c.a);
    SDL_RenderFillRect(ren, &r);
}

void wl_render_board_frame(wl_render *R, const wl_layout *L)
{
    int x = L->board_x, y = L->board_y;
    int w = WL_BOARD_FRAME_W, h = WL_BOARD_FRAME_H;

    fill_rect(R->ren, x, y, w, h, FRAME_LO);
    fill_rect(R->ren, x + 1, y + 1, w - 2, h - 2, FRAME_HI);
    fill_rect(R->ren, x + 2, y + 2, w - 4, h - 4, FRAME_LO2);
    fill_rect(R->ren, x + 3, y + 3, w - 6, h - 6, PANEL_BG);
}

static int is_highlighted(const int *hl, int n, int cell)
{
    int i;
    for (i = 0; i < n; i++) if (hl[i] == cell) return 1;
    return 0;
}

void wl_render_board(wl_render *R, const wl_layout *L, const wl_board *b,
                     const int *highlight, int n_highlight)
{
    int r, c, v;
    for (r = 0; r < WL_SIDE; r++) {
        for (c = 0; c < WL_SIDE; c++) {
            int cell = wl_idx(r, c);
            int srcx, srcy, dx, dy;
            SDL_Rect dst, src;
            v = b->cell[cell] & 0x3F;
            dx = L->board_x + WL_SPRITE_INSET + c * L->cell_px;
            dy = L->board_y + WL_SPRITE_INSET + r * L->cell_px;
            if (v == 0) {
                /* empty cell tile */
                srcx = R->sprite_set * WL_SET_STRIDE;
                srcy = WL_SRC_Y;
                dst.x = dx; dst.y = dy; dst.w = L->cell_px; dst.h = L->cell_px;
                src.x = srcx; src.y = srcy; src.w = WL_SRC_STEP; src.h = WL_SRC_STEP;
                SDL_RenderCopy(R->ren, R->sheet, &src, &dst);
                continue;
            }
            srcx = v * WL_SRC_STEP + R->sprite_set * WL_SET_STRIDE;
            srcy = WL_SRC_Y;
            dst.x = dx; dst.y = dy; dst.w = L->cell_px; dst.h = L->cell_px;
            src.x = srcx; src.y = srcy; src.w = WL_SRC_STEP; src.h = WL_SRC_STEP;
            SDL_RenderCopy(R->ren, R->sheet, &src, &dst);

            if (highlight && is_highlighted(highlight, n_highlight, cell)) {
                SDL_Rect hl = { dx + 1, dy + 1, L->cell_px - 2, L->cell_px - 2 };
                SDL_SetRenderDrawColor(R->ren, 255, 255, 0, 255);
                SDL_RenderDrawRect(R->ren, &hl);
            }
        }
    }
}

void wl_render_panel(wl_render *R, const wl_layout *L, int score,
                     int next_colour, int show_next, int use_figures)
{
    int x = L->panel_x, y = L->panel_y;
    char buf[64];

    fill_rect(R->ren, x, y, L->panel_w, L->panel_h, PANEL_BG);
    fill_rect(R->ren, x, y, L->panel_w, 1, FRAME_LO);
    fill_rect(R->ren, x, y, 1, L->panel_h, FRAME_LO);

    /* score readout */
    SDL_snprintf(buf, sizeof buf, "%d", score);
    {
        SDL_Rect t = { x + 8, y + 12, L->panel_w - 16, 24 };
        SDL_SetRenderDrawColor(R->ren, TEXT_COL.r, TEXT_COL.g, TEXT_COL.b, 255);
        SDL_RenderDrawRect(R->ren, &t);
        (void)buf;
    }

    /* next-set preview */
    if (show_next && next_colour > 0) {
        SDL_Rect src = { next_colour * WL_SRC_STEP, WL_SRC_Y, WL_SRC_STEP, WL_SRC_STEP };
        SDL_Rect dst = { x + (L->panel_w - 32) / 2, y + 56, 32, 32 };
        SDL_RenderCopy(R->ren, R->sheet, &src, &dst);
    }

    /* sprite-set indicator */
    {
        SDL_Rect t = { x + 8, y + 104, L->panel_w - 16, 20 };
        SDL_SetRenderDrawColor(R->ren, TEXT_COL.r, TEXT_COL.g, TEXT_COL.b, 255);
        SDL_RenderDrawRect(R->ren, &t);
        (void)use_figures;
    }
}
