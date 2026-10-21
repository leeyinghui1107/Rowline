# 黑盒采集操作手册（Windows VM）

> 配套：`docs/01_BLACKBOX_PLAN.md`（采集项定义）、`tools/blackbox/`（脚本）
> 目标：把 54 个采集项从"人工看屏幕"变成**脚本产出 + 可审计的证据文件**。

---

## 0. 一次性准备

| # | 步骤 | 说明 |
|---|---|---|
| 0.1 | 准备 Windows VM，**关闭网络** | 1999 年的程序不应外联；顺带满足合规要求 |
| 0.2 | 快照基线 | 每个用例可一键回滚 |
| 0.3 | 安装 Python 3.9+ | 采集脚本零第三方依赖 |
| 0.4 | 安装 MSVC 或 MinGW | 编译 `winprobe.c` |
| 0.5 | 装 ProcMon / WinSpy / OBS / AutoHotkey | 记录 I/O、窗口树、录屏、宏回放 |
| 0.6 | 设区域设置为**简体中文 + CP936** | 菜单是简体中文；俄文提示用 CP1251 另行转写 |
| 0.7 | 复制采集工具 | `tools/blackbox/` 与 `tools/replay/` 整目录拷到 VM |

### 编译 winprobe

```bat
cd tools\blackbox
cl /nologo /W4 winprobe.c /Fe:winprobe.exe      REM MSVC
REM 或
gcc -O2 -Wall winprobe.c -o winprobe.exe          REM MinGW
```

---

## 1. 运行时快照（菜单 / 窗口 / 配置）

这是**信息密度最高**的一步，一次运行可关闭 6+ 个采集项。

```bat
REM 1) 先启动 WinLinez.EXE
REM 2) 再运行采集器（默认按类名 "WinLinez" 匹配）
winprobe.exe --out C:\bb\run1 --class WinLinezWindowClass
```

产出：

| 文件 | 内容 | 对应采集项 |
|---|---|---|
| `windows.json` | 窗口/控件树：类名、文本、样式、矩形、ID、可见性 | **BB-U01**、BB-M02、BB-U05、BB-U08 |
| `menu.json` | **运行时**菜单树：ID、文本、勾选/禁用状态 + 加速键表 | **BB-M01**、BB-M02、BB-M05 |
| `config.json` | `WinLinez.INI` 原始字节 + 注册表 `Table`/`Hash` 原始字节 | BB-U01、BB-N08、BB-N10 |
| `metrics.json` | 屏幕尺寸、目标窗口客户区尺寸 | BB-U03 |

### 一致性自动比对

```bat
python tools\blackbox\analyze.py probe C:\bb\run1 --out C:\bb\run1
```

输出对照表（期望值来自 `docs/03_FEATURE_SPEC.md`）：

| 检查项 | 期望 |
|---|---|
| 主窗口类名 | `WinLinezWindowClass` |
| 菜单 ID 集合 | 256 / 257 / 258 / 512 / 513 / 514 / 768 / 769 / 16528 |
| 加速键数量 | 6 |
| 注册表 `Table` 大小 | 512 B |
| 注册表 `Hash` 大小 | 16 B |
| INI 键 | 8 个已知键全部出现 |

> **运行时菜单树是最终事实来源**——它能一次性确认静态解析的 `RT_MENU` 是否正确，
> 并揭示**运行时勾选状态**（静态分析看不到）。

---

## 2. 截图几何测量（UI 布局）

### 2.1 采集

1. 把窗口调整到目标客户区尺寸（**宽度与高度都 ≥ 444×372**，否则原版公式会溢出，见 `docs/04` D3）
2. 用系统截图或 OBS 截取**客户区**（不要含标题栏）
3. 存为 `C:\bb\shots\board_1280x960.png`

### 2.2 测量与比对

```bat
python tools\blackbox\geometry.py C:\bb\shots\board_1280x960.png ^
    --json C:\bb\shots\board_1280x960.geometry.json

python tools\blackbox\analyze.py shot C:\bb\shots\board_1280x960.png ^
    --out C:\bb\run1
```

`analyze.py shot` 的对照表：

| 检查项 | 期望（来自规格） |
|---|---|
| board origin X | `(clientW − 328) / 2` |
| board origin Y | `(clientH − 376) / 2 + 46` |
| 格边长 | 36 px |
| 网格边长 | 9 × 9 |
| 棋盘边框 | 327 × 328 px（居中跨距 328） |
| 网格覆盖率 | ≥ 0.95（所有精灵都落在 9×9 网格内） |

> **测量原理**：先检测棋盘外框（长水平灰线）得到绝对原点，再用"饱和像素落在 9×9 槽位内"验证。
> 不依赖规格公式，因此可以反过来证伪公式。
>
> 该工具已在 8 组尺寸 × 2 种精灵集上做过回归（用模拟器生成的图，期望值 100% 命中）。

### 2.3 多尺寸扫描（BB-U03）

```bat
for %%S in (444x372 640x480 1024x768 1280x960 1440x900 1920x1080) do (
    python tools\blackbox\simulate.py --out C:\bb\shots\mock_%%S.png --size %%S --seed 1
)
REM 真实测量：把窗口调到各尺寸后逐个截图，再对每个跑 geometry.py
```

---

## 3. 输入录制与回放

### 3.1 录制

```bat
python tools\blackbox\inputlog.py --out C:\bb\captures\session1.csv --seconds 600
```

操作约定：

| 按键 | 作用 |
|---|---|
| **F8** | 打一个 MARKER，用于把长录制切分成单个用例 |
| 其余按键 / 鼠标 | 照常操作，脚本自动记录按下/抬起/移动 |

输出 CSV：`t_ms, kind, key, vk, x, y, extra`

### 3.2 回放（AutoHotkey 版本，适合需要精确复现的用例）

`tools/blackbox/inputlog_ahk.ahk` 提供：

```ahk
; 用法: AutoHotkey inputlog_ahk.ahk out.csv
; 录制: 按 Ctrl+Alt+R 开始/停止; F8 打标记
```

### 3.3 用途

| 用途 | 说明 |
|---|---|
| 生成可复现用例 | 用例卡里直接引用 `session1.csv` 的时间区间 |
| 差分回放 | 把录制转成 `tools/replay/cases/*.trace` 喂给差分门 |
| 行为对拍 | 同一 seed 下，原作与重构版的输入序列完全相同 → 输出可直接比对 |

---

## 4. 数值表的采集

| 采集项 | 方法 | 产出 |
|---|---|---|
| BB-N01 棋盘维度 | `geometry.py` 的 `grid_side` | 自动 |
| BB-N02 颜色集合 | `geometry.py` 可扩展：统计每个槽位的主导色 | 需扩展 |
| BB-N03 初始球数 | 新局截图 → 统计非空槽位 | 半自动 |
| BB-N04 补球数量 | 消除前后截图对比 | 半自动 |
| BB-N05 消除阈值 | 构造 4 连 / 5 连用例 | 人工 |
| BB-N06 分值表 | 记录每次消除的分数增量 | 人工 + CSV 汇总 |
| BB-N08 高分榜条目数 | `config.json` 的 `Table` 长度 ÷ 32 | **自动** |

---

## 5. 证据归档与清单回填

### 5.1 目录约定

```
blackbox/
  checklist.json          采集状态（由 checklist.py 维护）
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

### 5.2 回填清单

```bat
python tools\blackbox\checklist.py set BB-M01 --status done ^
    --evidence "blackbox/run1/menu.json" ^
    --note "运行时菜单树与静态解析完全一致"

python tools\blackbox\checklist.py summary
python tools\blackbox\checklist.py check          REM 一致性自检
python tools\blackbox\checklist.py export > blackbox/progress.md
```

### 5.3 自检规则

`checklist.py check` 强制：
- 标为 `done` 的项**必须**有证据文件；
- 标为 `blocked` 的项**必须**写明原因。

---

## 6. 回填规则模型（把黑盒结论写进代码）

规则语义已全部定标。改规则时要同时改**三处**（Python 参考实现 + C 实现 + Rust 实现），
然后跑差分门：

| 参数 | 文件 | 定标依据 |
|---|---|---|
| `threshold = 5` | `tools/replay/model.py` / `reference/c-impl/wl_model.h` / `rust/crates/core/src/rules.rs` | `0x401AF8`（BB-G03 / N05） |
| `spawn_per_move = 3`（仅未消线回合） | 同上 | `0x401CD4`（BB-G02 / G06 / N04） |
| `initial_balls = 5` | 同上 | `0x402801`（BB-G01 / N03） |
| `source_keeps_colour = false` | 同上 | 移动后源格清空（BB-G05） |
| 清除形状 / 分值表 / 结束条件 | `rules.rs`、`engine.rs` 中的固定逻辑 | `0x401BD4` / `0x401E1D` / `0x401CA1` |

```bat
REM 改完两处后
make test
```

> **两侧必须同时改**。只改一边，差分门会立刻报分歧——这正是这个门存在的意义。
> 通过后，把 `docs/03_FEATURE_SPEC.md` §7 / §8 里对应条目的置信度从 ⬜ 改成 ✅，
> 并在 `docs/06_IMPLEMENTATION_STATUS.md` §3 更新取值。

---

## 7. 全流程速查

```bat
REM ---- 1) 运行时快照
winprobe.exe --out C:\bb\run1 --class WinLinezWindowClass
python tools\blackbox\analyze.py probe C:\bb\run1 --out C:\bb\run1

REM ---- 2) 截图几何
python tools\blackbox\analyze.py shot C:\bb\shots\board.png --out C:\bb\run1

REM ---- 3) 输入录制
python tools\blackbox\inputlog.py --out C:\bb\captures\s1.csv --seconds 600

REM ---- 4) 回填清单
python tools\blackbox\checklist.py set BB-U04 --status done ^
    --evidence C:\bb\run1\geometry.json
python tools\blackbox\checklist.py summary

REM ---- 5) 定标规则并跑差分门
make test
```

---

## 8. 不依赖 Windows 的部分

以下工具可在任意平台运行，用于**准备与回归**：

| 工具 | 用途 |
|---|---|
| `tools/blackbox/simulate.py` | 生成模拟窗口图，给 `geometry.py` / `analyze.py` 做回归 |
| `tools/blackbox/geometry.py` | 截图 → 棋盘几何（已 8/8 回归通过） |
| `tools/blackbox/analyze.py` | 一致性比对 |
| `tools/blackbox/checklist.py` | 清单管理 |
| `tools/blackbox/pngio.py` | 零依赖 PNG 读写 |
| `tools/replay/*` | 差分回放门 |
