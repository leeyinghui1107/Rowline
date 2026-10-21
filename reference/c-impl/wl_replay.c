/*
 * wl_replay.c -- headless replay CLI.
 *
 * Reads a trace (format documented in tools/replay/model.py) and prints the
 * result events.  This is the C side of the differential replay gate:
 *
 *     build/wlinez-replay tools/replay/cases/foo.trace
 *
 * The output must match tools/replay/model.py byte for byte.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "wl_model.h"

#define MAX_OPS 4096

typedef struct { char kind[16]; int a, b, c, d; } wl_op;

static void print_board(const wl_board *b)
{
    int i;
    for (i = 0; i < WL_CELLS; i++)
        putchar(b->cell[i] == 0 ? '.' : (char)('0' + b->cell[i]));
}

static void print_rc(int i)
{
    int r, c;
    wl_rc(i, &r, &c);
    printf(" %d,%d", r, c);
}

int main(int argc, char **argv)
{
    FILE *f;
    char line[512];
    uint32_t seed = 0;
    wl_board initial;
    int have_board = 0;
    wl_op ops[MAX_OPS];
    int nops = 0;
    wl_engine eng;
    wl_rule_model model;
    int i;

    if (argc < 2) {
        fprintf(stderr, "usage: %s <trace-file>\n", argv[0]);
        return 2;
    }
    f = fopen(argv[1], "r");
    if (!f) { perror(argv[1]); return 2; }

    memset(&initial, 0, sizeof initial);
    while (fgets(line, sizeof line, f)) {
        char *nl = strchr(line, '\n');
        if (nl) *nl = 0;
        if (line[0] == '#' || line[0] == 0) continue;

        if (!strncmp(line, "seed", 4)) {
            seed = (uint32_t)strtoul(line + 4, NULL, 0);
        } else if (!strncmp(line, "board", 5)) {
            const char *p = line + 5;
            while (*p == ' ' || *p == '\t') p++;
            if (strlen(p) >= WL_CELLS) {
                for (i = 0; i < WL_CELLS; i++) {
                    char ch = p[i];
                    int v;
                    if (ch == '.' || ch == '_') v = 0;
                    else if (ch >= '0' && ch <= '9') v = ch - '0';
                    else v = (ch | 0x20) - 'a' + 10;
                    initial.cell[i] = (uint8_t)v;
                }
                have_board = 1;
            }
        } else if (!strncmp(line, "op ", 3)) {
            char kind[16] = {0};
            int a = 0, b = 0, c = 0, d = 0;
            int n = sscanf(line + 3, "%15s %d %d %d %d", kind, &a, &b, &c, &d);
            if (n >= 1 && nops < MAX_OPS) {
                wl_op *o = &ops[nops++];
                memset(o, 0, sizeof *o);
                strncpy(o->kind, kind, sizeof o->kind - 1);
                o->a = a; o->b = b; o->c = c; o->d = d;
            }
        }
    }
    fclose(f);

    wl_rule_model_default(&model);
    wl_engine_init(&eng, seed, &model);
    if (have_board) {
        eng.board = initial;
    } else {
        wl_engine_new_game(&eng);
        printf("state %d ", eng.step);
        print_board(&eng.board);
        printf(" %d\n", eng.score);
    }

    for (i = 0; i < nops; i++) {
        wl_op *o = &ops[i];
        eng.step++;
        if (!strcmp(o->kind, "move")) {
            wl_move_result res;
            int j;
            wl_engine_move_ex(&eng, o->a, o->b, o->c, o->d, &res);
            if (res.status == 0) {
                printf("note %d illegal move\n", eng.step);
            } else if (res.status == 1) {
                printf("note %d no path\n", eng.step);
            } else {
                printf("path %d", eng.step);
                for (j = 0; j < res.path_len; j++) print_rc(res.path[j]);
                printf("\n");
                if (res.group_len > 0) {
                    printf("elim %d %d", eng.step, res.group_len);
                    for (j = 0; j < res.group_len; j++) print_rc(res.group[j]);
                    printf(" %d\n", res.delta);
                }
                printf("state %d ", eng.step);
                print_board(&eng.board);
                printf(" %d\n", eng.score);
            }
        } else if (!strcmp(o->kind, "click")) {
            wl_engine_click(&eng, o->a, o->b);
            printf("note %d click %d,%d\n", eng.step, o->a, o->b);
        } else if (!strcmp(o->kind, "newgame")) {
            wl_engine_new_game(&eng);
            printf("state %d ", eng.step);
            print_board(&eng.board);
            printf(" %d\n", eng.score);
        } else if (!strcmp(o->kind, "setusefigures")) {
            wl_engine_set_use_figures(&eng, o->a);
            printf("note %d usefigures %d\n", eng.step, o->a);
        }
    }
    return 0;
}
