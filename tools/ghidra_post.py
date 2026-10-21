#!/usr/bin/env python3
"""
Ghidra headless post-processing script for WINLINEZ.EXE.

Usage (headless):
  analyzeHeadless <projdir> WINLINEZ -import winlinez.exe -postScript ghidra_post.py \
      -scriptPath <this dir> -deleteProject

What it does
  1. Renames the ~40 anchor functions / globals that were identified statically.
  2. Applies struct definitions for the score-table record and the board array.
  3. Exports decompiled C for every anchor function into out/pseudo/*.c
  4. Writes out/anchors.json (address -> semantic name) for later scripting.

This script performs *renaming and export only*. It is a research aid for
producing an independent behavioural specification; no original code is copied
into the deliverable.
"""
import json, os, re

# ---------------------------------------------------------------- anchors
# name, address, kind, comment
FUNCS = [
    ("EntryPoint_BorlandStartup", 0x401000, "entry stub -> WinMain -> ExitProcess"),
    ("WinMain",                   0x401133, "creates main window, reads INI, starts timer"),
    ("rand_next",                 0x401100, "LCG: state = state*0x41C64E6D + 0x3039"),
    ("rand_seed",                 0x40112D, "state = arg (seeded from GetTickCount)"),
    ("DrawBoardGrid",             0x401533, "paints the 9x9 board via BitBlt from atlas"),
    ("DrawPanelSprite",           0x401698, "paints one sprite cell (secondary panel)"),
    ("Board_InitNewGame",         0x401743, "fills the 9x9 board array"),
    ("MenuCommandRouter",         0x4019FF, "routes WM_COMMAND ids"),
    ("GameRules_ValidateMove",    0x401A4A, "bounds + 4-neighbour scan, threshold 5"),
    ("Score_UpdateStatus",        0x401BD4, "updates score/status"),
    ("Board_Helper",              0x401CA1, "small board accessor"),
    ("Game_EvaluateLine",         0x401CD4, "line evaluation + path highlight + sleep"),
    ("DrawPanel_Background",      0x401EC6, "panel background + connect threshold"),
    ("MenuAction_Dispatch",       0x402134, "menu action dispatch"),
    ("DrawPanel_BoardFrame",      0x40238B, "board frame geometry"),
    ("DrawPanel_SidePanel",       0x4024E7, "right side panel (125 px)"),
    ("DrawPanel_HUD",             0x402680, "HUD: score, cell count, flags"),
    ("Game_ResetRound",           0x402801, "starts a new round"),
    ("Game_LevelLogic",           0x4028A1, "level / round progression"),
    ("ScoreTable_Wipe",           0x402A05, "clears the 16-entry score table"),
    ("ScoreTable_Load",           0x402A50, "reads Table+Hash from registry"),
    ("ScoreTable_LoadBody",       0x402A80, "registry read + hash verify"),
    ("ScoreTable_Save",           0x402BFD, "writes Table+Hash to registry"),
    ("WndProc_Main",              0x402DA2, "main window procedure"),
    ("Hash_Compute",              0x4029C3, "computes 4 x u32 digest over 512 bytes"),
    ("Color_ToGray",              0x407079, "maps a COLORREF to a grey level"),
    ("Color_GetLevel",            0x407023, "extracts intensity byte"),
    ("Blit_RecolorCells",         0x4070E0, "per-pixel recolour / colour-key blit"),
    ("Paint_CustomControl",       0x407099, "themed bevel/button painter"),
    # --- window-message dispatch tree (compares `msg`) ---
    ("WndProc_DispatchMessages",  0x403B65, "message dispatch tree: WM_CREATE/SIZE/PAINT/CLOSE/COMMAND/TIMER/MOUSE"),
    # --- WM_COMMAND dispatch tree (compares LOWORD(wParam) = menu command id) ---
    ("WndProc_DispatchCommand",   0x40366D, "menu command id dispatch"),
    ("Cmd_NewGame",               0x403408, "cmd 256  新建 New (F4)"),
    ("Cmd_ShowHighScores",        0x40346E, "cmd 257  显示高分榜 Show High Scores"),
    ("Cmd_Exit",                  0x403496, "cmd 258  退出 Exit (Alt+F9)"),
    ("Cmd_ToggleNextSet",         0x4034E0, "cmd 512  下一组 Next Set (F3) -> INI ShowNext"),
    ("Cmd_ToggleUseFigures",      0x40359E, "cmd 514  使用形状 Use Figures (F6) -> INI UseFigures"),
    ("Cmd_Contents",              0x403606, "cmd 768  内容 Contents (F1) -> winlinez.hlp"),
    ("Cmd_About",                 0x40364D, "cmd 769  关于 About -> dialog 1003"),
    # --- generic window-procedure wrapper used by the main window and the X* controls ---
    ("WndProc_GenericWrapper",    0x404F1B, "NC-message handling, GWL_USERDATA, DefWindowProcA"),
]

GLOBALS = [
    ("g_rngState",            0x40B010, "LCG state"),
    ("g_hInstance",           0x40B048, "HINSTANCE"),
    ("g_hMainWnd",            0x40B2C0, "main window handle"),
    ("g_hStatusWnd",          0x40B2B8, "status bar handle"),
    ("g_atlasDC",             0x40B2B4, "DC with the sprite atlas selected"),
    ("g_atlasOldBitmap",      0x40B2B0, "previous bitmap in atlas DC"),
    ("g_boardOriginX",        0x40B060, "board pixel origin X"),
    ("g_boardOriginY",        0x40B064, "board pixel origin Y"),
    ("g_panelBase",           0x40B069, "panel sprite index base"),
    ("g_panelSpriteIdx",      0x40B06A, "panel sprite index"),
    ("g_currentScore",        0x40B06C, "score of the running round"),
    ("g_boardAreaWidth",      0x40B070, "board area width (clientW-96)"),
    ("g_boardCellCount",      0x40B074, "board cells per side (10)"),
    ("g_boardOriginXCalc",    0x40B078, "board origin X (computed)"),
    ("g_connectThreshold",    0x40B07C, "cells needed to clear (5)"),
    ("g_showNextSet",         0x40B080, "ShowNext flag (cmd 512)"),
    ("g_menuRoute",           0x40B084, "menu routing global"),
    ("g_panelHalfWidth",      0x40B088, "panel half width"),
    ("g_boardOriginYCalc",    0x40B08C, "board origin Y (computed)"),
    ("g_stateFlags0",         0x40B090, "state flags 0"),
    ("g_stateFlags1",         0x40B094, "state flags 1"),
    ("g_panelOriginX",        0x40B098, "side panel origin X"),
    ("g_panelOriginY",        0x40B09C, "side panel origin Y"),
    ("g_panelWidth",          0x40B0A0, "side panel width (125)"),
    ("g_stateFlags2",         0x40B0A4, "state flags 2"),
    ("g_scoreTable",          0x40B0AC, "16 records x 32 bytes"),
    ("g_scoreTable0Score",    0x40B0C8, "score of record 0"),
    ("g_bestScore",           0x40B2A8, "best score (record 0 .score)"),
    ("g_board",               0x40B2F0, "9x9 board, 81 bytes"),
    ("g_boardAux",            0x40B2EF, "auxiliary per-cell byte array"),
    ("g_pathFlags0",          0x40B2E8, "path/line state byte 0"),
    ("g_pathFlags1",          0x40B2E9, "path/line state byte 1"),
    ("g_pathFlags2",          0x40B2EA, "path/line state byte 2"),
    ("g_useFigures",          0x40B3EC, "UseFigures toggle: 0 = spheres, 1 = polygons"),
    ("g_wipeAccepted",        0x40B3F0, "score-wipe accepted flag"),
]

# Menu command ids recovered from RT_MENU (Simplified Chinese) + RT_ACCELERATOR.
MENU_IDS = {
    256:  "Cmd_NewGame        新建(&N)  F4",
    257:  "Cmd_ShowHighScores  显示高分榜(&H)",
    258:  "Cmd_Exit            退出(&X)  Alt+F9",
    512:  "Cmd_ToggleNextSet   下一组(&N)  F3      -> INI ShowNext",
    513:  "Cmd_ShowStatistics  显示统计(&S)  F5      -> INI ShowStatistics",
    514:  "Cmd_ToggleUseFigures 使用形状  F6           -> INI UseFigures",
    768:  "Cmd_Contents        内容(&C)  F1",
    769:  "Cmd_About           关于(&A)...",
    16528:"Cmd_Help            帮助(&H)",
}

STRINGS = {
    0x40A020: "STR_WindowClass",
    0x40A098: "STR_winlines_res_dead",
    0x40A0A8: "STR_lines_res_dead",
    0x40A0C0: "STR_FontName",
    0x40A0D0: "STR_AppTitleVersion",
    0x40A138: "STR_MainWindowTitle",
    0x40A150: "STR_IniFileName",
    0x40A160: "STR_IniKey_WindowX",
    0x40A168: "STR_IniKey_WindowY",
    0x40A170: "STR_IniKey_WindowWidth",
    0x40A180: "STR_IniKey_WindowHeight",
    0x40A190: "STR_IniKey_WindowMaximized",
    0x40A1A0: "STR_IniKey_ShowNext",
    0x40A1B0: "STR_IniKey_ShowStatistics",
    0x40A1C0: "STR_IniKey_UseFigures",
    0x40A1D0: "STR_StatisticsWindowTitle",
    0x40A200: "STR_RegKey_HighScore",
    0x40A230: "STR_RegValue_Table",
    0x40A238: "STR_RegValue_Hash",
    0x40A3A0: "STR_Class_XGrdCptDialog",
    0x40A3B0: "STR_Class_XButton",
    0x40A3B8: "STR_Class_XBevel",
    0x40A3C0: "STR_Class_XStatusBar",
    0x40A3D0: "STR_Class_XHyperLink",
    0x40A3E8: "STR_Subject",
    0x40A3F8: "STR_Class_XStaticText",
    0x40A408: "STR_Class_XGauge",
    0x40A420: "STR_DisplayDevice",
    0x40A481: "STR_HelpFile",
    0x40A4A5: "STR_Fmt_RankName",
    0x40A4AF: "STR_Fmt_ScorePercent",
    0x40A4E9: "STR_Class_Tooltips",
    0x40A4FA: "STR_Fmt_Percent",
}

STRUCTS = {
    "ScoreRecord": [
        ("name", 28, "char[N]  player name, NUL padded"),
        ("score", 4, "uint32    points"),
    ],
}

# ---------------------------------------------------------------- script
def run():
    from ghidra.program.model.listing import CodeUnit
    from ghidra.program.model.symbol import SourceType
    from ghidra.app.decompiler import DecompInterface

    here = os.path.dirname(os.path.abspath(__file__))
    outdir = os.path.join(here, "..", "analysis", "pseudo")
    os.makedirs(outdir, exist_ok=True)

    program = currentProgram
    st = program.getSymbolTable()
    fm = program.getFunctionManager()
    mem = program.getMemory()
    listing = program.getListing()

    # 1. rename functions
    for name, addr, comment in FUNCS:
        a = program.getAddressFactory().getDefaultAddressSpace().getAddress(addr)
        if a is None or mem.getBlock(a) is None:
            continue
        try:
            fn = fm.getFunctionAt(a)
            if fn is None:
                fn = fm.createFunction(name, a, None)
            else:
                fn.setName(name, SourceType.USER_DEFINED)
            fn.setComment(comment)
        except Exception as e:
            print("[!] func %s: %s" % (name, e))

    # 2. rename globals
    for name, addr, comment in GLOBALS:
        a = program.getAddressFactory().getDefaultAddressSpace().getAddress(addr)
        if a is None:
            continue
        try:
            sym = st.getPrimarySymbol(a)
            if sym is None:
                st.createLabel(a, name, True, SourceType.USER_DEFINED)
            else:
                sym.setName(name, SourceType.USER_DEFINED)
            st.getPrimarySymbol(a).setComment(comment)
        except Exception as e:
            print("[!] glob %s: %s" % (name, e))

    # 3. label strings
    for addr, name in STRINGS.items():
        a = program.getAddressFactory().getDefaultAddressSpace().getAddress(addr)
        if a is None or mem.getBlock(a) is None:
            continue
        try:
            s = listing.getDataAt(a)
            if s is None:
                st.createLabel(a, name, True, SourceType.USER_DEFINED)
        except Exception:
            pass

    # 4. decompile every anchor function
    ifc = DecompInterface()
    ifc.openProgram(program)
    manifest = {}
    for name, addr, comment in FUNCS:
        a = program.getAddressFactory().getDefaultAddressSpace().getAddress(addr)
        fn = fm.getFunctionAt(a)
        if fn is None:
            continue
        res = ifc.decompileFunction(fn, 120, monitor)
        txt = res.getDecompiledFunction().getC() if res.getDecompiledFunction() else "/* failed */"
        with open(os.path.join(outdir, name + ".c"), "w") as fh:
            fh.write("/* %s @ 0x%X -- %s */\n" % (name, addr, comment))
            fh.write(txt + "\n")
        manifest[name] = {"addr": hex(addr), "comment": comment}
    ifc.dispose()

    with open(os.path.join(outdir, "anchors.json"), "w") as fh:
        json.dump({"functions": manifest,
                   "menu_command_ids": {str(k): v for k, v in MENU_IDS.items()}},
                  fh, indent=2, ensure_ascii=False)
    with open(os.path.join(outdir, "menu_command_ids.h"), "w") as fh:
        fh.write("/* Menu command IDs recovered from RT_MENU + RT_ACCELERATOR.\n"
                 " * Labels are Simplified Chinese, verbatim from the resource. */\n")
        for k in sorted(MENU_IDS):
            fh.write("#define CMD_%-22s %d   /* %s */\n" % (MENU_IDS[k].split()[0] + "_", k, MENU_IDS[k].split(None, 1)[1]))
    print("[+] exported %d decompiled functions to %s" % (len(manifest), outdir))

run()
