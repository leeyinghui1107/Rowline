/*
 * wl_main.c -- SDL2 application shell.
 *
 * Scope: this is the M1 skeleton.  It proves the layout formulas, the sprite
 * index mapping and the input -> cell mapping.  Rules and scoring are wired
 * to the shared model in wl_model.c so that the differential replay gate can
 * exercise exactly the same code.
 */
#if defined(__has_include)
#  if __has_include(<SDL2/SDL.h>)
#    include <SDL2/SDL.h>
#  else
#    include <SDL.h>
#  endif
#else
#  include <SDL.h>
#endif
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "wl_model.h"
#include "wl_layout.h"
#include "wl_render.h"

#define DEFAULT_W 900
#define DEFAULT_H 640

typedef struct {
    wl_engine    eng;
    wl_layout    layout;
    wl_render    rnd;
    int          selected;      /* cell index or -1 */
    int          show_next;
    int          show_statistics;
    int          use_figures;
    int          next_colour;
} app;

static void compute_layout(app *a, int w, int h)
{
    wl_layout_compute(&a->layout, w, h);
}

static void draw(app *a)
{
    int hl[4], nhl = 0;
    wl_render_board_frame(&a->rnd, &a->layout);
    if (a->selected >= 0) hl[nhl++] = a->selected;
    wl_render_board(&a->rnd, &a->layout, &a->eng.board, hl, nhl);
    wl_render_panel(&a->rnd, &a->layout, a->eng.score, a->next_colour,
                    a->show_next, a->use_figures);
}

static void on_cell_click(app *a, int r, int c)
{
    int cell = wl_idx(r, c);
    if (a->selected < 0) {
        if (a->eng.board.cell[cell] != 0) a->selected = cell;
        return;
    }
    if (a->selected == cell) { a->selected = -1; return; }
    {
        wl_move_result res;
        int sr, sc;
        wl_rc(a->selected, &sr, &sc);
        wl_engine_move_ex(&a->eng, sr, sc, r, c, &res);
        if (res.status == 2) a->selected = -1;
    }
}

static int handle_event(app *a, const SDL_Event *ev)
{
    switch (ev->type) {
    case SDL_QUIT:
        return 1;
    case SDL_WINDOWEVENT:
        if (ev->window.event == SDL_WINDOWEVENT_SIZE_CHANGED) {
            compute_layout(a, ev->window.data1, ev->window.data2);
        }
        break;
    case SDL_MOUSEBUTTONDOWN:
        if (ev->button.button == SDL_BUTTON_LEFT) {
            int r, c;
            if (wl_layout_hit_cell(&a->layout, ev->button.x, ev->button.y, &r, &c))
                on_cell_click(a, r, c);
        }
        break;
    case SDL_KEYDOWN:
        switch (ev->key.keysym.sym) {
        case SDLK_ESCAPE: return 1;
        case SDLK_F1:     SDL_Log("cmd 768  Contents (help)  -- not wired yet"); break;
        case SDLK_F3:     a->show_next = !a->show_next; break;   /* cmd 512 */
        case SDLK_F4:     wl_engine_new_game(&a->eng); a->selected = -1; break; /* cmd 256 */
        case SDLK_F5:     a->show_statistics = !a->show_statistics; break;    /* cmd 513 */
        case SDLK_F6:     a->use_figures = !a->use_figures;                  /* cmd 514 */
                          a->rnd.sprite_set = a->use_figures; break;
        default: break;
        }
        break;
    default:
        break;
    }
    return 0;
}

int main(int argc, char **argv)
{
    app a;
    const char *sheet = "assets/sheet/wlinez_sheet.bmp";
    SDL_Window *win;
    int running = 1;

    if (argc > 1) sheet = argv[1];

    if (SDL_Init(SDL_INIT_VIDEO) != 0) {
        SDL_Log("SDL_Init failed: %s", SDL_GetError());
        return 1;
    }
    win = SDL_CreateWindow("WinLinez (re-implementation)",
                           SDL_WINDOWPOS_CENTERED, SDL_WINDOWPOS_CENTERED,
                           DEFAULT_W, DEFAULT_H, SDL_WINDOW_RESIZABLE);
    if (!win) { SDL_Log("window failed: %s", SDL_GetError()); return 1; }

    memset(&a, 0, sizeof a);
    a.selected = -1;
    a.show_next = 1;
    a.show_statistics = 1;
    wl_rule_model_default(&a.eng.model);
    wl_engine_init(&a.eng, (uint32_t)SDL_GetTicks(), &a.eng.model);
    wl_engine_new_game(&a.eng);
    a.next_colour = 1 + (a.eng.rng.state >> 16) % WL_COLORS;
    compute_layout(&a, DEFAULT_W, DEFAULT_H);

    a.rnd.ren = SDL_CreateRenderer(win, -1, SDL_RENDERER_ACCELERATED |
                                            SDL_RENDERER_PRESENTVSYNC);
    if (!a.rnd.ren) a.rnd.ren = SDL_CreateRenderer(win, -1, SDL_RENDERER_SOFTWARE);
    if (wl_render_init(&a.rnd, a.rnd.ren, sheet) != 0) {
        SDL_Log("failed to initialise the sprite sheet (%s)", sheet);
        SDL_Log("run `make sheet` to generate the placeholder art");
    }

    while (running) {
        SDL_Event ev;
        while (SDL_PollEvent(&ev)) if (handle_event(&a, &ev)) running = 0;
        SDL_SetRenderDrawColor(a.rnd.ren, 192, 192, 192, 255);
        SDL_RenderClear(a.rnd.ren);
        if (a.rnd.sheet) draw(&a);
        SDL_RenderPresent(a.rnd.ren);
    }

    wl_render_shutdown(&a.rnd);
    SDL_DestroyRenderer(a.rnd.ren);
    SDL_DestroyWindow(win);
    SDL_Quit();
    return 0;
}
