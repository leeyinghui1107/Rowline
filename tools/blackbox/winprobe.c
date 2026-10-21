/*
 * winprobe.c -- Windows-side runtime collector for the WinLinez black-box phase.
 *
 * Build (on the analysis VM, with the MSVC or MinGW toolchain):
 *     cl /nologo /W4 winprobe.c /Fe:winprobe.exe
 *     gcc -O2 -Wall winprobe.c -o winprobe.exe
 *
 * Usage:
 *     winprobe.exe --out <dir> [--class <className>] [--title <substring>]
 *                             [--pid <pid>] [--shot <file.bmp>]
 *
 * It dumps, as JSON, everything that can be read from a *running* instance:
 *   windows.json  the window/control tree (class, rect, styles, text, id)
 *   menu.json     the runtime menu tree (ids, texts, check states, hints)
 *   config.json   the INI file verbatim + the registry key values verbatim
 *   metrics.json  screen metrics and the target window's client rect
 *
 * Nothing is written to the target process; the collector only reads.
 */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <tchar.h>

#define MAXCH  4096
#define MAXWIN 512

static char g_out[MAXCH];
static char g_class[MAXCH];
static char g_title[MAXCH];
static DWORD g_pid = 0;
static int  g_found = 0;
static HWND g_target = NULL;

static FILE *g_fw = NULL, *g_fm = NULL, *g_fc = NULL, *g_ft = NULL;

/* ------------------------------------------------------------------ json */
static void jstr(FILE *f, const char *s)
{
    fputc('"', f);
    for (; *s; s++) {
        unsigned char c = (unsigned char)*s;
        if (c == '"' || c == '\\') { fputc('\\', f); fputc(c, f); }
        else if (c == '\n') fputs("\\n", f);
        else if (c == '\r') fputs("\\r", f);
        else if (c == '\t') fputs("\\t", f);
        else if (c < 0x20) fprintf(f, "\\u%04x", c);
        else fputc(c, f);
    }
    fputc('"', f);
}

static void jhex(FILE *f, const unsigned char *p, size_t n)
{
    size_t i;
    fputc('"', f);
    for (i = 0; i < n; i++) fprintf(f, "%02x", p[i]);
    fputc('"', f);
}

/* ------------------------------------------------------------------ enum */
static BOOL CALLBACK enum_proc(HWND h, LPARAM lp)
{
    DWORD pid = 0;
    char cls[MAXCH], txt[MAXCH];
    DWORD style, ex;
    RECT rc;
    HWND parent;

    GetWindowThreadProcessId(h, &pid);
    if (g_pid && pid != g_pid) return TRUE;

    GetClassNameA(h, cls, sizeof cls);
    GetWindowTextA(h, txt, sizeof txt);

    if (!g_found) {
        int cls_match = g_class[0] && strstr(cls, g_class) != NULL;
        int ttl_match = g_title[0] && strstr(txt, g_title) != NULL;
        if (cls_match || ttl_match) {
            g_target = h;
            g_found = 1;
        }
    }

    if (!g_fw) return TRUE;

    style = (DWORD)GetWindowLongA(h, GWL_STYLE);
    ex    = (DWORD)GetWindowLongA(h, GWL_EXSTYLE);
    GetWindowRect(h, &rc);
    parent = GetParent(h);

    fprintf(g_fw, "    {\"hwnd\":\"0x%llx\",\"parent\":\"0x%llx\",\"class\":",
            (unsigned long long)(ULONG_PTR)h, (unsigned long long)(ULONG_PTR)parent);
    jstr(g_fw, cls);
    fprintf(g_fw, ",\"text\":");
    jstr(g_fw, txt);
    fprintf(g_fw, ",\"style\":\"0x%08lx\",\"exstyle\":\"0x%08lx\"", style, ex);
    fprintf(g_fw, ",\"visible\":%s", IsWindowVisible(h) ? "true" : "false");
    fprintf(g_fw, ",\"enabled\":%s", IsWindowEnabled(h) ? "true" : "false");
    fprintf(g_fw, ",\"id\":%ld", (long)GetWindowLongA(h, GWL_ID));
    fprintf(g_fw, ",\"rect\":[%ld,%ld,%ld,%ld]",
            rc.left, rc.top, rc.right, rc.bottom);
    {
        RECT cr;
        if (GetClientRect(h, &cr))
            fprintf(g_fw, ",\"client\":[%ld,%ld,%ld,%ld]",
                    cr.left, cr.top, cr.right, cr.bottom);
    }
    fprintf(g_fw, "}\n");
    return TRUE;
}

/* ------------------------------------------------------------------ menu */
static void dump_menu(FILE *f, HMENU m, int depth)
{
    int n = GetMenuItemCount(m), i;
    for (i = 0; i < n; i++) {
        char buf[MAXCH];
        MENUITEMINFOA mi;
        HMENU sub;

        memset(&mi, 0, sizeof mi);
        mi.cbSize = sizeof mi;
        mi.fMask = MIIM_ID | MIIM_SUBMENU | MIIM_STATE | MIIM_STRING | MIIM_TYPE;
        mi.dwTypeData = buf;
        mi.cch = sizeof buf - 1;
        if (!GetMenuItemInfoA(m, i, TRUE, &mi)) continue;

        sub = (mi.fMask & MIIM_SUBMENU) ? mi.hSubMenu : NULL;

        fputc(' ', f);
        {
            int k; for (k = 0; k < depth; k++) fputs("  ", f);
        }
        fprintf(f, "{\"id\":%u,\"text\":", (unsigned)mi.wID);
        jstr(f, buf);
        fprintf(f, ",\"checked\":%s", (mi.fState & MF_CHECKED) ? "true" : "false");
        fprintf(f, ",\"grayed\":%s", (mi.fState & MF_GRAYED) ? "true" : "false");
        fprintf(f, ",\"disabled\":%s", (mi.fState & MF_DISABLED) ? "true" : "false");
        fprintf(f, ",\"hilite\":%s", (mi.fState & MF_HILITE) ? "true" : "false");
        fprintf(f, ",\"submenu\":%s}\n", sub ? "true" : "false");
        if (sub) dump_menu(f, sub, depth + 1);
    }
}

/* ------------------------------------------------------------------ accel */
static void dump_accel(FILE *f, HACCEL acc)
{
    ACCEL *a;
    int n, i;
    if (!acc) return;
    n = CopyAcceleratorTableA(acc, NULL, 0);
    if (n <= 0) return;
    a = (ACCEL *)malloc(sizeof(ACCEL) * n);
    if (!a) return;
    n = CopyAcceleratorTableA(acc, a, n);
    for (i = 0; i < n; i++) {
        fprintf(f, "    {\"cmd\":%u,\"key\":%u,\"ctrl\":%s,\"shift\":%s,\"alt\":%s}\n",
                (unsigned)a[i].cmd, (unsigned)a[i].key,
                (a[i].fVirt & FVIRTKEY) ? "false" : "true",
                (a[i].fVirt & FSHIFT) ? "true" : "false",
                (a[i].fVirt & FALT) ? "true" : "false");
    }
    free(a);
}

/* ------------------------------------------------------------------ files */
static void dump_file(FILE *f, const char *path)
{
    FILE *in = fopen(path, "rb");
    long sz;
    unsigned char *buf;
    if (!in) { fprintf(f, "    {\"path\":"); jstr(f, path); fputs(",\"present\":false}\n", f); return; }
    fseek(in, 0, SEEK_END); sz = ftell(in); fseek(in, 0, SEEK_SET);
    buf = (unsigned char *)malloc((size_t)sz + 1);
    if (!buf) { fclose(in); return; }
    if (fread(buf, 1, (size_t)sz, in) != (size_t)sz) { /* short read ok */ }
    buf[sz] = 0;
    fclose(in);
    fprintf(f, "    {\"path\":");
    jstr(f, path);
    fprintf(f, ",\"present\":true,\"size\":%ld,\"data\":", sz);
    jhex(f, buf, (size_t)sz);
    fputs("}\n", f);
    free(buf);
}

static void dump_reg(FILE *f, HKEY h, const char *name)
{
    DWORD type = 0, sz = 0;
    unsigned char *buf;
    HKEY sub;
    LONG r = RegQueryValueExA(h, name, NULL, &type, NULL, &sz);
    if (r != ERROR_SUCCESS) {
        fprintf(f, "    {\"key\":");
        jstr(f, name);
        fprintf(f, ",\"value\":");
        jstr(f, name);
        fputs(",\"present\":false}\n", f);
        return;
    }
    buf = (unsigned char *)malloc(sz ? sz : 1);
    if (!buf) return;
    r = RegQueryValueExA(h, name, NULL, &type, buf, &sz);
    if (r == ERROR_SUCCESS) {
        fprintf(f, "    {\"key\":");
        jstr(f, name);
        fprintf(f, ",\"value\":");
        jstr(f, name);
        fprintf(f, ",\"type\":%lu,\"size\":%lu,\"data\":", type, sz);
        jhex(f, buf, sz);
        fputs("}\n", f);
    }
    free(buf);
    (void)sub;
}

int main(int argc, char **argv)
{
    int i;
    char path[MAXCH];
    FILE *f;

    for (i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--out") && i+1 < argc) strncpy(g_out, argv[++i], MAXCH-1);
        else if (!strcmp(argv[i], "--class") && i+1 < argc) strncpy(g_class, argv[++i], MAXCH-1);
        else if (!strcmp(argv[i], "--title") && i+1 < argc) strncpy(g_title, argv[++i], MAXCH-1);
        else if (!strcmp(argv[i], "--pid") && i+1 < argc) g_pid = (DWORD)strtoul(argv[++i], NULL, 10);
    }
    if (!g_out[0]) strcpy(g_out, ".");
    if (!g_class[0] && !g_title[0]) strcpy(g_class, "WinLinez");

    /* ------------------------------------------------------------ windows */
    sprintf(path, "%s\\windows.json", g_out);
    f = fopen(path, "w");
    if (f) { g_fw = f; fputs("{\n  \"windows\": [\n", f); }

    EnumWindows(enum_proc, 0);

    if (g_fw) {
        fputs("  ],\n", g_fw);
        fprintf(g_fw, "  \"target_found\": %s,\n", g_found ? "true" : "false");
        if (g_target) {
            RECT cr, wr;
            GetClientRect(g_target, &cr);
            GetWindowRect(g_target, &wr);
            fprintf(g_fw, "  \"target\": {\"hwnd\":\"0x%llx\",\"client\":[%ld,%ld,%ld,%ld],"
                          "\"window\":[%ld,%ld,%ld,%ld]},\n",
                    (unsigned long long)(ULONG_PTR)g_target,
                    cr.left, cr.top, cr.right, cr.bottom,
                    wr.left, wr.top, wr.right, wr.bottom);
        }
        fputs("  \"screen\": {\"width\":", g_fw);
        fprintf(g_fw, "%d,\"height\":%d}\n}\n", GetSystemMetrics(SM_CXSCREEN),
                GetSystemMetrics(SM_CYSCREEN));
        fclose(g_fw);
        g_fw = NULL;
    }

    /* --------------------------------------------------------------- menu */
    sprintf(path, "%s\\menu.json", g_out);
    f = fopen(path, "w");
    if (f) {
        g_fm = f;
        fputs("{\n  \"menu\": [\n", f);
        if (g_target) {
            HMENU hm = GetMenu(g_target);
            if (hm) dump_menu(f, hm, 0);
        }
        fputs("  ],\n  \"accel\": [\n", f);
        if (g_target) {
            HACCEL ha = LoadAcceleratorsA(GetModuleHandleA(NULL),
                                          MAKEINTRESOURCEA(1100));
            if (ha) dump_accel(f, ha);
        }
        fputs("  ]\n}\n", f);
        fclose(f);
    }

    /* ------------------------------------------------------------- config */
    sprintf(path, "%s\\config.json", g_out);
    f = fopen(path, "w");
    if (f) {
        g_fc = f;
        fputs("{\n  \"ini\": [\n", f);
        {
            char dir[MAXCH], full[MAXCH];
            GetModuleFileNameA(NULL, dir, MAXCH);
            {
                char *slash = strrchr(dir, '\\');
                if (slash) *slash = 0; else strcpy(dir, ".");
            }
            sprintf(full, "%s\\WinLinez.INI", dir);
            dump_file(f, full);
        }
        fputs("  ],\n  \"registry\": [\n", f);
        {
            HKEY hk;
            if (RegOpenKeyExA(HKEY_CURRENT_USER,
                    "SOFTWARE\\Ivan Golubev\\WinLinez\\HighScore", 0,
                    KEY_READ, &hk) == ERROR_SUCCESS) {
                dump_reg(f, hk, "Table");
                dump_reg(f, hk, "Hash");
                RegCloseKey(hk);
            }
        }
        fputs("  ]\n}\n", f);
        fclose(f);
    }

    /* ------------------------------------------------------------ metrics */
    sprintf(path, "%s\\metrics.json", g_out);
    f = fopen(path, "w");
    if (f) {
        fprintf(f, "{\n  \"screen_w\": %d,\n  \"screen_h\": %d,\n",
                GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        fprintf(f, "  \"target_found\": %s,\n", g_found ? "true" : "false");
        if (g_target) {
            RECT cr;
            GetClientRect(g_target, &cr);
            fprintf(f, "  \"client_w\": %ld,\n  \"client_h\": %ld\n",
                    cr.right - cr.left, cr.bottom - cr.top);
        } else {
            fputs("  \"client_w\": 0,\n  \"client_h\": 0\n", f);
        }
        fputs("}\n", f);
        fclose(f);
    }

    printf("winprobe: target %s\n", g_found ? "found" : "NOT FOUND");
    printf("winprobe: output written to %s\n", g_out);
    return g_found ? 0 : 1;
}
