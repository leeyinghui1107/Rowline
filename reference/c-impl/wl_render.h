/*
 * wl_render.h -- SDL2 presentation layer.
 *
 * Sprite sampling follows the index layout recovered from the original:
 *     srcX = cellValue * 36 + spriteSet * 288
 *     srcY = 225
 * (see docs/03_FEATURE_SPEC.md section 5.2)
 */
#ifndef WL_RENDER_H
#define WL_RENDER_H

#if defined(__has_include)
#  if __has_include(<SDL2/SDL.h>)
#    include <SDL2/SDL.h>
#  else
#    include <SDL.h>
#  endif
#else
#  include <SDL.h>
#endif
#include "wl_model.h"
#include "wl_layout.h"

#define WL_SHEET_W 576
#define WL_SHEET_H 586
#define WL_SRC_Y   225
#define WL_SRC_STEP 36
#define WL_SET_STRIDE 288

typedef struct {
    SDL_Renderer *ren;
    SDL_Texture  *sheet;
    int           sprite_set;   /* 0 = spheres, 1 = polygons */
} wl_render;

int  wl_render_init(wl_render *R, SDL_Renderer *ren, const char *sheet_bmp);
void wl_render_shutdown(wl_render *R);

/* Draw the 9x9 board. `highlight` marks up to 4 cells for the selection/path. */
void wl_render_board(wl_render *R, const wl_layout *L, const wl_board *b,
                     const int *highlight, int n_highlight);

/* Draw the sunken 3D frame around the board area. */
void wl_render_board_frame(wl_render *R, const wl_layout *L);

/* Draw the side panel: score, next-set preview, sprite-set indicator. */
void wl_render_panel(wl_render *R, const wl_layout *L, int score,
                     int next_colour, int show_next, int use_figures);

#endif /* WL_RENDER_H */
