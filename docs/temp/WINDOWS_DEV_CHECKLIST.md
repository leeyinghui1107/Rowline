# Windows 开发机工作清单

> **状态**：待执行（在 Windows 开发机上完成黑盒阶段 + 产品侧验证）
> **适用**：后续主战场迁移到 Windows 平台
> **关联**：`tools/blackbox/README.md`（采集操作手册） · `docs/01_BLACKBOX_PLAN.md`（采集项定义） · `docs/03_FEATURE_SPEC.md`（行为规格） · `docs/11_SCREENSHOT_OBSERVATIONS.md`（截图证据）
> **证据落点**：`blackbox/`（清单与证据） · `docs/11` §7（未决事项）

本文回答两个问题：

1. **在 Windows 上要问原作哪些问题**，才能把当前代码里仍是"猜测"的参数换成事实；
2. **在 Windows 上怎么干**，包括环境、坑、命令顺序，以及每确认一项之后的固定动作。

---

## 0. 背景：哪些已经定了，哪些还是猜的

静态分析 + 已有截图已经定掉的事实（**不需要**再问）：

| 事实 | 结论 | 证据 |
|---|---|---|
| 棋盘 | 9 × 9 = 81 格 | 二进制 + 截图 |
| 颜色 | 7 种 | 二进制 + 截图 |
| 消除阈值 | 5 | 二进制常量 `[0x40B07C]` |
| 新局布球 | **恰好 5 颗随机球** | 截图 §5c |
| 下一组 | **3 颗球**，消耗队首、队尾补新球 | 截图 §2.1 |
| HUD 结构 | **顶部横栏**：左分数框 / `下组` + 3 格预览 / `颜色` / 右分数框 | 截图 §2 |
| 两个分数框归属 | **左 = 国王（CPU），右 = 挑战者（玩家）** | 截图 §5b |
| 榜单 | 16 行，格式 `N. 名字    分数`，空槽 `- 空的 -` | 截图 §5b + 二进制 |
| 榜单初值 | 自带 `国王 100` | 二进制（最高分默认 100） |
| 菜单 | 简体中文，`文件(&F)` / `选项(&O)`，9 个命令 ID | RT_MENU 精确解析 |
| 音频 | **完全没有** | 二进制导入表 |

**尚未确定、且当前代码里是"参数化猜测"的部分**（`core::rules::RuleModel`）：

| 字段 | 当前值 | 状态 |
|---|---|---|
| `threshold` | 5 | 二进制常量，缺黑盒复核 |
| `source_keeps_colour` | `true` | 🟡 猜测 |
| `max_turns` | `None`（不限拐弯） | 🟡 猜测 |
| `refill` | **0（消除后不补球）** | 🟡 **最可疑** |
| `points_per_ball` | 1 | 🟡 猜测 |
| `min_score_delta` | 0 | 🟡 猜测 |
| `allow_revisit_source` | `true` | 🟡 猜测 |
| CPU 是否存在 | 只有一个分数在涨 | ❓ **模型级空白** |

> `refill = 0` 意味着我们的游戏玩几轮棋盘就空了，与「棋盘满则结束」的模型自相矛盾 —— 几乎肯定是错的。

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
| 运行时采集 | `tools/blackbox/winprobe.c` | **纯 Win32**（只依赖 `windows.h`/`stdio`/`stdlib`/`string`/`tchar`）→ MSVC `cl /nologo /W4 winprobe.c /Fe:winprobe.exe` 或 MinGW `gcc -O2 -Wall winprobe.c -o winprobe.exe` |
| 输入录制 | `tools/blackbox/inputlog.py` | **Windows 原生**（`ctypes` + `GetAsyncKeyState`），纯标准库 |
| 宏回放 | `tools/blackbox/inputlog_ahk.ahk` | **AutoHotkey v2**（`Ctrl+Alt+R` 开始/停止，`F8` 打 MARKER） |
| 截图测量 | `tools/blackbox/geometry.py` | 纯标准库，跨平台 |
| 一致性比对 | `tools/blackbox/analyze.py` | 纯标准库，跨平台 |
| 零依赖 PNG 读写 | `tools/blackbox/pngio.py` | 纯标准库 |
| 回归夹具 | `tools/blackbox/simulate.py` | 已支持 `--layout centred\|legacy` |
| C 对照实现 | `reference/c-impl/` | Makefile 用 **gcc 风格**（`-std=c99`）→ 装 **mingw-w64** 后 `make CC=gcc`，或用 **WSL2** 跑整条链路 |
| 四方差分门 | `tools/replay/diff_replay.py` | 已兼容 Windows 的 `.exe` 后缀 |
| 产品构建 | Rust + wgpu | `cargo build --release` → **DX12**；CI 的 windows-latest 已覆盖 |
| 其它 | Python 3.9+、ProcMon、WinSpy、OBS | |

### 1.3 ⚠️ Windows 特有的 5 个坑

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

---

## 2. 已经为 Windows 准备好的东西

| 资产 | 位置 | 说明 |
|---|---|---|
| 布局**双假设**报告 | `tools/blackbox/analyze.py` | 截图符合性同时给出 `centred`（截图所见）与 `legacy`（二进制常量）两套预测及 delta，并明确判定哪个成立（两者差 110/233 px，肉眼可辨） |
| 双假设回归夹具 | `tools/blackbox/simulate.py --layout` | 可生成任一布局的模拟图 |
| Windows 差分门兼容 | `tools/replay/diff_replay.py` | 自动尝试 `.exe` 后缀 |
| Makefile 解释器参数化 | `Makefile` | `make PYTHON=python CC=gcc ...` |
| **离线出图**（无需 GPU/显示器） | `rust/crates/tools/src/bin/rowline-shot.rs` | `make shot`；可在 Windows 上直接渲染任意场景成 PNG，与软件后端逐像素一致 |
| CI 覆盖 | `.github/workflows/ci.yml` | ubuntu/macos/windows 三平台 + 差分门 + 布局门 + 离线渲染 smoke test |

---

## 3. 必答问题清单（按优先级）

### P0 · 会改变规则实现（每项确认后要同步改三份实现）

| ID | 问题 | 现状 | VM/开发机上怎么测 |
|---|---|---|---|
| **BB-G06 / G02 / G08** | 消除后棋盘怎么变？补几颗球？从哪来？落在哪？是否消耗「下一组」？ | `refill = 0`（几乎肯定错） | 造一次 5 连消除，**消除前后逐帧截图**；数非空格子数、看新球落点与颜色来源；连做 3 次看球数是否恒定 |
| **BB-G03** | 消除阈值是 4 还是 5？连线是**直线**还是**任意连通**？ | `threshold=5`，8 邻域洪水填充 | 分别摆 3/4/5/6 连各一局，只走一步 |
| **BB-G04** | 允许拐弯吗？最多几次？ | `max_turns = None` | 摆 L 形 / U 形 / 回字形路径 |
| **BB-G05** | 路径能穿过已有球吗？能绕回起点吗？ | `allow_revisit_source = true`；穿越异色球禁止（推断） | 造必须绕自己的盘面、必须穿过异色球的盘面 |
| **BB-G11 / N06** | 计分：按球数线性还是按长度阶梯？ | `points_per_ball=1` | 消除 5/6/7 连各一次，记录分数增量，建立对照表 |
| **BB-G07 / G14** | 游戏结束条件：满盘 / 无路可走 / 分数上限？结束画面？ | 无合法移动即结束 | 一直玩到结束，录结束瞬间 |
| **BB-G12** | 分数有封顶吗？ | 未知 | 造高分消除 |
| **BB-G15** | 新纪录提示：何时弹？弹几次？ | 超过历史最高分即弹 | 刻意破纪录 |

### P1 · 人机对战模型（最大的模型级空白）

| 问题 | 说明 |
|---|---|
| **CPU 会不会自己走子？** | **谁都不操作，录 3 分钟**，看有没有球自己移动、分数自己变化。三种可能：(a) 真正的 AI 对手在同一块棋盘上走子；(b)「国王」只是榜单里的历史记录/占位；(c) 双人轮流操作 |
| 若有 AI，它的走子规则、节奏、是否双方轮流？ | 决定要补一整套对手模型（当前引擎完全没有） |
| 榜单里为什么有「国王」这条？ | 只是初始值，还是 CPU 的成绩？与上一条一起回答 |

### P1 · 几何与布局（100% 缩放截图一次性解决）

| ID | 要拿到什么 |
|---|---|
| **BB-U02** | 棋盘外框矩形、网格内缩、**格子的边框粗细与圆角半径**、棋盘区背景的确切颜色 |
| **BB-U03** | 窗口拖到 5 组尺寸（880×892 / 1024×900 / 1280×960 / 1440×900 / 1920×1080）各截客户区 → 判断棋盘是**居中**还是**按偏移左对齐** |
| **BB-U04** | 棋盘原点的确切规则（与二进制 `[0x40B078]=(right−0x6C)/2` 的矛盾，用 `analyze.py shot` 的双假设报告直接判定） |
| **BB-U01** | 窗口初始尺寸/位置、`WinLinez.INI` 全部键与默认值 |

### P2 · 输入（决定玩家怎么操作）

| ID | 问题 |
|---|---|
| **BB-K03 / K05** | **点两下**还是**按住拖拽**？（当前实现是点两下） |
| **BB-K04 / K06 / K07 / K08** | 双击、悬停高亮、右键/中键、移出窗口后的状态复位 |
| **BB-K01 / K02** | 完整「按键 → 功能」映射；焦点在对话框里时是否仍生效 |
| **BB-M03 / M05 / M06** | 每个菜单项点击后的确切行为；菜单与快捷键等价性；进行中 vs 结束后菜单可用性差异 |
| **BB-K09 / K10** | 拖动窗口边框时坐标映射是否随客户区变化；有没有纯键盘操控棋盘 |

### P3 · 数值与 UI

| ID | 问题 |
|---|---|
| **BB-N02** | 7 色的**确切 RGB**（我们目前用自制色板） |
| **BB-N04** | 每次补球的数量（与 BB-G06 同测） |
| **BB-N09** | 榜单**同分**时的次序（稳定排序还是插入序） |
| **BB-N10** | 改名框规则：最大长度、字符集、默认名「挑战者」、超长如何截断 |
| **BB-N11** | 补球频率是否随时间/分数变化 |
| **BB-U07** | 菜单项 513「显示统计」到底显示什么（**我们目前只切开关、什么都没画**） |
| **BB-U08 / U09 / U12** | 三个对话框的控件布局、工具提示、是否跟随 `GetSysColor` |
| — | **反作弊弹窗**的原文与"是/否"的后果（见 §1.3 第 3 条） |

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

REM ── 2. 沙箱里跑原作，运行时快照（一次关掉 6+ 项）
start WinLinez.EXE
winprobe.exe --out C:\bb\run1 --class WinLinezWindowClass
python tools\blackbox\analyze.py probe C:\bb\run1 --out C:\bb\run1

REM ── 3. 几何（100% 缩放，5 组尺寸，每组截客户区 PNG）
python tools\blackbox\analyze.py shot C:\bb\shots\board_1280x960.png --out C:\bb\run1
REM    看 "the centred hypothesis fits" 与 "the legacy hypothesis fits" 哪个 OK

REM ── 4. 规则用例（AHK 自动摆盘 + inputlog.py 录制，F8 打 MARKER）
python tools\blackbox\inputlog.py --out C:\bb\captures\s1.csv --seconds 600
REM    a) 消除前后连拍 30 帧 → 补球数量与落点
REM    b) 3/4/5/6 连各一局
REM    c) L/U/回字形路径
REM    d) 「谁都不操作」录 3 分钟 → CPU 有没有

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
| 差分门报 "rust-implementation not built" | Windows 下二进制是 `.exe` | `diff_replay.py` 已兼容；若仍失败请手动确认 `rust\target\release\rowline-replay.exe` 存在 |
| 截图量出来的格子数和预期差 1~2 px | 截图被缩放 / 带了标题栏 / 系统 DPI ≠ 100% | 重截：客户区、100% 缩放、PNG 无损 |
| 录制脚本卡住不动 | 反作弊弹窗抢焦点 | AHK 里加对该窗口的自动关闭 |
| `geometry.py` 报 `coverage` 偏低 | 截图含窗口边框或被裁切 | 重新裁到客户区 |

---

## 8. 优先级建议（如果时间有限）

先做这 **5 件事**，即可覆盖当前实现里**全部高风险的猜测**：

1. **谁都不操作录 3 分钟** → 判定有没有 AI 对手（决定整个模型是否要重做）
2. **一次消除前后逐帧截图** → 补球规则（`refill`，当前几乎肯定错）
3. **3/4/5/6 连各一次** → 阈值与连线形状
4. **点击 vs 拖拽各一次** → 输入模型
5. **100% 缩放 + 5 组尺寸的客户区截图** → 布局公式与格子圆角/边框

---

## 9. 未在本机完成、需要 Windows 侧补的收尾

- [ ] Windows 上 `cargo test --release --workspace --all-features` 全绿
- [ ] Windows 上 `make PYTHON=python CC=gcc test` → `RESULT: ALL MATCH`
- [ ] Windows 上 `make shot` 出图正常（中文不显示方框 → 说明系统 CJK 字体被正确找到）
- [ ] `rowline-shot` 在 Windows 上跑通（字体查找顺序：**CJK 优先** —— macOS `PingFang` / Linux `Noto Sans CJK` / Windows `msyh`(微软雅黑) → 拉丁回退 `Arial`）
- [ ] 原作与重构版并排对拍记录
- [ ] 三档 DPI 的截图留档
