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
| **[docs/temp/WINDOWS_DEV_CHECKLIST.md](docs/temp/WINDOWS_DEV_CHECKLIST.md)** | **Windows 开发机工作清单**：规则/几何/输入/数值的确认记录、环境与坑、执行顺序；**工具链自检 9/9**，黑盒项已按反汇编 + 实机回填 |

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
│   ├── sheet/                自制精灵图集 @1x/@2x/@3x/@4x
│   ├── icons/                自制图标 @1x/@2x/@3x/@4x + SVG 源
│   ├── noise/                自制噪点纹理（仅设计稿，运行时不加载）
│   ├── png/ resources/       ⚠️ 从原作 EXE 提取的受版权保护资产，已 .gitignore 隔离
│   （sheet/icons 的 PNG 在编译时用 include_bytes! 编入可执行文件，发布只有一个 exe）
├── analysis/                 静态分析产物
├── blackbox/                 采集清单与证据
└── .github/workflows/         ci.yml（测试矩阵） · release.yml（三平台发布构建）
```

## 快速开始

```bash
make            # C 对照实现：回放 CLI + 布局门
make rust       # Rust workspace
make test       # 布局门 + 四方差分门（Python / C / Rust / golden）
make app        # SDL2 应用（需要 SDL2）
make sheet      # 重新生成精灵图集
make icons      # 重新生成图标与噪点
make package    # 编译 release 并打包当前平台（macOS: Rowline.app；其它: 可执行文件 + zip）
make cross      # macOS 本机交叉编译三平台单文件（macOS / Windows / Linux）
make dist       # 同上；检测到 gh 时再构建其余平台
make dist-ci    # 三平台全部由 GitHub Actions 构建并下载到 dist/
```

> **macOS 本机三平台构建**：`make cross` 不需要 Linux 机器或 Docker。
> 它用 Apple 工具链产出 macOS universal，用 mingw-w64 产出 Windows `.exe`，
> 用 Rust 自带的 `rust-lld` + musl 产出**静态** Linux ELF。
> 产物：`dist/rowline-darwin/Rowline.app`、`dist/rowline-win32/rowline.exe`、
> `dist/rowline-linux/rowline`（以及各自的 zip）。
> 首次运行会自动 `rustup target add`；Windows 交叉编译需要
> `brew install mingw-w64`。

> **macOS 启动方式**：产物是 `dist/rowline-darwin/Rowline.app`。双击即可启动，
> **不会附带 Terminal 窗口**；等价命令是 `open Rowline.app`。不要直接双击
> `Contents/MacOS/rowline`，那样仍会由 Terminal 托管。Linux / Windows 则直接运行
> 裸可执行文件。
>
> `.app` 使用 ad-hoc 签名（未公证）。从网络下载 zip 后首次启动若被 Gatekeeper
> 拦截，右键 → 打开，或在“系统设置 → 隐私与安全性”中放行。

> **Windows 无需先配环境**：`make` 在 Windows 上把 Python 交给
> `tools/win/py.ps1`、把 cargo 交给 `tools/win/cargo.ps1`，两者都会自动定位
> 真实解释器/工具链（应用商店的 `python.exe` 占位符会以 9009 退出，已被跳过）。
> `make dist` 直接调用 `tools/build_all.ps1`。
>
> 直接打包也可以：
>
> ```powershell
> powershell -NoProfile -ExecutionPolicy Bypass -File tools\build_all.ps1 --mode local
> ```

> **三平台构建**：`tools/build_all.py` 会在本机构建宿主平台；当请求其它平台时，
> 它会触发 `.github/workflows/release.yml`，由 GitHub 的原生 runner 分别构建
> Linux、macOS（Intel + Apple Silicon 通用二进制）和 Windows，完成后下载到 `dist/`。
> CI 构建的是**已推送的提交**，工作区有未提交改动时脚本会拒绝运行（可用
> `--allow-dirty` 跳过检查）。`gh` 未安装时自动降级为仅构建宿主平台。
> 可先用 `python tools/build_all.py --list` 查看计划；Windows 下如果 `python`
> 是应用商店占位符，使用 `powershell -NoProfile -ExecutionPolicy Bypass -File tools\build_all.ps1 --list`。

离线出图（不需要 GPU / 显示器，用于视觉复核）：

```bash
cargo build --release -p rowline-tools
./rust/target/release/rowline-shot --out shot.png --width 1280 --height 900
./rust/target/release/rowline-shot --out scores.png --scenario scoreboard
```

Rust 侧：

```bash
cd rust
cargo build --release --workspace --all-features
cargo test --release --workspace --all-features     # 140 项（含 doc-test）
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

- **默认外观 = 原作 Classic**：`#F0F0F0` 菜单/HUD、黑色棋盘区、`#C0C0C0`
  Win95 立体格、原作精灵球；菜单为 `文件 / 帮助`，统计、帮助、关于、总榜、
  名字输入统一为带 × 与“关闭/确定”的方形模态卡片。默认客户区 **760×480**，
  屏幕居中，几何随存档恢复。只保留这一套默认主题，不提供主题切换。
  HiDPI 输入按 `Viewport` 反算为逻辑坐标（200% 实机验证）
- **角色高度 = 分数**：左右同一比例尺（记录 = 整列 2/3），挑战者随本局分数升高、
  超过记录时高于国王并在 150% 封顶；新纪录后按新比例重算，国王“降到 0 再升起”、
  挑战者同时回落（详见 docs/03 §3.1b）
- **单文件发布**：素材与中英文字体编入 exe，**约 7.2 MB**（其中 5.2 MB 是内嵌字体），
  无需 `assets/` 目录，也不依赖系统 CJK 字体；macOS 包成 `Rowline.app`，双击启动无终端窗口；
  `opt-level="z"` +
  fat LTO + strip + 按平台裁剪后端 + 自写文本存档。无 DX12 的机器（部分虚拟机/
  老显卡）可用 `cargo build --release -p rowline-app --features vulkan` 一并链接 Vulkan
- **原生 Win32 C/C++**（Borland/Turbo 链接器 2.18），32 位 x86，**无壳、无混淆、无 .NET、无游戏引擎**
- **99.2% 体积是 PE 资源段**，熵仅 3.10 → 资产**未压缩未加密，已 100% 提取**
- 棋盘（已确认）：**9×9 = 81 格**，行主序字节数组，格字节低 6 位有效，**7 色**，**消除阈值 = 5**
- 新局（已由截图确认）：空棋盘上**随机放 5 颗球**，并预告**下一组 3 颗**
- 精灵取样（已确认）：图集 576×586、36 px 网格，源 Y=225，源 X = 格值×36 + 精灵集×288
- 布局（反汇编 + 实机 604×371 确认）：棋盘 `x=(W−328)/2`、`y=(H−376)/2+46`，外框 327×328；
  左右角色 `((W−328)/2−58)/2` / `t+328+((t−58)/2)`；最小客户区 444×372
  （`[0x40B078]=(W−108)/2` 是**预览球组**的 X，见 `docs/11` §5d）
- 原作菜单（已精确解析 276/276 字节 + 加速键交叉校验）：**简体中文**，
  `文件(&F)` / `选项(&O)`；命令 ID 256 新建(F4) / 257 高分榜 / 258 退出(Alt+F9) /
  512 下一组(F3→ShowNext) / 513 显示统计(F5) / 514 使用形状(F6→UseFigures) /
  768 内容(F1) / 769 关于
- 最终菜单：`文件`（新游戏 / 高分榜 / 显示统计 / 退出）、`帮助`（帮助主题 / 关于）。
  **移除了**原作的“下一组”开关（该设置从未被读取，实际预览一直在 HUD 中）、
  “使用形状”、“固定分辨率”、重复的“内容”入口，以及`视图`菜单里的主题切换、
  动态效果和文字大小三项；帮助/关于/统计均为可关闭的模态卡片
- 统计窗口（已由实机确认）：8 行（空格子 + 7 色），数量、占比与
  `消除-N`；百分比 = `数量×100/81`（向下取整），`N` 为本局该颜色累计消除数
  （`0x401C7A`）。重设计后改为居中模态卡片，不再与棋盘重叠
- 高分榜（已确认）：注册表 `HKCU\...\HighScore`，`Table`=512 B（16×32 B，姓名+28 B / 分数+0x1C），`Hash`=16 B
- 随机源（已确认）：Borland LCG `state*0x41C64E6D+0x3039`，以 `GetTickCount()` 播种
- **无音频**；`winlines.res`/`lines.res` 为**死字符串**（全文件零引用）

## 版权与合规提醒

原作 WinLinez v1.30 © 1998-1999 **Ivan Golubev**（http://www.golubev.com），发行方 GMSoft。
`assets/png/`、`assets/resources/` 是从原作 EXE 提取的受版权保护资产，**仅供研究参考，不得分发**，
已由 `.gitignore` 隔离。重构版使用**本仓库自行生成**的素材（精灵图集 / 图标 / 噪点）与**自有名称**（Rowline），
不得包含原作的任何美术、代码或数据。详见 `REPORT.md` §5 与 `docs/04`。

内嵌 UI 字体为 **WenQuanYi Micro Hei 0.2.0-beta**（拉丁 + CJK），
以 **Apache License 2.0** 授权，见 `assets/font/NOTICE.txt` 与
`assets/font/LICENSE-APACHE-2.0.txt`。
