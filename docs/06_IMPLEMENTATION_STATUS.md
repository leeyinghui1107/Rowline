# 06 · 实施状态

> 对照 `docs/04_FIDELITY_MATRIX.md` 的 M1–M6 里程碑。
> 最近一次全量验证：**Rust 165 项测试通过 · 零编译警告 · clippy `-D warnings --all-targets` 通过 · 四方差分门 ALL MATCH · 布局门 PASSED · 200% DPI 实机点击/换选/移动/动画通过**。

---

## 1. 交付清单

### 1.1 静态分析层

| 项 | 状态 | 位置 |
|---|---|---|
| PE 结构解析（头/节/熵/导入/资源/overlay） | ✅ | `tools/pe_triage.py` |
| 资源无损导出（15 个叶子） | ✅ | `tools/res_dump.py` |
| PE 内 DIB 解码 → PNG（BI_RGB / RLE8 / RLE4） | ✅ | `tools/bmp2png.py` |
| **RT_MENU 精确解析器**（Borland 变体，276/276 字节 + 加速键交叉校验） | ✅ | `tools/menu_parser.py` |
| **命令分派提取器**（区分"比 msg"与"比 LOWORD(wParam)"） | ✅ | `tools/cmd_dispatch.py` |
| 带 IAT/字符串标注的反汇编器 | ✅ | `tools/disasm.py` |
| 全局变量读写图（31 个全局量 × 所属函数） | ✅ | `tools/global_xref.py` |
| 函数结构摘要 | ✅ | `tools/func_summary.py` |
| 精灵图集逐格分类 | ✅ | `tools/sprite_table.py` |
| Ghidra headless 自动化（40 函数 + 31 全局 + 40 字符串 + 伪码导出） | ✅ | `tools/ghidra_post.py` |

### 1.2 黑盒采集层

| 项 | 状态 | 位置 |
|---|---|---|
| Windows 运行时采集器（窗口树 / **运行时菜单树** / INI / 注册表） | ✅ | `tools/blackbox/winprobe.c` |
| 截图几何测量（棋盘原点 / 格边长 / 外框 / 网格边数） | ✅ | `tools/blackbox/geometry.py` |
| 一致性比对报告 | ✅ | `tools/blackbox/analyze.py` |
| 模拟窗口图生成（回归夹具） | ✅ | `tools/blackbox/simulate.py` |
| 键鼠录制（Python + AutoHotkey 两版） | ✅ | `tools/blackbox/inputlog.py` / `.ahk` |
| 54 项采集清单管理 + 一致性自检 | ✅ | `tools/blackbox/checklist.py` |
| 显示环境探针 | ✅ | `rust/crates/tools/src/bin/rowline-dpi-probe.rs` |
| **离线截图工具**（无 GPU 也能出图，便于视觉复核与 CI 比对） | ✅ | `rust/crates/tools/src/bin/rowline-shot.rs` |

### 1.3 Rust 工程

| Crate | 职责 | 依赖 | 测试 |
|---|---|---|---|
| `core` | 棋盘 · 规则 · RNG · 布局 · 引擎 · 输入 · 渲染指令 · 视图 · 状态 · 存档 · 菜单 · 特效 | **零依赖** | 68 |
| `display` | 视口计算 · Lanczos-3 重采样 · 变体选择 | core | 11 |
| `theme` | Classic/Dark/Light 色板 · 8pt 栅格 · 字阶 · 缓动 · 减少动效 · WCAG 对比度断言 | core | 27 |
| `storage` | RON 存档 · 文件存储 · 原作榜单导入 | core · platform · ron | 6 |
| `platform` | 窗口事件 · 时钟 · 文件系统抽象 | core | 1 |
| `render` | Renderer trait · wgpu 后端 · softbuffer 后端 · 动态字形图集 | core · assets | 10 |
| `assets` | 精灵图集 · 图标条 · 变体命名 · 系统字体解析 | core · display · image | 4 |
| `app` | 装配层：事件循环 · 命令路由 · HUD · 菜单 · 移动/选中动画 · 角色 · 名字输入 · 特效 · 窗口几何 | 全部 | 36 |
| `tools` | `rowline-replay` · `rowline-layout-check` · `rowline-dpi-probe` · `rowline-import` · `rowline-shot` | core · display · storage · app · render | — |

### 1.4 视觉与资产

| 项 | 状态 | 位置 |
|---|---|---|
| **Classic 主题（默认）**：#F0F0F0 UI / 黑棋盘 / #C0C0C0 立体格 / 原作 7 色 / 方角控件 | ✅ | `theme::Palette::CLASSIC` |
| **移动动画**：沿 4 邻域路径逐格滑动（60 ms/格，短移动最少 180 ms），动画期间 `Poll` 驱动，路径高亮 | ✅ | `Game::move_anim` · `move_animation_commands` |
| **名字输入**：白色 Win95 编辑框、可见文本、IME（中文）提交、`确定(O)` 可点击 | ✅ | `app::game::name_entry_overlay` · `overlay_click` |
| **选中动画**：球上跳 + 外扩圆环；点击另一球切换选中 | ✅ | `Game::select_anim` · `click_at` |
| **中文输入**：组合期忽略拼音按键，提交时用汉字替换 | ✅ | `Game::ime_start` + `Ime::Preedit` |
| **排行榜**：按钮居中不越界；确定后开始新局 | ✅ | `scoreboard_buttons` · `start_new_game` |
| **边框**：窗口与棋盘的 Win95 凸起边框（软件/GPU 一致） | ✅ | `client_frame_commands` · `core::view` |
| **自制卡通角色**：圆身、表情、眨眼、待机浮动、胜利跳、权杖/剑；位置与标签按原版测量 | ✅ | `app::game::one_figure` |
| **原版/复刻对比截图工具** | ✅ | `tools/win/compare_shots.ps1` |
| **总榜列对齐**：名次/姓名左列固定，分数右对齐 | ✅ | `app::game::scoreboard_overlay` |
| **左侧记录分 + 破纪录动画**（国王跳跃、金边闪烁 0.9 s） | ✅ | `Game::king_score` · `record_anim` |
| **角色高度编码分数**：国王=记录、挑战者=本局分，默认左高右低；挑战者随分数升高并在 150% 处封顶；新纪录后国王升至挑战者高度、挑战者回落（同时动画） | ✅ | `Game::{king_rise, challenger_rise, update_figure_targets}` |
| **棋盘下边框**：格阴影 + 2 px 黑覆盖窗口浮雕边 | ✅ | `board_bottom_edge` |
| **对比度**：次要文字 `#606060`（AA）、隔行 `#E4E4E4`、角色加深 + 圆角描边 | ✅ | `theme::Palette::CLASSIC` |
| **发布版无控制台窗口**（GUI 子系统；诊断不 panic） | ✅ | `app::main` · `render::canvas` |
| **默认窗口**：604×396 客户区（372 内容 + 20 菜单 + 4 底部边框预留）；屏幕居中；几何随存档恢复 | ✅ | `core::save` · `core::layout` · `app::main` |
| **HiDPI 输入**：物理坐标经 `Viewport` 反算为逻辑坐标（含 letterbox） | ✅ | `app::main::GpuApp::to_game` |
| 设计令牌（色板 / 栅格 / 字阶 / 动效 / 无障碍） | ✅ | `rust/crates/theme/` |
| SDF 形状管线（圆角 / 渐变 / 投影 / 辉光） | ✅ | `rust/crates/render/src/wgpu_backend.rs` |
| 双管线 + 绘制列表（保 z 序） | ✅ | 同上 |
| 动态字形图集（按 DPR 栅格化） | ✅ | `rust/crates/render/src/font.rs` |
| 菜单系统（数据模型 + 绘制 + 键盘/鼠标） | ✅ | `core/src/menu.rs` + `app/src/game.rs` |
| 粒子特效（纯装饰） | ✅ | `core/src/effects.rs` |
| 图标集（13 个线条图标 + SVG 源 + @1x/@2x/@3x/@4x） | ✅ | `tools/make_icons.py` |
| 噪点纹理（256² 可平铺） | ✅ | `tools/make_icons.py` |
| 精灵图集 @1x/@2x/@3x/@4x（真高分辨率重绘） | ✅ | `tools/make_sheet.py` |
| 命令参考浮层（替代原作的 .hlp） | ✅ | `app/src/game.rs` |
| **16 行榜单模型**（`- 空的 -` 占位、`N. 名字 分数` 格式） | ✅ | `rust/crates/core/src/scoreboard.rs` |
| **双分数模型**（历史记录分 / 本局分，非 AI 对手） | ✅ | `core::scoreboard::Players` |
| **高分榜窗口**（总榜 / 清除 / 确定） | ✅ | `app/src/game.rs` |
| **名称输入窗口**（预填「挑战者」） | ✅ | `app/src/game.rs` |
| **统计窗口**（空格子 + 7 色：数量/百分比/`消除-N`，默认关闭） | ✅ | `core::engine` + `app::game::stats_panel` |
| **消除动画**（2 × 50 ms 溶解 + 420 ms 胜利立绘） | ✅ | `app::game::{clear_animation_commands, win_lift}` |
| **棋盘悬停环与最短路预览**（重设计） | ✅ | `Game::update_hover` + `hover_commands` |
| **关于对话框** | ✅ | `app/src/game.rs` |
| **无路可走判定** | ✅ | `core::rules::has_legal_move()` |

### 1.5 工程化

| 项 | 状态 | 位置 |
|---|---|---|
| Cargo workspace（9 crate） | ✅ | `rust/` |
| Makefile（replay / check / rust / app / sheet / icons / test / package / dist / dist-ci） | ✅ | `Makefile` |
| **Windows 工具链包装器**：自动定位真实 Python（跳过应用商店占位符 9009）与自定义 CARGO_HOME 下的 cargo | ✅ | `tools/win/py.ps1` · `tools/win/cargo.ps1` |
| **发布体积**：Windows 单文件 **2.84 MB**（zip 1.53 MB）；`opt-level="z"` + fat LTO + strip + 按平台裁剪 wgpu 后端 | ✅ | `rust/Cargo.toml` · `crates/*/Cargo.toml` · `tools/package.py` |
| **可选 Vulkan 回退**（无 DX12 的虚拟机/老显卡） | ✅ | `cargo build --features vulkan` |
| CMakeLists（C 对照实现） | ✅ | `reference/c-impl/CMakeLists.txt` |
| GitHub Actions（rust / C / 差分门 / 资产可复现） | ✅ | `.github/workflows/ci.yml` |
| **素材可复现检查**：按解码像素比对（避免 zlib 版本差异误报）；生成器数值量化到 1e-6 | ✅ | `tools/check_assets.py` · `tools/make_icons.py` |
| 打包脚本（平台 bundle + README + zip） | ✅ | `tools/package.py` |
| **三平台发布构建**（宿主本机 + GitHub Actions；macOS 通用二进制；zip + SHA256） | ✅ | `tools/build_all.py` · `.github/workflows/release.yml` |
| 零编译警告 | ✅ | `cargo build --workspace --all-features` |
| **Windows 工具链自检**（cargo build/test + make test/shot + winprobe + 黑盒管线 + DPI 探针，证据归档） | ✅ 9/9 | `tools/win/verify.ps1` · `blackbox/windows_verify/20261021-192704/` |

---

## 2. 里程碑状态

| 里程碑 | 状态 | 说明 |
|---|---|---|
| **M0 空窗口** | ✅ | winit + wgpu surface，HiDPI 正确 |
| **M1 静态帧** | ✅ | 布局门 6 种尺寸全过；精灵索引公式与原作一致 |
| **M2 输入** | ✅ | 点击选球 → 落子 → 消除；菜单优先于棋盘；物理→逻辑坐标换算（HiDPI） |
| **M3 核心玩法** | ✅ | 规则语义已由反汇编 + 实机定标；四方差分门 `ALL MATCH` |
| **M4 持久化** | ✅ | RON 存档 + 设置 + 高分榜 + 窗口几何；默认 feature 启用 `fs`；原作注册表导入器（`rowline-import`） |
| **M5 表现** | ✅ | **Classic 默认主题**（Win95 配色/立体边/顶部 HUD/自绘菜单）· 深/浅可选 · 粒子 · 动画 · @1x–@4x · 减少动效 |
| **截图对齐** | ✅ | 按 `docs/11` 修正：HUD 顶部横栏 · 下一组 3 颗球 · 黑底浅灰立体格 · 结束对话框 · 默认 604×396 客户区 |
| **M6 交付** | ✅ | CI 三平台 + 资产可复现校验 + 打包脚本 + 三平台发布工作流 |
| **发布** | ⬜ | 需商业授权（`REPORT.md` §5 R6） |

> **截图对齐（`docs/11`）已完成**：下一组 3 颗球、HUD 顶部横栏、棋盘黑底浅灰格、
> 游戏结束对话框均已实现并有回归测试。
>
> **M3 已闭环**：规则语义（直线清除、4 邻域移动、阈值 5、消线不补球、
> 普通回合补 3 球、满盘才结束、非线性分值）由 `0x401BD4` / `0x401CD4` /
> `0x401CA1` / `0x401E1D` / `0x401AF8` 的反汇编与实机观察共同定标，
> 三份实现与 golden 四方一致。

---

## 3. 规则模型（已定标）

| 开关 / 语义 | 当前值 | 依据 |
|---|---|---|
| `threshold` | 5 | `0x401AF8` 硬编码比较 |
| 清除形状 | 过落点的整条直线（横 / 竖 / 两斜） | `0x401BD4` + `0x40A000` 四轴表 |
| 移动 | 4 邻域寻路，源格清空 | `0x4016xx` 步表 |
| `spawn_per_move` | 3（**仅未消线的回合**） | `0x401CD4` ×3，消线分支不调用 |
| 消线计分 | 5→10，6→12，7→18，8→28，9→42 | `0x401E1D` |
| 消线后补球 | **不补** | `0x401BD4` 分支无 spawn |
| 游戏结束 | 棋盘无空位（`0x401CA1` 计数为 0）才结束 | `0x401CA1` + `游戏结束!` |
| `initial_balls` | **5** | `0x402801` 新局 |
| 每回合补球数 | 3 | `0x401CD4` |

三处实现必须**同时**修改：

| 实现 | 位置 |
|---|---|
| Python | `tools/replay/model.py` → `RuleModel` |
| C | `reference/c-impl/wl_model.h` → `wl_rule_model` |
| Rust | `rust/crates/core/src/rules.rs` → `RuleModel` |

---

## 4. 尚未实现 / 已知限制

| 项 | 影响 | 计划 |
|---|---|---|
| ~~统计窗口（分数明细）~~ | ✅ 已实机确认并实现：8 行、`count (percent%)`、`,消除-N` | docs/11 §5e |
| 角色像的**原作外观** | 用自制"纹章 + 底座"代替（红冕章 / 品红剑章） | 原作角色像素属受版权保护的美术，**不得复用**（REPORT.md R2）；如需更接近的造型，须由美术重新创作 |
| ~~棋盘原点的常量语义~~ | ✅ 已确认 `0x40B060/64` 为棋盘原点，`0x40B078/7C` 为预览球组 | docs/11 §5d |
| ~~消除动画的多步时序~~ | ✅ 已由 `clear5` 连拍 + `0x401C48` 确认：2 × 50 ms 溶解 + 420 ms 胜利立绘 | docs/11 §5f |
| 音效 | 原作无音频；若新增属功能扩展 | 需单独立项与授权评估 |
| i18n | 界面固定为**简体中文**（与原作一致） | 文案集中在 `core::menu` 与 `app::commands`；如需英文版须另做一套并处理字体回退 |
| ~~浅色主题的棋盘底纹~~ | ✅ 已加 `board_backdrop` / `board_cell`（暖纸底 + 米色格子） | `theme::Palette` |
| ~~`Fixed` 策略未接入 UI 菜单~~ | ✅ 已加 `固定分辨率`（id 515，与 `Ctrl+R` 等价） | `core::menu` + `app` |
| ~~素材只到 @3x~~ | ✅ 已补 @4x（精灵表 PNG + 图标） | `assets/sheet` / `assets/icons` |
| ~~CPU 是否真的会自动走子~~ | ✅ **不会**：单人游戏，空闲 480 s 画面不变 | `BB-N03` 已闭环 |
| 字体依赖系统 | 缺少 CJK 字体时中文会显示为方框 | 已按"优先 CJK、后拉丁"的顺序查找系统字体；如需自带字体须评估授权与体积 |
| Classic 的 UI 灰 | 取自 `COLOR_BTNFACE`：Win10/11 为 `#F0F0F0`，旧版 Windows 为 `#C0C0C0` | 与原作调用 `GetSysColor` 的行为一致，不做硬编码 |

---

## 5. 合规状态

| 检查项 | 状态 |
|---|---|
| 未包含原作位图 / Logo / 图标 / 名称 | ✅ `assets/` 全部由本仓库脚本生成 |
| 未把反编译伪代码写入产品 | ✅ 伪码仅导出到 `analysis/pseudo/` |
| 代码为独立重写 | ✅ `reference/c-impl/` 与 `rust/` 下无任何反编译产物，行为依据为 `docs/03` |
| 反作弊 XOR 掩码未复刻 | ✅ 按 `docs/04` P-04 移除 |
| 内部文档已署名原作作者与版权 | ✅ `README.md`、`REPORT.md` §5 |
| 商业发布前需授权 | ⬜ 未进行（当前仅为本地研究） |
