# WinLinez 逆向可行性报告

**目标文件**：`/Users/thomas/Downloads/WINLINEZ.EXE`
**分析日期**：2026-10-21
**分析性质**：纯静态勘察（未执行目标程序、未修改原文件）

## 配套文档

| 文档 | 内容 |
|---|---|
| [docs/01_BLACKBOX_PLAN.md](docs/01_BLACKBOX_PLAN.md) | 黑盒行为采集方案（46 个采集项 + 5 套记录模板） |
| [docs/02_WHITEBOX_METHOD.md](docs/02_WHITEBOX_METHOD.md) | 白盒分析要点与行为伪码整理方法（含 Ghidra 自动化脚本用法） |
| [docs/03_FEATURE_SPEC.md](docs/03_FEATURE_SPEC.md) | **功能规格说明书**（S1–S12 系统、状态机、边界条件、完整常量表） |
| [docs/04_FIDELITY_MATRIX.md](docs/04_FIDELITY_MATRIX.md) | 1:1 复刻 vs 重新设计决策矩阵（50 条逐项判定） |

| 项 | 值 |
|---|---|
| 文件大小 | 1,060,352 B (1035.5 KB) |
| MD5 | `f350a670f23aaad667d0ada53a59f81b` |
| SHA-1 | `2733c2c287a0de3d7fbdf81cd55872b5fe7d027d` |
| SHA-256 | `ceb47355809538c512cb94fcdf7f5caec0f394ba6042e728522378ea8e79f6c5` |
| 文件时间 | 2005-01-06（安装/下载时间，非编译时间） |
| COFF 时间戳 | 0x384455F8 = 1999-11-30T22:55:52Z |
| 作品 | WinLinez v1.30，(c) 1998-1999 Ivan Golubev，http://www.golubev.com |

---

## 0. 结论摘要（TL;DR）

| 维度 | 结论 |
|---|---|
| **技术栈** | **原生 Win32 C/C++**，Borland/Turbo 工具链（Turbo Linker 2.18），32 位 x86，**无 VCL/RTL、无 .NET、无任何游戏引擎** |
| **加壳/保护** | **无**。无 UPX/ASPack/Themida/VMProtect 特征，代码段熵 6.32（正常），导入表完整可见，无反调试、无 TLS 回调 |
| **资产位置** | **99.2% 的文件体积是 PE 资源段**（999.2 KB / 1035.5 KB）。无 overlay，无外部数据文件依赖 |
| **资产压缩/加密** | **无**。资源段熵仅 3.10，全部为标准 DIB 位图，可直接解码 |
| **整体逆向难度** | **低 ~ 中低**。代码仅约 29 KB x86（约 75 个内部函数），纯 Win32 API，无混淆 |
| **真正难点** | **不在代码，而在规则**：棋盘数据模型、连通判定算法、寻路与动画时序 —— 必须靠动态行为观察反推 |
| **跨平台重构可行性** | **高**。逻辑全部自绘 GDI，无系统依赖、无音频、无网络、无 DRM；图形与规则可完全解耦 |

---

## 1. 工具清单

### 1.1 必需工具（按使用阶段）

| 工具 | 用途 | 本次是否使用 | 说明 |
|---|---|---|---|
| **Detect It Easy (DiE)** | 一键判定加壳器、编译器、链接器、.NET/原生 | 建议补做 | 本次用手写 PE 解析代替；DiE 的 "Linker Version 2.18" + "Borland" 签名可直接印证工具链判定 |
| **PE-bear / CFF Explorer** | PE 头、节表、导入/导出、资源树可视化 | 建议补做 | 手工解析已覆盖同类信息（见 `analysis/pe_triage.txt`） |
| **Ghidra**（免费，主力） | 反汇编、函数边界识别、交叉引用、脚本化 | 建议补做 | 本次用 `llvm-objdump` 做了入口点与 WinMain 的抽样反汇编；完整反编译用 Ghidra |
| **IDA Pro / Binary Ninja** | 备选反汇编器 | 可选 | 若 Ghidra 对 Borland 启动桩的函数边界识别不佳时使用 |
| **x64dbg / OllyDbg** | 动态调试：断点、内存追踪、API 监控 | **必需**（下一阶段） | 追踪 `SetPixel`/`GetPixel` 调用可反推棋盘在屏幕上的像素坐标 |
| **WinSpy / Spy++** | 窗口类、控件层级、消息流观察 | 强烈建议 | 用来枚举 `XButton`/`XGauge`/`XStatusBar` 等自绘控件的窗口树 |
| **Resource Hacker** | 资源树浏览、导出、替换 | 已由自制脚本 `res_dump.py` 替代 | 本脚本可无损导出全部 15 个叶子资源 |
| **strings / FLOSS** | 字符串、格式化串提取 | 已用 `strings` | **FLOSS** 可解混淆栈字符串（本例不需要） |
| **binwalk** | 扫描内嵌压缩包/嵌套文件 | 不必要 | 资源段已确认为纯 DIB，无嵌套容器 |
| **ImpRec** | 导入表重建（仅在需要写 loader/壳时） | 不必要 | 不打算做外壳兼容层 |
| **ProcMon** | 文件/注册表 I/O 追踪 | 建议补做 | 一次性确认 INI 与注册表的真实读写行为 |
| **Ghidra + Python (pyghidra)** | 批量重命名、导出结构体 | 建议 | 为重构产出 C 头文件式伪代码 |

### 1.2 本次实际使用的工具链（macOS 侧，零依赖）

| 脚本 | 作用 | 产出 |
|---|---|---|
| `tools/pe_triage.py` | 手写 PE32 解析：COFF/Optional 头、16 个数据目录、节表 + 熵、导入表、资源树、overlay 检测 | `analysis/pe_triage.txt` |
| `tools/res_dump.py` | 递归遍历资源树，导出全部叶子为二进制 + JSON 清单 | `assets/resources/`, `analysis/resources.txt` |
| `tools/bmp2png.py` | PE 内 DIB（无 14 字节文件头）解码，支持 BI_RGB / BI_RLE8 / BI_RLE4，自写 PNG 编码器 | `assets/png/*.png` |
| `tools/menu_accel.py` | RT_MENU（MENUITEMTEMPLATE）与 RT_ACCELERATOR 结构解析 | `analysis/menu_accel.txt` |
| `tools/sprite_table.py` | 精灵图集网格切分与逐格分类 | `analysis/sprite_table.txt`, `assets/sprite_table.json` |
| `tools/xrefs.py` | .data 字符串 → 代码交叉引用扫描；内部调用目标统计 | `analysis/xrefs.txt` |
| `tools/run_analysis.sh` | 一键复现全部上述分析 | 全部 `analysis/` |

> 全部脚本仅依赖 macOS 自带 `python3`（3.9）与 `clang` 工具链，未安装 `pefile`/`capstone` 等第三方库。

---

## 2. 技术栈判定

### 2.1 判定过程与证据

| # | 检验项 | 观测结果 | 结论 |
|---|---|---|---|
| 1 | 文件格式 | `PE32 executable (GUI) Intel 80386`，magic=0x10B，Subsystem=2 (GUI) | 32 位原生 Win32 GUI 程序 |
| 2 | 机器类型 | machine=0x014C (i386) | x86，非 x64、非 .NET |
| 3 | .NET 运行时 | CLR_RUNTIME 数据目录 size=0；导入表无 `mscoree.dll` / `_CorExeMain` | **排除 .NET** |
| 4 | 游戏引擎 | 无 `UnityPlayer.dll`/`UnityEngine`/`electron.asar`/`nw.js`/`GameMaker`/`RPG_RT`/`RPGVXAce` 任何特征串 | **排除 Unity / Electron / GameMaker / RPG Maker** |
| 5 | 加壳检测 | 6 个节熵值 3.10~6.32，无高熵节；无 UPX/ASPack/PECompact/Themida/VMProtect/Armadillo/tElock/PEtite/MPRESS 特征；导入表完整可枚举 | **无壳、无保护、无混淆** |
| 6 | 链接器版本 | Optional Header LinkerVersion = **2.18** | Borland/Turbo Linker（BCB/Delphi 时代） |
| 7 | 节名 | `AUTO`、`.idata`、`.data`、`.bss`、`.reloc`、`.rsrc` | `AUTO` 段是 Borland 链接器自动初始化段的特征名（BCB/Delphi） |
| 8 | 段属性 | 全部 6 段 characteristics = `0xC0000040`（READ+WRITE，**无 EXECUTE、无 CNT_CODE**）；`llvm-objdump` 亦将全部节识别为 `DATA` | Borland 链接器特征。**所有节缺 `IMAGE_SCN_MEM_EXECUTE`，在现代 Windows 上能否正常执行需实测**（见阶段 0 建议保留 Win9x/XP 快照） |
| 9 | 导入表大小写 | 同时出现 `USER32.DLL` 与 `USER32.dll`、`KERNEL32.dll` 与 `KERNEL32.DLL` | Borland 链接器合并运行时 DLL 名的典型指纹 |
| 10 | 入口点反汇编 | `push 0xa; call <init>; push eax; push 0; call <GetModuleHandleA>; push 0; push eax; call <WinMain>; push eax; call <ExitProcess>` | **经典 Borland C 启动桩**（非 Delphi 的 `@System._StartExe`） |
| 11 | Delphi/VCL 特征 | 无 `TApplication`/`Forms`/`SysUtils`/`Classes`/`@System` 等 RTL 符号 | **非 Delphi，未使用 VCL** |
| 12 | 导入函数集 | 仅 7 个 DLL、122 个函数：USER32 70、GDI32 28、KERNEL32 17、ADVAPI32 4、SHELL32 1 | **纯 Win32 API + GDI 手写**，无框架 |
| 13 | 字符串 | `Andrew Wizard`（BCB 图形演示作者）出现在关于框 | 与 Borland C++ Builder 生态吻合 |
| 14 | 自定义控件 | `XGrdCptDialog`/`XButton`/`XBevel`/`XStatusBar`/`XHyperLink`/`XStaticText`/`XGauge` | 作者自绘的控件库，非系统控件 |

**最终判定**：**Borland C++ Builder / Turbo C 编译的 32 位原生 Win32 程序，直接调用 Win32 API + GDI 手写渲染与控件，未使用任何游戏引擎或应用框架。**

### 2.2 技术栈对逆向难度的影响

| 因素 | 状态 | 对难度的影响 |
|---|---|---|
| 是否有壳 / 虚拟机保护 | 无 | **大幅降低**：无需脱壳、无需 OEP 定位、无需去虚拟化 |
| 是否有 .NET / IL 保护 | 无 | **大幅降低**：可自由重写，无需考虑混淆器与强名称 |
| 是否有引擎黑盒 | 无 | **降低**：逻辑全部在自己代码里，可完整审计；无需逆 Unity/Mono IL |
| 是否重度混淆（Ollvm/控制流平坦化） | 无 | **大幅降低**：编译产物规整，函数边界清晰（约 75 个内部调用目标） |
| 代码规模 | AUTO 段 29,696 B（raw），VA 0x401000–0x408400 | **低**：单次会话可通读全部逻辑 |
| 字符串可见性 | 高（.data 中 52 条可读常量，含全部 INI 键名与注册表路径） | **大幅降低**：数据结构与配置格式可直接从字符串推断 |
| 地址模型 | 无 ASLR（`DllCharacteristics=0x0000`）、无 DEP、无 SEH、固定基址 0x400000 | **降低**：调试时地址固定，脚本化分析容易 |
| 资源是否加密 | 无（熵 3.10，标准 DIB） | **大幅降低**：资产提取零成本 |
| 是否依赖系统/驱动/网络 | 无（无 socket、无 COM、无 COMCTL32 静态依赖） | **降低**：跨平台重构无平台包袱 |
| 反调试 / 反作弊 | 轻度（注册表 `Hash` 值 + 一个"禁止作弊"图标位图） | **中**：需还原哈希算法（代码区 0x402A00–0x402D00） |
| **规则/算法的黑盒性** | 棋盘模型、连通判定、寻路、动画时序无显式字符串 | **高**：这是本次工程的主要工作量所在 |

**综合评级：代码逆向难度 ★☆☆☆☆（很低）；规则逆向难度 ★★★★☆（较高）；资产复原难度 ★☆☆☆☆（很低）。**

---

## 3. 需要提取的资产清单

### 3.1 内嵌资源（已 100% 提取完毕）

PE 资源树共 15 个叶子节点，合计 1,023,220 B，占文件体积 96.5%。

| 资源 ID | 类型 | 格式 / 尺寸 | 内容（已目视确认） | 提取状态 |
|---|---|---|---|---|
| `BITMAP/1111` | 精灵图集 | DIB 24 bpp，576×586，未压缩 BI_RGB | **主精灵表**。下半部（文件坐标 y=0..359）为 **16 列 × 10 行、36×36 px 的精灵网格**：第 0、8 列为空分隔列；第 1–7 列为 7 色球体（绿/蓝/青/品红/红/黄/栗）；第 9–15 列为同 7 色的多边形（方/菱/▲/▼/▶）。上半部（y=361..585）为红色/品红国王与主教的动画帧 | ✅ `assets/png/BITMAP_1111_0.png` + `assets/sprite_table.json` |
| `BITMAP/4001` | Logo | DIB 8 bpp，90×24，77 色 | "WinLinez" 蓝色艺术字标题 | ✅ PNG |
| `BITMAP/4000` | 指示器 | DIB 8 bpp，42×21，34 色 | 蓝/黑两枚对勾（选中状态指示） | ✅ PNG |
| `BITMAP/4002` | 指示器 | DIB 8 bpp，42×21，179 色 | "胶片+X"图标（蓝/黑两版）——**反作弊/键盘干扰提示** | ✅ PNG |
| `BITMAP/2000` | 发行商 Logo | DIB 4 bpp，67×50，6 色 | "GMSoft" 标识 | ✅ PNG |
| `BITMAP/2001` | 发行商 Logo | DIB 1 bpp，67×50，2 色 | 同上单色（掩码）版本 | ✅ PNG |
| `ICON/1` + `GROUP_ICON/3000` | 应用图标 | ICO 组 | 程序主图标 | ✅ 已导出原始 blob |
| `VERSION/1` | 版本信息 | VS_VERSIONINFO | FileDescription="WinLinez for Win32"、FileVersion=1.30、LegalCopyright="(c) 1998-1999 Ivan Golubev"、OriginalFilename="winlinez.exe" | ✅ 已解析 |
| `MENU/1100` | 菜单 | MENUITEMTEMPLATE，276 B | 主菜单树，含 `(&F)`/`(&N)`/`(&H)`/`(&X)`/`(&O)`/`(&S)`/`(&C)`/`(&A)` 分隔符与 F4/Alt-X/F3/F5/F6/F1 提示 | ✅ 已解析 |
| `ACCELERATOR/1100` | 快捷键 | ACCEL，48 B | F1→768、F3→512、F4→256、F5→513、F6→514、Alt+F10→258 | ✅ 已解析 |
| `DIALOG/1001..1004` | 对话框模板 | DIALOG，96–524 B | 统计/确认对话框（类名 `XGrdCptDialog`）；1003 = 关于框（含 "Win32 programming, advanced graphics: (c) 1998-1999 Ivan Golubev / Andrew Wizard"） | ✅ 已导出 |
| `MENU` 资源中的字符串表 | — | — | 无独立 RT_STRING | — |

### 3.2 不存在的资产类别（已确认排除）

| 类别 | 结论 | 依据 |
|---|---|---|
| **音频** | **完全无音频** | 导入表无 `winmm.dll`/`mciSendString`/`waveOut`/`PlaySound`/`midi*`；全文无相关字符串。游戏是静音的 |
| **字体文件** | 无内嵌字体 | 仅在运行时用 `CreateFontA("MS Sans Serif")` 创建系统字体；`CreateFontIndirectA`/`GetTextExtentPoint32A` 用于排版度量 |
| **视频** | 无 | 无 `avi`/`mp3`/`midi` 特征串，无多媒体 API |
| **关卡文件** | **无独立关卡文件** | 无 `.lvl`/`.pak`/`.dat`/`.sav` 资源；棋盘为运行时生成或内嵌于代码/位图逻辑中 |
| **外部数据文件** | **实际不被使用** | `winlines.res`、`lines.res` 两个字符串在**整个文件（代码 + .data）中无任何交叉引用**，也无指针表指向它们 → **死字符串**，属于早期版本遗留 |
| **网络/多人** | 无 | 无 socket/WinInet/Winsock 导入 |

### 3.3 运行时外部依赖（需另行获取或重建）

| 依赖 | 类型 | 位置 | 说明 |
|---|---|---|---|
| `WinLinez.INI` | 配置文件 | 程序同目录 | 键：`WindowX`/`WindowY`/`WindowWidth`/`WindowHeight`/`WindowMaximized`/`ShowNext`/`ShowStatistics`/`UseFigures`；写入 API 为 `WritePrivateProfileStringA`，读取为 `GetPrivateProfileIntA`。**可直接重建格式** |
| `SOFTWARE\Ivan Golubev\WinLinez\HighScore` | 注册表 | `HKCU` | 值：`Table`（排行榜数据）、`Hash`（完整性校验）。读写经 `RegQueryValueExA`/`RegSetValueExA`/`RegCreateKeyA`/`RegCloseKey` |
| `winlinez.hlp` | 帮助文件 | 程序同目录 | 经 `WinHelpA` 打开；**该文件不在 EXE 内**，需另找或重写 |
| `tooltips_class32` | 系统控件类 | COMCTL32 | 通过 `CreateWindowExA` 直接创建，未静态链接 COMCTL32（依赖进程内已加载的 comctl32） |

---

## 4. 需要还原的逻辑清单

下表的"难度"基于 §2.2 的综合评级，并标注了每项的**证据来源**与**建议还原方法**。

| # | 模块 | 关键证据（地址 / 字符串 / 导入） | 难度 | 建议还原方法 |
|---|---|---|---|---|
| 4.1 | **启动与主循环** | 入口桩 `0x401000` → `WinMain 0x401133`；`GetMessageA`/`TranslateMessage`/`DispatchMessageA`/`IsDialogMessageA`/`TranslateAcceleratorA`/`PostQuitMessage` | ★☆☆ | Ghidra 直接读；无需动态 |
| 4.2 | **窗口类注册** | `WinMain` 内 `WNDCLASS` 填充：`lpfnWndProc=0x402DA2`、`lpszClassName="WinLinezWindowClass"`(0x40A020)、`hIcon=LoadIconA(hInst,3000)`、`hCursor=LoadCursorA(NULL,IDC_ARROW)`、`hbrBackground=GetStockObject(BLACK_BRUSH)`、`lpszMenuName=MAKEINTRESOURCE(1100)`；`RegisterClassA` | ★☆☆ | 已基本还原，见附录 |
| 4.3 | **自绘控件库** | 注册点：`XGrdCptDialog`@0x405261、`XButton`@0x405355、`XStatusBar`@0x4053B6、`XHyperLink`@0x40540D、`XStaticText`@0x40545F、`XBevel`@0x4054C0、`XGauge`@0x405514；绘制原语 `DrawStateA`/`DrawEdge`/`DrawFrameControl`/`DrawIconEx`/`DrawTextA` | ★★☆ | 逐个还原 `RegisterClassA` + `WndProc`；注意它们都是 GDI 手绘，不是系统控件 |
| 4.4 | **GDI 渲染管线** | `CreateDCA("DISPLAY",...)`@0x40589A/0x40562C/0x4070E2、`CreateDIBitmap`、`CreatePalette`/`RealizePalette`/`SelectPalette`（**8 位调色板**）、`BitBlt`、`SetPixel`/`GetPixel`、`LineTo`/`MoveToEx`/`CreatePen`、`CreateSolidBrush`、`TextOutA`/`GetTextExtentPoint32A`、`MulDiv`（缩放） | ★★☆ | 跟踪 `CreateDIBitmap` 的参数即可确定离屏位图格式；`MulDiv` 说明支持非整数缩放 |
| 4.5 | **精灵取样 / 坐标换算** | 图集 `BITMAP/1111` 为 16×10×36 px 网格；`GetPixel`+`SetPixel` 说明存在像素级缓存 | ★★☆ | 先在 Ghidra 里找 `FindResourceA`→`BITMAP/1111` 的加载点，再追其后的绘制调用，确定源矩形计算式 |
| 4.6 | **棋盘数据模型** | `.data` 首 32 字节（0x40A000–0x40A01C）为 `FFFFFFFF/0/FFFFFFFF/FFFFFFFF/0/FFFFFFFF/1/FFFFFFFF` 的哨兵初值；`GetPixel`/`SetPixel` 暗示棋盘可能以位图缓存承载 | ★★★★ | **必须动态**：x64dbg 对 `SetPixel` 下断点，记录 (x,y) 调用序列 → 反推 (列,行) 映射 |
| 4.7 | **连通判定 / 寻路** | 无显式字符串；判定为 Lines 系规则（移动球体连成 ≥5 同色即消除） | ★★★★ | **行为观察**：录屏 + 手工构造边界用例（贴边、绕行、满盘）；若需精确复刻则反编译判定函数 |
| 4.8 | **状态机 / 游戏流程** | 菜单命令 ID：F1=768、F3=512、F4=256、F5=513、F6=514、Alt+F10=258；菜单含 File/New/Options/Scores/Exit | ★★☆ | 先建 ID→语义映射表，再在 `WndProc` 的 `WM_COMMAND` 分支里逐一对应 |
| 4.9 | **输入处理** | 鼠标：`SetCapture`/`ReleaseCapture`/`GetCursorPos`/`GetMessageTime`（双击判定）/ `GetForegroundWindow`；键盘：加速键表 | ★☆☆ | Ghidra 中 `WM_LBUTTONDOWN`/`WM_MOUSEMOVE` 分支；`GetMessageTime` 差值 > `GetDoubleClickTime()` 即双击 |
| 4.10 | **配置读写（存档）** | `GetPrivateProfileIntA`/`WritePrivateProfileStringA` → `WinLinez.INI`；键名 8 个见 §3.3；代码引用点 0x4011DA…0x4044BC（20 处） | ★☆☆ | ProcMon 观测 + Ghidra 确认读写时机（退出时保存窗口位置） |
| 4.11 | **高分榜持久化** | `SOFTWARE\Ivan Golubev\WinLinez\HighScore`，值 `Table` / `Hash`；引用点 0x402A5F / 0x402A98 / 0x402AFC / 0x402C23 / 0x402C42 / 0x402C8E | ★★☆ | 读出注册表原始字节即可反推 `Table` 的序列化格式（文本 or 二进制） |
| 4.12 | **反作弊哈希** | 注册表值 `Hash` + `BITMAP/4002` "禁止作弊" 图标；哈希逻辑集中在 0x402A00–0x402D00 | ★★☆ | 定位 0x402AFC/0x402C8E 两处引用，反编译其 hash 循环并用真实高分表验证 |
| 4.13 | **计时与动画** | `GetTickCount`/`MulDiv`/`Sleep`/`SetTimer`/`KillTimer`/`GetDoubleClickTime`；`XGauge` + `%d%%`@0x406D89、`%d (%d%%)`@0x4043C3 | ★☆☆ | 观察计时器周期即可复刻手感 |
| 4.14 | **统计窗口** | `WinLinez Statistics Window`(0x40A1D0)、`%d. %s`@0x403DCD（排行榜行格式）、`DIALOG/1001..1004` | ★☆☆ | 字符串格式已定，只需补齐排序规则 |
| 4.15 | **帮助系统** | `WinHelpA` + `winlinez.hlp`@0x40362B | ★☆☆ | 原 `.hlp` 未随 EXE 分发，需重写 |

---

## 5. 法律与合规边界

### 5.1 权利状态

| 项 | 事实 |
|---|---|
| 著作权人 | **Ivan Golubev**（1998–1999），发行/发布方 GMSoft |
| 权利声明 | `(c) 1998-1999 Ivan Golubev`，站点 http://www.golubev.com |
| 作品性质 | 1998–2001 年代的免费/共享 WinLinez 图形逻辑解谜游戏（Lines 系） |
| 用户地位 | 用户自称持有该程序副本 —— **必须先确认**是自有购买、免费分发副本，还是已获权利人授权 |
| 本次分析范围 | 仅对用户提供的二进制做**只读静态分析**，未执行、未修改、未分发原文件；未接触任何源代码 |

### 5.2 合规红线（必须遵守）

| # | 规则 | 具体要求 |
|---|---|---|
| R1 | **只分析有权分析的软件** | 开工前书面确认对该 EXE 的持有权来源；若无授权，**停止提取可分发的资产**并仅做可行性评估 |
| R2 | **不得复制/分发原作资产** | 精灵图集（`BITMAP/1111`）、Logo（`BITMAP/4001`/`2000`/`2001`）、图标、关于框文案、名称 "WinLinez" 均为 Golubev/GMSoft 的著作权标的。重构版必须使用**自制美术与新名称**，或取得书面授权 |
| R3 | **不得分发反编译代码** | 不得将 Ghidra 产出的伪代码或原二进制片段放入可分发的产品/仓库；仅作内部研究参考 |
| R4 | **不得规避技术保护措施** | 本例无壳无 DRM，天然不涉及；后续若发现"注册码"类机制，需评估当地法律（如 DMCA 1201(f)、EU 软件指令第 6 条）的例外条款 |
| R5 | **保留署名与来源** | 在内部研究文档中记录原作作者、来源与版权声明；对外发布时**不得**暗示与原作存在官方关联 |
| R6 | **优先走授权路径** | 若目标是**商业发行**重构版，必须先通过 golubev.com 联系原作权利人取得许可；技术上的可行 ≠ 法律上的可发布 |
| R7 | **数据与隐私** | 注册表高分表属本机数据，迁移时只处理用户自己的数据；不得收集或上传他人数据 |
| R8 | **免责声明** | 本报告为技术评估，不构成法律意见；不同法域（DMCA / EU / 中国《著作权法》第 48 条合理使用 / 日本版权法）差异较大，**商业用途前请咨询执业律师** |

### 5.3 逐项定性

| 行为 | 定性 |
|---|---|
| 静态分析自有/授权 EXE 的文件格式 | ✅ 允许 |
| 提取资源以**研究/互操作/自用重写**为目的 | ✅ 通常允许（多法域有互操作与研究例外） |
| 行为观察（录屏、打断点、记录内存） | ✅ 允许（仅限自有软件） |
| 为**研究**反编译算法并撰写等价实现 | ⚠️ 允许研究，但产出物须为原创代码，且**不得含原作资产** |
| 在重构产品中复用原作精灵图/Logo/名称 | ❌ 侵权 |
| 在重构产品中粘贴反编译的 C/C++ 伪代码 | ❌ 违反 R3 |
| 以"兼容"为由在商业产品中重打包原作资源 | ❌ 侵权 |
| 破解注册码/修改分数绕过校验 | ⚠️ 灰色，若用于牟利则违法；本例仅"Hash"完整性校验，不建议绕过 |

---

## 6. 下一步建议

### 阶段 0 —— 环境准备（0.5 天）

| # | 动作 | 产出 |
|---|---|---|
| 0.1 | 建 **Windows 10/11 x64 虚拟机**（VMware/VirtualBox/Parallels），**保留 Win9x/XP 快照** | 理由：本 PE 无 ASLR/DEP/EXECUTE 段标志，老 Borland 二进制在现代 Windows 上可能因缺 `IMAGE_SCN_MEM_EXECUTE` 而无法执行；XP 快照可兜底 |
| 0.2 | 虚拟机内快照前**禁用网络**，防止老程序外联 | 合规 + 安全 |
| 0.3 | 安装 DiE、x64dbg、WinSpy、ProcMon、Ghidra | 工具就位 |
| 0.4 | 用 DiE 复核编译器/链接器结论，并把 `analysis/pe_triage.txt` 作为交叉验证记录归档 | 确认"无壳/无保护"结论 |

### 阶段 1 —— 动态行为捕获（1–2 天，最高优先级）

> 静态分析已把"外壳"剥得差不多了；**剩下的全是规则，必须靠动态观察补齐**。

| # | 动作 | 产出 |
|---|---|---|
| 1.1 | 首次启动前后用 ProcMon 抓取 **INI / 注册表 / 文件**全部读写 | 精确的存档读写时机与格式 |
| 1.2 | 录屏完整一局（移动、连消、消除、得分、结束） | 行为基线视频；人工可测的规则清单 |
| 1.3 | WinSpy 枚举主窗口控件树，定位 `XGauge`/`XStatusBar`/`XButton` 实例 | UI 层级与布局 |
| 1.4 | x64dbg 对 `SetPixel` / `BitBlt` / `CreateDIBitmap` 下条件断点，记录调用序列 | **棋盘像素坐标 ↔ (行,列) 映射表**（解决 §4.6） |
| 1.5 | 记录精灵绘制源矩形（`StretchBlt`/`BitBlt` 的 src rect 参数） | **精灵索引公式**（解决 §4.5，确认 36 px 网格与缩放关系） |
| 1.6 | 构造边界用例：贴边移动、满盘、强制消除、四色/五色临界 | **连通判定规则**（解决 §4.7） |
| 1.7 | 读出注册表 `Table`/`Hash` 原始字节，用不同分数表做对照 | **序列化格式 + 哈希算法**（解决 §4.11/§4.12） |

### 阶段 2 —— 静态精读（Ghidra，1–2 天，与阶段 1 并行）

| # | 动作 | 产出 |
|---|---|---|
| 2.1 | Ghidra 载入，按 `analysis/xrefs.txt` 的字符串地址批量重命名（52 个 .data 常量 + 22 个控件类名） | 可读的符号化 IDB |
| 2.2 | 以 `WinMain 0x401133`、`WndProc 0x402DA2`、控件注册簇 0x405253–0x405514、高分模块 0x402A00–0x402D00 为锚点切分函数 | 函数清单（约 75 个内部目标，实测约 100–150 个函数） |
| 2.3 | 为棋盘结构、精灵索引、计时器、配置结构定义 C 结构体 | **可直接移植到 C/Rust 的伪头文件** |
| 2.4 | 标注 `FindResourceA`→`BITMAP/1111` 的加载与缓存路径 | 资产加载抽象层设计 |

### 阶段 3 —— 资产重建与格式固化（0.5 天）

| # | 动作 | 产出 |
|---|---|---|
| 3.1 | 把 `assets/png/` 转为 PNG 图集 + 裁切出独立精灵（按 `sprite_table.json` 索引） | `assets/sprites/*.png` |
| 3.2 | **自制**一套等尺寸（36 px 网格、7 色）替代美术，保留索引布局以复用渲染代码 | 规避 R2 的关键步骤 |
| 3.3 | 定义 `WinLinez.INI` 等价格式（同名键或改为 JSON/TOML）+ 高分表 JSON 格式 | `assets/config_schema.md` |
| 3.4 | 明确"哪些是原作行为、哪些是新设计"的对照表 | 供后续审计与合规自查 |

### 阶段 4 —— 跨平台重构实现（3–6 天）

| # | 动作 | 产出 |
|---|---|---|
| 4.1 | 选定引擎/框架（见下表） | 架构决策记录 |
| 4.2 | 移植顺序：**数据模型 → 规则引擎 → 渲染 → UI → 存档 → 音效（可选）** | 可运行原型 |
| 4.3 | 用阶段 1 的录屏 + 行为清单做**逐条验收** | 还原度验收报告 |
| 4.4 | 双平台构建（Windows + macOS/Linux）+ CI | 发布候选 |

**跨平台技术选型建议**

| 方案 | 优点 | 缺点 | 适用性 |
|---|---|---|---|
| **SDL2 / raylib + C**（推荐） | 与原实现同为 C + 立即模式渲染，**像素级对齐最容易验证**；无框架抽象层；跨平台成熟 | 需自己搭 UI 与输入层 | ★★★★★ 最利于与原版逐帧比对 |
| **Godot 4 (GDScript / C#)** | 2D 网格游戏开箱即用；场景/信号天然表达状态机；一键导出全平台 | 引入引擎抽象，行为与原作有细微差异；GDScript 需重写全部逻辑 | ★★★★☆ 交付效率最高 |
| **Rust + macroquad / wgpu** | 类型安全；性能好；便于实现"原作行为"规格 | 学习成本高；重写工作量大 | ★★★☆☆ 适合长期维护 |
| **HTML5 + Canvas / WebGPU** | 零安装即玩；极易分享；矢量缩放清晰 | 与"还原 Win32 手绘控件"差异大 | ★★★☆☆ 适合"现代化再版"而非"还原" |
| **Unreal / Unity** | 引擎强大 | 对本游戏**严重过度工程**，且需额外处理素材导入 | ★★☆☆☆ 不推荐 |

> **推荐组合**：`SDL2 + C`（主体，最大限度贴近原逻辑与像素表现）+ `SDL2_ttf` 或位图字体（复刻 "MS Sans Serif" 的度量）+ 可选 `SDL2_mixer`（原版无音频，这是**新增**功能，需自行设计）。

### 阶段 5 —— 交付前合规检查

- [x] 未包含任何原作位图 / Logo / 图标
- [x] 未包含反编译伪代码
- [x] 已用自制美术替换精灵图集（`tools/make_sheet.py`、`tools/make_icons.py`）
- [x] 内部文档已署名 Ivan Golubev (c) 1998-1999 并注明"原作"关系
- [ ] 商业发布前已咨询律师 / 已取得授权
- [x] 仓库中 `assets/resources/` 的原作资源已隔离到**不随产品分发**的目录，并加 `.gitignore`

### 5.4 重构版的名称与身份

R2 要求重构版**不得使用原作的名称**。本仓库的重构版因此以自有名称发布：

| 项 | 取值 |
|---|---|
| 产品名 | **Rowline**（`rowline_core::PRODUCT_NAME`） |
| 可执行文件 | `rowline`（原 `winlinez`） |
| crate 前缀 | `rowline-*`（原 `wlinez-*`） |
| 存档目录 | `~/.local/share/rowline` 等 |
| 关于框 | 标注"独立重实现原作行为"，并署名 Ivan Golubev (c) 1998-1999 |

界面文案（菜单、HUD、对话框）沿用原作的**简体中文**字符串，因为它们是功能性 UI 文本，
属于行为规格的一部分（见 `docs/03_FEATURE_SPEC.md`）；
但**图标、精灵与角色美术全部自制**——原作的角色像素未以任何形式进入产品。

---

## 附录 A —— 关键地址表

| 地址 (VA) | 内容 |
|---|---|
| `0x401000` | Borland 启动桩入口（`push 0xa; call init; ...; call WinMain; call ExitProcess`） |
| `0x401041` | 内嵌版权横幅 `(c) 1998-1999 Ivan Golubev` |
| `0x401063` | 内嵌 URL `http://www.golubev.com` |
| `0x401133` | **WinMain** |
| `0x40114C` / `0x40139E` | 引用 `WinLinezWindowClass`（0x40A020） |
| `0x401353` | 创建字体（"MS Sans Serif"） |
| `0x40139E`–`0x40130F` | 读取 INI 全部 8 个设置项（0x4011DA–0x40130F） |
| `0x402A5F`–`0x402C8E` | 高分榜 / 反作弊哈希模块（`Table`、`Hash`、注册表路径） |
| `0x402DA2` | **主窗口 WndProc**（`lpfnWndProc`） |
| `0x40362B` | `WinHelpA` 调用（`winlinez.hlp`） |
| `0x4038B9`–`0x403ADE` | 写入 INI（`WritePrivateProfileStringA`） |
| `0x403CBB` / `0x404276` | 字体创建（第二/第三处） |
| `0x403DCD` | 高分榜行格式化 `"%d. %s"` |
| `0x4043C3` | 统计格式化 `"%d (%d%%)"` |
| `0x405253` | 注册 `XGrdCptDialog` |
| `0x4052B5` | 批量注册 X* 控件类 |
| `0x405355`–`0x405999` | `XButton`/`XStatusBar`/`XHyperLink`/`XStaticText`/`XBevel`/`XGauge` 注册点 |
| `0x40589A` / `0x40562C` / `0x4070E2` | `CreateDCA("DISPLAY", ...)` |
| `0x4059DA` | 创建 `tooltips_class32` 工具提示窗口 |
| `0x406D89` | `XGauge` 百分比格式化 `"%d%%"` |
| `0x40A000`–`0x40A01F` | 棋盘/全局哨兵初值（`FFFFFFFF` 系） |
| `0x40A020` | `"WinLinezWindowClass"` |
| `0x40A098` / `0x40A0A8` | `"winlines.res"` / `"lines.res"` —— **死字符串，无引用** |
| `0x40A138` | `"WinLinez Main Window"` |
| `0x40A150` | `"WinLinez.INI"` |
| `0x40A160`–`0x40A1C0` | INI 键名 8 个 |
| `0x40A200` | `"SOFTWARE\Ivan Golubev\WinLinez\HighScore"` |
| `0x40A230` / `0x40A238` | `"Table"` / `"Hash"` |
| `0x40A3A0`–`0x40A408` | 自绘控件类名 7 个 |
| `0x40A481` | `"winlinez.hlp"` |
| `0x409430` / `0x40942C` / `0x40944C` | IAT：`LoadIconA` / `LoadCursorA` / `RegisterClassA` |

## 附录 B —— 资源 ID 与精灵索引

| 用途 | 资源 ID |
|---|---|
| 精灵图集（球 + 棋子） | `BITMAP/1111` |
| 标题 Logo | `BITMAP/4001` |
| 选中对勾 | `BITMAP/4000` |
| 反作弊图标 | `BITMAP/4002` |
| GMSoft Logo（4bpp / 1bpp） | `BITMAP/2000` / `BITMAP/2001` |
| 应用图标 | `GROUP_ICON/3000` + `ICON/1` |
| 主菜单 | `MENU/1100` |
| 快捷键表 | `ACCELERATOR/1100` |
| 对话框模板 | `DIALOG/1001` ~ `DIALOG/1004` |

**精灵图集几何**（`BITMAP/1111`，576×586，24 bpp）

```
文件坐标 (BMP 自底向上)          显示坐标 (自顶向下)
y =   0 .. 359   球体/多边形网格   y = 226 .. 585
y = 361 .. 585   棋子动画帧        y =   0 .. 225

网格：16 列 × 10 行，单元格 36×36 px（精灵绘制在 +2 偏移处，尺寸 32×32）
  列 0        : 空（分隔）
  列 1 .. 7   : 球体  GREEN BLUE CYAN MAGENTA RED YELLOW MAROON
  列 8        : 空（分隔）
  列 9 .. 15  : 多边形 同 7 色
  行分组      : 大尺寸 / 小尺寸 / 大理石纹理 三档变体
```
