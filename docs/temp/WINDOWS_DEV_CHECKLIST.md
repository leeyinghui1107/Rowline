# Windows 开发机工作清单

> **状态**：部分执行 —— 2026-10-21 已在 Windows 11（10.0.26200）+ MSYS2/MinGW 上跑通工具链与产品侧自检，见 §9 与 `blackbox/windows_verify/20261021-192704/`；原作黑盒采集尚未开始（本仓库不含 `WinLinez.EXE`）
> **适用**：后续主战场迁移到 Windows 平台
> **关联**：`tools/blackbox/README.md`（采集操作手册） · `docs/01_BLACKBOX_PLAN.md`（采集项定义） · `docs/03_FEATURE_SPEC.md`（行为规格） · `docs/11_SCREENSHOT_OBSERVATIONS.md`（截图证据）
> **证据落点**：`blackbox/`（清单与证据） · `docs/11` §7（未决事项）

本文回答两个问题：

1. **在 Windows 上要问原作哪些问题**，才能把当前代码里仍是"猜测"的参数换成事实；
2. **在 Windows 上怎么干**，包括环境、坑、命令顺序，以及每确认一项之后的固定动作。

---

## 0. 背景：哪些已经定了，哪些还是猜的

静态分析 + 实机 + 截图已经定掉的事实（**不需要**再问）：

| 事实 | 结论 | 证据 |
|---|---|---|
| 棋盘 | 9 × 9 = 81 格 | 二进制 + 截图 |
| 颜色 | 7 种 | 二进制 + 截图 |
| 消除阈值 | 5 | `0x401AF8` 硬编码 |
| 消除形状 | 过落点的整条直线（横 / 竖 / 两斜），可同时多条 | `0x401BD4` + `0x40A000` |
| 移动 | **4 邻域**寻路，源格清空 | `0x4016xx` 步表 |
| 新局布球 | **恰好 5 颗随机球** | `0x402801` + 截图 §5c |
| 下一组 | **3 颗球**；普通回合补 3 颗，消线回合不补 | `0x401CD4` + 截图 §2.1 |
| 计分 | 5→10 / 6→12 / 7→18 / 8→28 / 9→42 | `0x401E1D` |
| 游戏结束 | 棋盘**无空位**才结束（不是“无路可走”） | `0x401CA1` |
| HUD 结构 | **顶部横栏**：左分数框 / `下组` + 3 格预览 / `颜色` / 右分数框 | 截图 §2 |
| 两个分数框归属 | **左 = 历史记录分，右 = 本局当前分** | 截图 §5b + `0x40B2A8` / `0x40B06C` |
| 对手 / AI | **没有**；单人游戏，空闲 480 s 画面不变 | 实机连续截图 |
| 榜单 | 16 行，格式 `N. 名字    分数`，空槽 `- 空的 -` | 截图 §5b + 二进制 |
| 进榜时机 | 游戏结束时（`0x402CB1`），分数 > 末行才进；确定名字后显示总榜 | `0x4032E2` |
| 榜单初值 | 自带 `国王 100` | 二进制（最高分默认 100） |
| 菜单 | 简体中文，`文件(&F)` / `选项(&O)`，9 个原作命令 ID + 1 个重设计扩展（固定分辨率） | RT_MENU 精确解析 |
| 音频 | **完全没有** | 二进制导入表 |

**规则模型（`core::rules::RuleModel`）的最终值**：

| 字段 | 值 | 状态 |
|---|---|---|
| `threshold` | 5 | ✅ `0x401AF8` |
| `spawn_per_move` | 3（仅未消线回合） | ✅ `0x401CD4` |
| `initial_balls` | 5 | ✅ `0x402801` |
| `source_keeps_colour` | `false` | ✅ 源格清空 |
| 拐弯上限 / 重访源格 | 不再建模：移动走 4 邻域最短路 | ✅ |
| AI 对手 | 不存在 | ✅ |

---

## 1. 环境准备（一次性）

### 1.1 主机与沙箱

| # | 步骤 | 说明 |
|---|---|---|
| 1 | 准备 **Windows 11 主机** + **Hyper-V 快照沙箱**（或 VMware/VirtualBox） | 原作会写注册表与 INI，沙箱可一键回滚；没有沙箱就用**专用本地账户** + 定期导出注册表键备份 |
| 2 | 沙箱**关闭网络** | 1999 年的程序不应外联；同时满足合规要求 |
| 3 | 每个用例前打快照 | 用例卡里记录快照名 |
| 4 | 区域设置 → **简体中文 + CP936** | 菜单是简体中文 |

### 1.2 工具链

| 用途 | 工具 | Windows 上的获取 / 注意 |
|---|---|---|
| **一键自检** | `tools/win/verify.ps1` | 自动定位真实的 Python / cargo / gcc / make，依次跑 `cargo build/test`、`make test`、`make shot`、编译 `winprobe.c`，结果写入 `blackbox/windows_verify/<时间戳>/summary.json`。`powershell -NoProfile -ExecutionPolicy Bypass -File tools\win\verify.ps1` |
| 运行时采集 | `tools/blackbox/winprobe.c` | **纯 Win32**（只依赖 `windows.h`/`stdio`/`stdlib`/`string`/`tchar`）→ MSVC `cl /nologo /W4 winprobe.c /Fe:winprobe.exe` 或 MinGW `gcc -O2 -Wall winprobe.c -o winprobe.exe` |
| 输入录制 | `tools/blackbox/inputlog.py` | **Windows 原生**（`ctypes` + `GetAsyncKeyState`），纯标准库 |
| 宏回放 | `tools/blackbox/inputlog_ahk.ahk` | **AutoHotkey v2**（`Ctrl+Alt+R` 开始/停止，`F8` 打 MARKER） |
| 截图测量 | `tools/blackbox/geometry.py` | 纯标准库，跨平台 |
| 一致性比对 | `tools/blackbox/analyze.py` | 纯标准库，跨平台 |
| 零依赖 PNG 读写 | `tools/blackbox/pngio.py` | 纯标准库 |
| 回归夹具 | `tools/blackbox/simulate.py` | 按确认公式生成模拟图，供 `geometry.py` 反证 |
| C 对照实现 | `reference/c-impl/` | Makefile 用 **gcc 风格**（`-std=c99`）→ 装 **mingw-w64** 后 `make CC=gcc`，或用 **WSL2** 跑整条链路 |
| 四方差分门 | `tools/replay/diff_replay.py` | 已兼容 Windows 的 `.exe` 后缀 |
| 产品构建 | Rust + wgpu | `cargo build --release` → **DX12**；CI 的 windows-latest 已覆盖 |
| 其它 | Python 3.9+、ProcMon、WinSpy、OBS | |

### 1.3 ⚠️ Windows 特有的 8 个坑

1. **截图必须本机、无损、100%**
   - 系统缩放设为 **100%**（笔记本默认常是 125/150%，一缩放像素测量就废了）
   - 截**客户区**、存 **PNG**（不要 JPG）
   - 用 Win+Shift+S / OBS「窗口捕获」/ PowerShell `CopyFromScreen`
   - **不要**用 RDP 截屏（帧缓冲会被缩放）
   - 顺带把 125% / 150% 各来一组，正好验证我们「按显示器 DPR 自适应」的实现

2. **1999 年的程序会污染系统**
   - `HKCU\SOFTWARE\Ivan Golubev\WinLinez\HighScore`
   - 工作目录下的 `WinLinez.INI`
   - 可能加载 Sound / Joystick 驱动
   - → 沙箱快照；每轮采集后 `reg export` 备份

3. **反作弊弹窗会打断录制**
   - 一旦改了 `Table`/`Hash`，原作会弹 `MB_YESNO|MB_ICONWARNING`
   - AHK 脚本要能识别并自动点掉，否则录制脚本会卡死
   - **顺便把弹窗原文截下来** —— 这是我们目前完全没实现的行为

4. **1999 二进制在现代 Windows 上的 DPI 感知**
   - 老程序常被系统做位图缩放 → 内容不可信
   - → 设为「系统级（增强）DPI 感知」，或干脆按 100% 跑

5. **`python3` 不存在**
   - Windows 的 Python 安装器只给 `python`
   - Makefile 已参数化：`make PYTHON=python ...`
   - 但 `where python` 常常先命中 `...\WindowsApps\python.exe`（应用商店别名，不是解释器）→ 用真实路径，或让 `verify.ps1` 自动跳过

6. **用户名含中文 → GNU ld 链接 proc-macro 失败**
   - 现象：`cargo build` 在 `serde_derive` / `zerocopy-derive` 处报 `cannot find ...\rustlib\x86_64-pc-windows-gnu\lib\libstd-*.rlib: No such file or directory`，但文件确实存在
   - 原因：MSYS2 的 `ld.exe` 打不开非 ASCII 路径下的 rlib
   - 处理：把 Rust 工具链放到纯 ASCII 路径（本机为 `D:\rustup` / `D:\cargo`）：
     ```bat
     set RUSTUP_HOME=D:\rustup
     set CARGO_HOME=D:\cargo
     set PATH=D:\cargo\bin;C:\msys64\ucrt64\bin;%PATH%
     ```
     `tools\win\verify.ps1` 会自动探测 `D:\rustup` / `D:\cargo`

7. **Python 的默认文本编码是 CP936**
   - `json.load(open(...))` 读 UTF-8 的 `blackbox/checklist.json` 会报 `UnicodeDecodeError: 'gbk' codec ...`
   - 已修：`tools/` 下所有文本 I/O 显式 `encoding="utf-8"`；新增脚本请沿用

8. **make 的默认 shell 可能是 `cmd.exe`**
   - 旧 Makefile 在 `test` 目标里用 `if [ -x ... ]`，cmd 下报 `-x 此时不应有`，`make` 以 Error 255 结束
   - 已修：改用 make 级 `$(wildcard)` 判断；C/Rust 产物统一加 `$(EXE)` 后缀；`mkdir`/`clean` 改用 Python
   - 若仍想让 make 用 sh：`set PATH=C:\msys64\usr\bin;%PATH%`，或 `make SHELL=C:/msys64/usr/bin/sh.exe ...`

---

## 2. 已经为 Windows 准备好的东西

| 资产 | 位置 | 说明 |
|---|---|---|
| **Windows 一键自检** | `tools/win/verify.ps1` | 工具链定位 + 全部门禁 + 证据归档（9 项检查，见 §9） |
| Makefile Windows 兼容 | `Makefile` | `.exe` 后缀、make 级 Rust 门禁判断、Python 建目录；`make shot` 的 `SHOT_BIN` 顺序 bug 已修 |
| UTF-8 文本 I/O | `tools/**/*.py` | 所有 `open()` 显式 `encoding="utf-8"`，避免 CP936 解码失败 |
| 布局**确认公式**报告 | `tools/blackbox/analyze.py` | 用 `x=(W−328)/2`、`y=(H−376)/2+46` 预测并与截图实测比对（604×371 实机为 138/44） |
| 布局回归夹具 | `tools/blackbox/simulate.py` | 按确认公式生成模拟图，供 `geometry.py` 反证 |
| Windows 差分门兼容 | `tools/replay/diff_replay.py` | 自动尝试 `.exe` 后缀 |
| Makefile 解释器参数化 | `Makefile` | `make PYTHON=python CC=gcc ...` |
| **离线出图**（无需 GPU/显示器） | `rust/crates/tools/src/bin/rowline-shot.rs` | `make shot`；可在 Windows 上直接渲染任意场景成 PNG，与软件后端逐像素一致 |
| CI 覆盖 | `.github/workflows/ci.yml` | ubuntu/macos/windows 三平台 + 差分门 + 布局门 + 离线渲染 smoke test |

---

## 3. 问题清单（已闭环 / 仍开放）

### P0 · 规则（全部闭环）

| ID | 问题 | 结论 | 证据 |
|---|---|---|---|
| **BB-G06 / G02 / G08** | 消除后补几颗球？ | **不补**；只有未消线的回合才补 3 颗 | `0x401BD4` 消线分支不调用 `0x401CD4` |
| **BB-G03** | 阈值与形状 | 阈值 5；过落点的**整条直线**（横/竖/两斜），不是洪水填充 | `0x401AF8`、`0x401BD4` |
| **BB-G04** | 允许拐弯吗？ | 移动走 **4 邻域**最短路，不限制拐弯数 | `0x4016xx` 步表 |
| **BB-G05** | 路径能穿过已有球吗？ | 不能；只能走空格，源格落子后清空 | `0x4016xx` |
| **BB-G11 / N06** | 计分公式 | **非线性**：5→10 / 6→12 / 7→18 / 8→28 / 9→42 | `0x401E1D` |
| **BB-G07 / G14** | 结束条件 | **棋盘无空位**才结束，弹 `游戏结束!`；随后处理进榜 | `0x401CA1`、`0x4032E2` |
| **BB-G12** | 分数封顶 | 未观察到；分值表最大 42/次 | `0x401E1D` |
| **BB-G15** | 新纪录提示 | **游戏结束时**弹一次；分数进榜才弹名字框，确定后显示总榜 | `0x402CB1` |

### P1 · 对手模型（已证伪）

| 问题 | 结论 |
|---|---|
| **CPU 会不会自己走子？** | **不会**。空闲 480 s 的连续截图像素不变，没有自动落子 |
| 「国王」是什么？ | 榜单里的**历史记录 / 出厂默认 100 分**，不是对手 |
| 是否需要 AI？ | **不需要**；模型里不建模对手 |

### P1 · 几何与布局（已闭环）

| ID | 结论 |
|---|---|
| **BB-U02** | 外框 327×328，内缩 2 px，格 36 px；边框/圆角属重设计（docs/04） |
| **BB-U03** | `x=(W−328)/2`、`y=(H−376)/2+46`，与尺寸无关 |
| **BB-U04** | `0x40B060/64` 是棋盘原点；`0x40B078/7C` 是**预览球组** X/Y |
| **BB-U01** | INI 8 键已由 `winprobe` 确认；默认窗口 604×371 客户区 |

### P2 · 输入（部分开放）

| ID | 状态 |
|---|---|
| **BB-K03 / K05** | ✅ 点两下（先选后落），与实现一致 |
| **BB-K01 / K02 / K04 / K06–K08** | 🟡 快捷键表已确认；双击/悬停/右键为**重设计**，不作 1:1 要求 |
| **BB-M03 / M05 / M06** | ✅ 菜单与快捷键等价；游戏结束前不禁用菜单 |
| **BB-K09 / K10** | ✅ 坐标随客户区重算；无纯键盘操控棋盘 |

### P3 · 数值与 UI（已闭环）

| ID | 状态 / 计划 |
|---|---|
| **BB-N02** | 原作 7 色已提取；重构版用自制色板（合规） |
| **BB-N04** | ✅ 3 颗（见 BB-G06） |
| **BB-N09** | ✅ 稳定排序（同分保持插入顺序） |
| **BB-N10** | ✅ 预填 `挑战者`；输入上限 24 字符；默认名兜底 |
| **BB-N11** | ✅ 补球频率恒定，不随分数变化 |
| **BB-U07** | ✅ 菜单 513「显示统计」：已实机确认并实现（8 行、`count (percent%)`、`,消除-N`；默认关闭），见 docs/11 §5e |
| **BB-U08 / U09 / U12** | ✅ 三个对话框布局按截图实现；工具提示无 |
| — | ✅ 反作弊弹窗不复刻（docs/04 P-04） |

---

## 4. 执行顺序（可直接照抄）

```bat
REM ── 0. 工具链
REM    Python 3.9+ / MSVC Build Tools / AutoHotkey v2 / ProcMon / OBS / WinSpy
REM    rustup + cargo ; mingw-w64（或用 WSL2 跑 C 参考实现）
REM    区域设置：简体中文 + CP936 ；系统缩放：100%

REM ── 1. 先确认我们自己的链路在 Windows 上是通的（5 分钟，别跳过）
cd rust
cargo build --release --workspace --all-features
cargo test  --release --workspace --all-features
cd ..
make PYTHON=python CC=gcc replay check && make PYTHON=python CC=gcc test
REM    期望：布局门 PASSED (0 failures) + 四方差分门 RESULT: ALL MATCH
REM    或直接跑一键自检（推荐；自动处理 Python/cargo/.exe/UTF-8）：
powershell -NoProfile -ExecutionPolicy Bypass -File tools\win\verify.ps1

REM ── 2. 沙箱里跑原作，运行时快照（一次关掉 6+ 项）
start WinLinez.EXE
winprobe.exe --out C:\bb\run1 --class WinLinezWindowClass
python tools\blackbox\analyze.py probe C:\bb\run1 --out C:\bb\run1

REM ── 3. 几何（100% 缩放，5 组尺寸，每组截客户区 PNG）
python tools\blackbox\analyze.py shot C:\bb\shots\board_1280x960.png --out C:\bb\run1
REM    看 "the layout formula fits" 是否 OK（604×371 实测 138/44）

REM ── 4. 规则用例（AHK 自动摆盘 + inputlog.py 录制，F8 打 MARKER）
python tools\blackbox\inputlog.py --out C:\bb\captures\s1.csv --seconds 600
REM    a) 消除前后连拍 30 帧 → 确认消线回合不补球
REM    b) 3/4/5/6 连各一局 → 阈值 5 与阶梯分值
REM    c) 4 邻域移动 / 源格清空
REM    d) 「谁都不操作」录 3 分钟 → 已确认无 AI

REM ── 5. 输入行为：单击 / 双击 / 拖拽 / 右键 / 键盘

REM ── 6. 回填清单
python tools\blackbox\checklist.py set BB-G06 --status done ^
  --evidence C:\bb\shots\refill_30.png --note "消除后补 3 球，来自下一组"
python tools\blackbox\checklist.py summary
python tools\blackbox\checklist.py check
```

### 产品侧附加验证（Windows 上顺手做，价值很高）

| # | 动作 | 目的 |
|---|---|---|
| 1 | 原作与 `rowline` **并排**同屏，同 seed 同输入串 | 逐格比对棋盘与分数，验证行为等价 |
| 2 | 125% / 150% / 200% 三档 DPI 各截一张 | 验证「逻辑坐标 + 每显示器 DPR + 整数倍缩放」在真实 Windows 上不错位 |
| 3 | `cargo run --release -p rowline-tools --features dpi --bin rowline-dpi-probe` | 读真实多屏 DPI 组合 |
| 4 | `make shot` 出离线 PNG，与屏幕截图并排比 | 验证离线渲染与窗口渲染一致 |

---

## 5. 每确认一项之后的固定动作

1. **同时**改三处（只改一边，差分门会当场报分歧）：
   - `tools/replay/model.py` → `RuleModel`
   - `reference/c-impl/wl_model.h` / `.c` → `wl_rule_model`
   - `rust/crates/core/src/rules.rs` → `RuleModel`
2. `make test` → 必须 `RESULT: ALL MATCH`
3. 行为变了 → `python3 tools/replay/diff_replay.py --record --all` 重冻 golden
4. `docs/03_FEATURE_SPEC.md` 里对应条目的置信度 ⬜/🟡 → ✅，并写明**证据文件名**
5. 更新 `docs/11_SCREENSHOT_OBSERVATIONS.md` §7「仍未确定」表
6. `python3 tools/blackbox/checklist.py set <ID> --status done --evidence <文件>` 回填
7. 提交并推送（**SSH 推**，否则 PAT 无 `workflow` scope 会被拒）

---

## 6. 证据归档目录

```
blackbox/
  checklist.json          采集状态（checklist.py 维护）
  run1/
    windows.json  menu.json  config.json  metrics.json
    conformance_shot.json  conformance_probe.json  geometry.json
  shots/
    board_1280x960.png  board_1280x960.geometry.json
  captures/
    session1.csv
  notes/
    BB-G03.md            每个用例的手工笔记
```

---

## 7. 常见故障速查

| 现象 | 原因 | 处理 |
|---|---|---|
| `git push` 被拒：`refusing to allow a PAT to create or update workflow` | PAT 缺 `workflow` scope | 改用 SSH：`git remote set-url origin git@github.com:leeyinghui1107/Rowline.git` |
| CI 的 "C reference implementation" 失败，`ld: open() failed ... 'build/rowline-replay'` | `build/` 被 gitignore，全新 clone 没有该目录 | 编译前 `mkdir -p build`（已修） |
| `make test` 报 `python3: command not found` | Windows 只有 `python` | `make PYTHON=python ...`（已参数化） |
| `make test` 报 `-x 此时不应有` / Error 255 | make 用了 `cmd.exe`，旧 `test` 目标里的 `if [ -x ]` 不是 cmd 语法 | 已修（make 级 `$(wildcard)`）；或用 `make SHELL=C:/msys64/usr/bin/sh.exe` |
| `cargo build` 报 `cannot find ...libstd-*.rlib`（文件其实存在） | 用户目录含中文，MSYS2 `ld.exe` 打不开非 ASCII 路径 | `set RUSTUP_HOME=D:\rustup` + `set CARGO_HOME=D:\cargo`，见 §1.3 第 6 条 |
| `make shot` 报 `No rule to make target '...rowline-shot'` | 旧 Makefile 把 `SHOT_BIN` 定义在规则之后，且 Windows 产物带 `.exe` | 已修（定义提前 + `$(EXE)`） |
| `checklist.py` / `analyze.py` 报 `UnicodeDecodeError: 'gbk' codec` | Python 在 Windows 上默认按 CP936 打开文本 | 已修（显式 UTF-8）；旧脚本请补 `encoding="utf-8"` |
| 差分门报 "rust-implementation not built" | Windows 下二进制是 `.exe` | `diff_replay.py` 已兼容；若仍失败请手动确认 `rust\target\release\rowline-replay.exe` 存在 |
| 截图量出来的格子数和预期差 1~2 px | 截图被缩放 / 带了标题栏 / 系统 DPI ≠ 100% | 重截：客户区、100% 缩放、PNG 无损 |
| 录制脚本卡住不动 | 反作弊弹窗抢焦点 | AHK 里加对该窗口的自动关闭 |
| `geometry.py` 报 `coverage` 偏低 | 截图含窗口边框或被裁切 | 重新裁到客户区 |

---

## 8. 剩余工作（已全部闭环）

原先的 5 个高风险猜测**全部闭环**：

1. ✅ **无 AI**：空闲 480 s 画面不变，单人游戏
2. ✅ **消线不补球**，普通回合补 3 球（`0x401BD4` / `0x401CD4`）
3. ✅ **阈值 5 + 整条直线**（`0x401AF8` / `0x401BD4`）
4. ✅ **点两下**输入；4 邻域移动、源格清空
5. ✅ **布局公式**已由 `0x402EA7` / `0x4036E8` + 604×371 实机确认

产品侧打磨也已在 2026-10-21 收尾（不影响规则等价）：

| 项 | 结果 |
|---|---|
| 菜单 513「显示统计」的真实内容 | ✅ 实机确认：8 行（空格子 + 7 色），`count (percent%)`，球色行追加 `,消除-N`；见 docs/11 §5e 与 `C:\bb\shots\stats_client.png` |
| 浅色主题棋盘底纹 | ✅ `Palette::board_backdrop` / `board_cell`：暖纸底 + 米色格子 |
| `Fixed` 显示策略菜单项 | ✅ `选项 · 固定分辨率`（id 515），与 `Ctrl+R` 等价 |
| 素材 @4x | ✅ 精灵表 `@4x.png` + 图标 `@4x.png`（500% 缩放 1:1） |
| 消除动画时序 | ✅ `clear5` 连拍确认：2 × 50 ms 溶解（`0x401C48`）+ 420 ms 胜利立绘；已实现 |
| 输入细节（双击/拖拽/悬停/右键/修饰键） | ✅ 已逐项实测并记录；原作无悬停/拖拽/右键，重设计增加悬停环与路径预览 |
| 黑盒清单 | ✅ `blackbox/checklist.json` **54/54**，`checklist.py check` 全部一致 |

---

## 9. Windows 侧收尾验证

### 9.1 已在本机完成（2026-10-21）

证据目录：`blackbox/windows_verify/20261021-192704/`（`summary.json`：**9/9 PASS**）

| 检查 | 结果 | 证据 |
|---|---|---|
| `cargo build --release --workspace --all-features` | ✅ exit 0（Rust 1.99.1 GNU host，MSYS2 gcc 16.1.0） | `logs/cargo-build.log` |
| `cargo test --release --workspace --all-features` | ✅ **140 passed / 0 failed**（22 个测试目标 + doc-tests） | `logs/cargo-test.log` |
| `make test` | ✅ 布局门 `PASSED (0 failure(s))` + 四方差分门 `RESULT: ALL MATCH` | `logs/make-test.log` |
| `make shot` | ✅ `build/shot.png`，1280×900，47,260 B | `logs/make-shot.log` |
| 中文渲染 | ✅ `文件` / `选项` / `下组` / `颜色` / `国王` / `挑战者` 均为实际字形，**无方框** | `build/shot.png` |
| `winprobe.c`（MinGW gcc） | ✅ 编译通过（`build/winprobe.exe`） | `logs/winprobe.log` |
| 黑盒管线 `simulate.py` → `analyze.py shot` | ✅ 模拟图 1024×768，布局公式 delta `+0/+0`，7 种颜色全部识别 | `shot/conformance_shot.json` |
| `checklist.py summary/check`（UTF-8） | ✅ 54 项正常加载，`all items consistent` | `logs/checklist-summary.log` |
| `rowline-dpi-probe`（真实显示器） | ✅ 读到 1 台显示器：3120×2080 px，**scale 2.00**（200%） | `logs/dpi-probe.log` |

- [x] Windows 上 `cargo test --release --workspace --all-features` 全绿
- [x] Windows 上 `make PYTHON=... CC=gcc test` → `RESULT: ALL MATCH`
- [x] Windows 上 `make shot` 出图正常（中文不显示方框 → 系统 CJK 字体会被正确找到）
- [x] `rowline-shot` 在 Windows 上跑通（CJK 优先命中 `msyh`，拉丁回退 `Arial`）
- [x] `winprobe.exe` 可在 Windows 上编译
- [x] 黑盒工具链（`simulate.py` / `analyze.py` / `checklist.py`）在 Windows 上跑通
- [x] `rowline-dpi-probe` 读到真实显示器 DPI（本机 200%）

### 9.2 仍待完成（需要原作或真实显示器）

- [ ] 原作与重构版**并排对拍**记录（本仓库不含 `WinLinez.EXE`，需在沙箱内提供）
- [ ] 125% / 150% / 200% 三档 DPI 截图留档
- [x] 黑盒采集回填（`blackbox/checklist.json` 当前 **54/54**；悬停/双击/右键等按“原作行为 + 重设计”分别记录）

复跑命令：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools\win\verify.ps1
```

它会新建 `blackbox/windows_verify/<时间戳>/`，并在最后打印 `N/N checks passed`。
