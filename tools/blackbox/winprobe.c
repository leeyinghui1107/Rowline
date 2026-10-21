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
static DWORD g_target_pid = 0;
static int  g_found = 0;
static int  g_win_first = 1;
static HWND g_target = NULL;

static FILE *g_fw = NULL, *g_fm = NULL, *g_fc = NULL;

/* ------------------------------------------------------------------ json */
static void jstr(FILE *f, const char *s)
{
    /* The target is an ANSI program: its window/menu strings are in the
     * system code page (CP936 here).  JSON is UTF-8 by definition, so the
     * text must be converted, otherwise CP936 trail bytes (0x5C, '"', ...)
     * corrupt the document. */
    static WCHAR wbuf[MAXCH * 2];
    static char  ubuf[MAXCH * 6];
    int n, m, i;

    n = MultiByteToWideChar(CP_ACP, 0, s, -1, wbuf, MAXCH * 2);
    if (n <= 0) { fputs("\"\"", f); return; }
    /* n includes the terminating NUL: do not turn it into \u0000. */
    m = WideCharToMultiByte(CP_UTF8, 0, wbuf, n - 1, ubuf, (int)sizeof ubuf - 1,
                            NULL, NULL);
    if (m <= 0) { fputs("\"\"", f); return; }
    ubuf[m] = 0;

    fputc('"', f);
    for (i = 0; i < m; i++) {
        unsigned char c = (unsigned char)ubuf[i];
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
    (void)lp;
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
            g_target_pid = pid;
            g_found = 1;
        }
    }

    if (!g_fw) return TRUE;

    if (!g_win_first) fputs(",\n", g_fw);
    g_win_first = 0;
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
    fputc('}', g_fw);
    return TRUE;
}

/* ------------------------------------------------------------------ menu */
static int g_menu_first = 1;

static void dump_menu(FILE *f, HMENU m, int depth)
{
    int n = GetMenuItemCount(m), i;
    for (i = 0; i < n; i++) {
        char buf[MAXCH];
        MENUITEMINFOA mi;
        HMENU sub;

        memset(&mi, 0, sizeof mi);
        mi.cbSize = sizeof mi;
        /* MIIM_STRING (not MIIM_TYPE): MIIM_TYPE is the pre-Win95 flag and
         * must not be combined with MIIM_STRING, otherwise the call fails
         * and the runtime menu comes back empty. */
        mi.fMask = MIIM_ID | MIIM_SUBMENU | MIIM_STATE | MIIM_STRING;
        mi.dwTypeData = buf;
        mi.cch = sizeof buf - 1;
        if (!GetMenuItemInfoA(m, i, TRUE, &mi)) continue;

        sub = (mi.fMask & MIIM_SUBMENU) ? mi.hSubMenu : NULL;

        if (!g_menu_first) fputs(",\n", f);
        g_menu_first = 0;
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

/* ------------------------------------------------------------------ files */
static void dump_file(FILE *f, const char *path, int *first)
{
    FILE *in;
    long sz;
    unsigned char *buf;
    if (!*first) fputs(",\n", f);
    *first = 0;
    in = fopen(path, "rb");
    if (!in) { fprintf(f, "    {\"path\":"); jstr(f, path); fputs(",\"present\":false}", f); return; }
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
    fputc('}', f);
    free(buf);
}

/* Dump every value of one registry key as a flat JSON record.  The process
 * is a 32-bit ANSI program on a 64-bit OS, so the effective writes end up in
 * the WOW6432Node / VirtualStore views; callers pass each candidate path. */
static void jsep(FILE *f, int *first)
{
    if (!*first) fputs(",\n", f);
    *first = 0;
}

static void dump_reg_key(FILE *f, HKEY root, const char *rootname,
                         const char *path, int *first)
{
    HKEY hk;
    LONG r;
    DWORD nvals = 0, i;
    int emitted = 0;

    r = RegOpenKeyExA(root, path, 0, KEY_READ, &hk);
    jsep(f, first);
    if (r != ERROR_SUCCESS) {
        fputs("    {\"root\":", f); jstr(f, rootname);
        fputs(",\"path\":", f); jstr(f, path);
        fputs(",\"present\":false}", f);
        return;
    }
    RegQueryInfoKeyA(hk, NULL, NULL, NULL, NULL, NULL, NULL, &nvals,
                     NULL, NULL, NULL, NULL);
    if (nvals == 0) {
        fputs("    {\"root\":", f); jstr(f, rootname);
        fputs(",\"path\":", f); jstr(f, path);
        fputs(",\"present\":true,\"value_count\":0}", f);
    }
    for (i = 0; i < nvals; i++) {
        char name[MAXCH];
        DWORD namelen = MAXCH, type = 0, sz = 0;
        unsigned char *buf;

        r = RegEnumValueA(hk, i, name, &namelen, NULL, &type, NULL, &sz);
        if (r != ERROR_SUCCESS) continue;
        name[namelen < MAXCH ? namelen : MAXCH - 1] = 0;
        buf = (unsigned char *)malloc(sz ? sz : 1);
        if (!buf) continue;
        namelen = MAXCH;
        r = RegEnumValueA(hk, i, name, &namelen, NULL, &type, buf, &sz);
        if (r != ERROR_SUCCESS) { free(buf); continue; }
        if (emitted) fputs(",\n", f);
        fputs("    {\"root\":", f); jstr(f, rootname);
        fputs(",\"path\":", f); jstr(f, path);
        fputs(",\"value\":", f); jstr(f, name);
        fprintf(f, ",\"present\":true,\"type\":%lu,\"size\":%lu,\"data\":",
                type, sz);
        jhex(f, buf, sz);
        if ((type == REG_SZ || type == REG_EXPAND_SZ) && sz) {
            fputs(",\"text\":", f);
            jstr(f, (const char *)buf);
        }
        fputc('}', f);
        free(buf);
        emitted = 1;
    }
    RegCloseKey(hk);
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
            fprintf(g_fw, "  \"target\": {\"hwnd\":\"0x%llx\",\"pid\":%lu,"
                          "\"client\":[%ld,%ld,%ld,%ld],"
                          "\"window\":[%ld,%ld,%ld,%ld]},\n",
                    (unsigned long long)(ULONG_PTR)g_target,
                    (unsigned long)g_target_pid,
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
        g_menu_first = 1;
        fputs("{\n  \"menu\": [\n", f);
        if (g_target) {
            HMENU hm = GetMenu(g_target);
            if (hm) dump_menu(f, hm, 0);
        }
        fputs("  ],\n  \"accel\": [],\n"
              "  \"accel_note\": \"an accelerator table lives in the target process; verify the shortcuts by sending keys instead\"\n}\n", f);
        fclose(f);
    }

    /* ------------------------------------------------------------- config */
    sprintf(path, "%s\\config.json", g_out);
    f = fopen(path, "w");
    if (f) {
        char windir[MAXCH], localapp[MAXCH], exedir[MAXCH], cwd[MAXCH];
        char full[MAXCH * 2], *slash;

        g_fc = f;
        GetWindowsDirectoryA(windir, MAXCH);
        GetEnvironmentVariableA("LOCALAPPDATA", localapp, MAXCH);
        GetModuleFileNameA(NULL, exedir, MAXCH);
        slash = strrchr(exedir, '\\');
        if (slash) *slash = 0; else strcpy(exedir, ".");
        GetCurrentDirectoryA(MAXCH, cwd);

        int first_ini = 1, first_reg = 1;
        fputs("{\n  \"ini\": [\n", f);
        /* Newest Windows versions redirect a 32-bit non-manifested program's
         * write to %WINDIR% into the per-user VirtualStore.  Dump both the
         * logical path and the effective one, most-specific first. */
        if (localapp[0]) {
            snprintf(full, sizeof full,
                     "%s\\VirtualStore\\Windows\\WinLinez.INI", localapp);
            dump_file(f, full, &first_ini);
        }
        snprintf(full, sizeof full, "%s\\WinLinez.INI", windir);
        dump_file(f, full, &first_ini);
        snprintf(full, sizeof full, "%s\\WinLinez.INI", exedir);
        dump_file(f, full, &first_ini);
        snprintf(full, sizeof full, "%s\\WinLinez.INI", cwd);
        if (strcmp(cwd, exedir) != 0) dump_file(f, full, &first_ini);

        fputs("  ],\n  \"registry\": [\n", f);
        dump_reg_key(f, HKEY_CURRENT_USER,
            "HKCU",
            "Software\\Classes\\VirtualStore\\MACHINE\\SOFTWARE\\WOW6432Node\\Ivan Golubev\\WinLinez\\HighScore",
            &first_reg);
        dump_reg_key(f, HKEY_LOCAL_MACHINE,
            "HKLM",
            "SOFTWARE\\WOW6432Node\\Ivan Golubev\\WinLinez\\HighScore",
            &first_reg);
        dump_reg_key(f, HKEY_LOCAL_MACHINE,
            "HKLM",
            "SOFTWARE\\Ivan Golubev\\WinLinez\\HighScore",
            &first_reg);
        dump_reg_key(f, HKEY_CURRENT_USER,
            "HKCU",
            "Software\\Ivan Golubev\\WinLinez\\HighScore",
            &first_reg);
        fputs("  ],\n  \"accel_note\": \"accelerator tables belong to the target process and cannot be read cross-process\",\n", f);
        fputs("  \"exe\": ", f); jstr(f, exedir);
        fputs("\n}\n", f);
        fclose(f);
    }

    /* ------------------------------------------------------------ metrics */
    sprintf(path, "%s\\metrics.json", g_out);
    f = fopen(path, "w");
    if (f) {
        RECT cr, wr, wa;
        int awareness = -1;
        unsigned window_dpi = 0, system_dpi = 0;
        HMODULE shcore, user32;
        typedef HRESULT (WINAPI *FN_GetProcessDpiAwareness)(HANDLE, int *);
        typedef UINT (WINAPI *FN_GetDpiForWindow)(HWND);
        typedef UINT (WINAPI *FN_GetDpiForSystem)(void);

        shcore = LoadLibraryA("shcore.dll");
        if (shcore && g_target_pid) {
            FN_GetProcessDpiAwareness fn = (FN_GetProcessDpiAwareness)
                (void *)GetProcAddress(shcore, "GetProcessDpiAwareness");
            if (fn) {
                HANDLE hp = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,
                                        FALSE, g_target_pid);
                if (hp) {
                    fn(hp, &awareness);
                    CloseHandle(hp);
                }
            }
        }
        user32 = GetModuleHandleA("user32.dll");
        if (user32 && g_target) {
            FN_GetDpiForWindow fn = (FN_GetDpiForWindow)
                (void *)GetProcAddress(user32, "GetDpiForWindow");
            if (fn) window_dpi = fn(g_target);
        }
        if (user32) {
            FN_GetDpiForSystem fn = (FN_GetDpiForSystem)
                (void *)GetProcAddress(user32, "GetDpiForSystem");
            if (fn) system_dpi = fn();
        }
        if (!system_dpi) system_dpi = 96;
        {
            DWORD applied = 0, asz = sizeof applied;
            if (RegGetValueA(HKEY_CURRENT_USER,
                             "Control Panel\\Desktop\\WindowMetrics",
                             "AppliedDPI", RRF_RT_REG_DWORD, NULL,
                             &applied, &asz) == ERROR_SUCCESS && applied)
                system_dpi = applied;
        }

        fprintf(f, "{\n  \"screen_w\": %d,\n  \"screen_h\": %d,\n",
                GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        if (SystemParametersInfoA(SPI_GETWORKAREA, 0, &wa, 0))
            fprintf(f, "  \"work_w\": %ld,\n  \"work_h\": %ld,\n",
                    wa.right - wa.left, wa.bottom - wa.top);
        fprintf(f, "  \"target_found\": %s,\n", g_found ? "true" : "false");
        fprintf(f, "  \"pid\": %lu,\n", (unsigned long)g_target_pid);
        fprintf(f, "  \"dpi_awareness\": %d,\n", awareness);
        fprintf(f, "  \"window_dpi\": %u,\n  \"system_dpi\": %u,\n",
                window_dpi, system_dpi);
        fprintf(f, "  \"scale\": %.2f,\n", system_dpi / 96.0);
        fputs("  \"dpi_note\": \"probe is DPI-unaware; window/client metrics are virtualized to 96 dpi, scale comes from AppliedDPI\",\n", f);
        if (g_target) {
            GetClientRect(g_target, &cr);
            GetWindowRect(g_target, &wr);
            fprintf(f, "  \"client_w\": %ld,\n  \"client_h\": %ld,\n",
                    cr.right - cr.left, cr.bottom - cr.top);
            fprintf(f, "  \"window_w\": %ld,\n  \"window_h\": %ld\n",
                    wr.right - wr.left, wr.bottom - wr.top);
        } else {
            fputs("  \"client_w\": 0,\n  \"client_h\": 0,\n"
                  "  \"window_w\": 0,\n  \"window_h\": 0\n", f);
        }
        fputs("}\n", f);
        fclose(f);
    }

    printf("winprobe: target %s\n", g_found ? "found" : "NOT FOUND");
    printf("winprobe: output written to %s\n", g_out);
    return g_found ? 0 : 1;
}
