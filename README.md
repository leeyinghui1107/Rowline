# WinLinez 逆向工程工作区

主文档 → **[REPORT.md](REPORT.md)**（逆向可行性报告）

> **重构版产品名：Rowline**（可执行文件 `rowline`，crate 前缀 `rowline-*`）。
> 原作的名称、图标、精灵与美术**均未**进入重构版，详见 `REPORT.md` §5.4。

## 文档索引

| 文档 | 内容 |
|---|---|
| **[REPORT.md](REPORT.md)** | 逆向可行性报告：工具清单、技术栈判定、资产清单、逻辑清单、法律边界 |
| **[docs/01_BLACKBOX_PLAN.md](docs/01_BLACKBOX_PLAN.md)** | 黑盒采集方案：54 个采集项 + 5 套记录模板 + 工具化说明 |
| **[docs/02_WHITEBOX_METHOD.md](docs/02_WHITEBOX_METHOD.md)** | 白盒分析要点：锚点定位、符号自动化、行为伪码规范、消息/命令分表重建 |
| **[docs/03_FEATURE_SPEC.md](docs/03_FEATURE_SPEC.md)** | **功能规格说明书**：S1–S12 十三个系统的输入→处理→输出、状态机、边界、常量表 |
| **[docs/04_FIDELITY_MATRIX.md](docs/04_FIDELITY_MATRIX.md)** | 复刻/重设计决策矩阵：50 条逐项判定 + 风险表 + 验收门 |
| **[docs/05_ART_SPEC.md](docs/05_ART_SPEC.md)** | 美术规格书：精灵索引契约、多倍率变体、色彩与形状通道 |
| **[docs/06_IMPLEMENTATION_STATUS.md](docs/06_IMPLEMENTATION_STATUS.md)** | **实施状态**：交付清单、M0–M6 里程碑、待定开关、限制、合规 |
| **[docs/07_RUST_ARCHITECTURE.md](docs/07_RUST_ARCHITECTURE.md)** | Rust 逻辑层架构（已被 08 取代，保留作演进记录） |
| **[docs/08_RUST_ENGINE_DESIGN.md](docs/08_RUST_ENGINE_DESIGN.md)** | **跨平台 Rust 工程设计**：技术选型、crate 划分、trait/数据结构、feature 矩阵、路线图 |
| **[docs/09_DISPLAY_DPI_ADAPTATION.md](docs/09_DISPLAY_DPI_ADAPTATION.md)** | **显示适配**：双坐标系、letterbox/整数倍、多倍率素材、DPR 栅格化字体、多屏异 DPI、测试清单 |
| **[docs/10_VISUAL_DESIGN.md](docs/10_VISUAL_DESIGN.md)** | **视觉设计规范**：深/浅色板、8pt 栅格、字阶、缓动与减少动效、资产规范、SDF 落地、无障碍 |
| **[docs/11_SCREENSHOT_OBSERVATIONS.md](docs/11_SCREENSHOT_OBSERVATIONS.md)** | **原版实机截图观察**：从截图直接读到的事实（下一组 = 3 颗球、HUD 顶部横栏、黑底棋盘、游戏结束对话框）与实现修改清单 |
| **[tools/blackbox/README.md](tools/blackbox/README.md)** | **黑盒采集操作手册**：Windows VM 上的完整执行流程 |
| **[docs/temp/WINDOWS_DEV_CHECKLIST.md](docs/temp/WINDOWS_DEV_CHECKLIST.md)** | **Windows 开发机工作清单**：待确认的问题（规则/AI 对手/几何/输入/数值）、环境与坑、执行顺序、每项确认后的固定动作；**工具链自检已完成 9/9**，黑盒采集待原作 |

## 工程结构

```
02_winlinez/
├── REPORT.md                  逆向可行性报告
├── docs/                     12 份设计与规格文档
│   └── temp/                 Windows 开发机工作清单（工具链自检 9/9；黑盒采集待原作）
├── reference/c-impl/         C 对照实现（差分门的第三方，非产品代码）
├── rust/                     Rust 工程（9 个 crate）
│   ├── crates/core/           纯逻辑：零依赖
│   ├── crates/display/        视口与重采样
│   ├── crates/theme/          设计令牌
│   ├── crates/storage/        存档与导入
│   ├── crates/platform/       平台抽象
│   ├── crates/render/         渲染后端
│   ├── crates/assets/         资源加载
│   ├── crates/app/            装配层
│   └── crates/tools/          CLI
├── tools/                    静态分析 · 黑盒采集 · 资产生成 · 打包
├── assets/
│   ├── sheet/                自制精灵图集 @1x/@2x/@3x
│   ├── icons/                自制图标 @1x/@2x/@3x + SVG 源
│   ├── noise/                自制噪点纹理
│   ├── png/ resources/       ⚠️ 从原作 EXE 提取的受版权保护资产，已 .gitignore 隔离
├── analysis/                 静态分析产物
├── blackbox/                 采集清单与证据
└── .github/workflows/ci.yml  CI
```

## 快速开始

```bash
make            # C 对照实现：回放 CLI + 布局门
make rust       # Rust workspace
make test       # 布局门 + 四方差分门（Python / C / Rust / golden）
make app        # SDL2 应用（需要 SDL2）
make sheet      # 重新生成精灵图集
make icons      # 重新生成图标与噪点
make package    # 打包当前平台
```

离线出图（不需要 GPU / 显示器，用于视觉复核）：

```bash
cargo build --release -p rowline-tools
./rust/target/release/rowline-shot --out shot.png --width 1280 --height 900
./rust/target/release/rowline-shot --out light.png --theme light --scenario scoreboard
```

Rust 侧：

```bash
cd rust
cargo build --release --workspace --all-features
cargo test --release --workspace --all-features     # 127 项（含 doc-test）
cargo clippy --workspace --all-features -- -D warnings
```

黑盒阶段（Windows VM）：

```bat
winprobe.exe --out bb\run1 --class WinLinezWindowClass
python tools\blackbox\analyze.py probe bb\run1 --out bb\run1
python tools\blackbox\analyze.py shot bb\shots\board.png --out bb\run1
python tools\blackbox\checklist.py summary
```

Windows 工具链一键自检（9 项：cargo build/test、make test/shot、winprobe、黑盒管线、DPI 探针）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools\win\verify.ps1
```

## 关键结论速览

- **原生 Win32 C/C++**（Borland/Turbo 链接器 2.18），32 位 x86，**无壳、无混淆、无 .NET、无游戏引擎**
- **99.2% 体积是 PE 资源段**，熵仅 3.10 → 资产**未压缩未加密，已 100% 提取**
- 棋盘（已确认）：**9×9 = 81 格**，行主序字节数组，格字节低 6 位有效，**7 色**，**消除阈值 = 5**
- 新局（已由截图确认）：空棋盘上**随机放 5 颗球**，并预告**下一组 3 颗**
- 精灵取样（已确认）：图集 576×586、36 px 网格，源 Y=225，源 X = 格值×36 + 精灵集×288
- 布局（已按截图校正）：棋盘**水平居中**、**垂直居中于 HUD 之下**；右角色 X 用二进制原公式；
  最小客户区 880×892（`[0x40B078]=(W−108)/2` 的确切用途仍未识别，见 `docs/11` §5d）
- 菜单（已精确解析 276/276 字节 + 加速键交叉校验）：**简体中文**，`文件(&F)` / `选项(&O)`
- 命令 ID：256 新建(F4) / 257 高分榜 / 258 退出(Alt+F9) / 512 下一组(F3→ShowNext) / 513 显示统计(F5) / 514 使用形状(F6→UseFigures) / 768 内容(F1) / 769 关于
- 高分榜（已确认）：注册表 `HKCU\...\HighScore`，`Table`=512 B（16×32 B，姓名+28 B / 分数+0x1C），`Hash`=16 B
- 随机源（已确认）：Borland LCG `state*0x41C64E6D+0x3039`，以 `GetTickCount()` 播种
- **无音频**；`winlines.res`/`lines.res` 为**死字符串**（全文件零引用）

## 版权与合规提醒

原作 WinLinez v1.30 © 1998-1999 **Ivan Golubev**（http://www.golubev.com），发行方 GMSoft。
`assets/png/`、`assets/resources/` 是从原作 EXE 提取的受版权保护资产，**仅供研究参考，不得分发**，
已由 `.gitignore` 隔离。重构版使用**本仓库自行生成**的素材（精灵图集 / 图标 / 噪点）与**自有名称**（Rowline），
不得包含原作的任何美术、代码或数据。详见 `REPORT.md` §5 与 `docs/04`。
