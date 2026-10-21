# 06 · 实施状态

> 对照 `docs/04_FIDELITY_MATRIX.md` 的 M1–M6 里程碑。
> 最近一次全量验证：**Rust 126 项测试通过 · 零编译警告 · clippy `-D warnings` 通过 · 四方差分门 ALL MATCH · 布局门 PASSED**。

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
| `core` | 棋盘 · 规则 · RNG · 布局 · 引擎 · 输入 · 渲染指令 · 视图 · 状态 · 存档 · 菜单 · 特效 | **零依赖** | 40 |
| `display` | 视口计算 · Lanczos-3 重采样 · 变体选择 | core | 11 |
| `theme` | 深/浅色板 · 8pt 栅格 · 字阶 · 缓动 · 减少动效 · WCAG 对比度断言 | core | 25 |
| `storage` | RON 存档 · 文件存储 · 原作榜单导入 | core · platform · ron | 6 |
| `platform` | 窗口事件 · 时钟 · 文件系统抽象 | core | 1 |
| `render` | Renderer trait · wgpu 后端 · softbuffer 后端 · 动态字形图集 | core · assets | 4 |
| `assets` | 精灵图集 · 图标条 · 变体命名 · 系统字体解析 | core · display · image | 4 |
| `app` | 装配层：事件循环 · 命令路由 · HUD · 菜单 · 特效 | 全部 | 6 |
| `tools` | `rowline-replay` · `rowline-layout-check` · `rowline-dpi-probe` · `rowline-import` · `rowline-shot` | core · display · storage · app · render | — |

### 1.4 视觉与资产

| 项 | 状态 | 位置 |
|---|---|---|
| 设计令牌（色板 / 栅格 / 字阶 / 动效 / 无障碍） | ✅ | `rust/crates/theme/` |
| SDF 形状管线（圆角 / 渐变 / 投影 / 辉光） | ✅ | `rust/crates/render/src/wgpu_backend.rs` |
| 双管线 + 绘制列表（保 z 序） | ✅ | 同上 |
| 动态字形图集（按 DPR 栅格化） | ✅ | `rust/crates/render/src/font.rs` |
| 菜单系统（数据模型 + 绘制 + 键盘/鼠标） | ✅ | `core/src/menu.rs` + `app/src/game.rs` |
| 粒子特效（纯装饰） | ✅ | `core/src/effects.rs` |
| 图标集（11 个线条图标 + SVG 源 + @1x/@2x/@3x） | ✅ | `tools/make_icons.py` |
| 噪点纹理（256² 可平铺） | ✅ | `tools/make_icons.py` |
| 精灵图集 @1x/@2x/@3x（真高分辨率重绘） | ✅ | `tools/make_sheet.py` |
| 命令参考浮层（替代原作的 .hlp） | ✅ | `app/src/game.rs` |
| **16 行榜单模型**（`- 空的 -` 占位、`N. 名字 分数` 格式） | ✅ | `rust/crates/core/src/scoreboard.rs` |
| **人机对战模型**（国王 / 挑战者 两个分数） | ✅ | `core::scoreboard::Players` |
| **高分榜窗口**（总榜 / 清除 / 确定） | ✅ | `app/src/game.rs` |
| **名称输入窗口**（预填「挑战者」） | ✅ | `app/src/game.rs` |
| **关于对话框** | ✅ | `app/src/game.rs` |
| **无路可走判定** | ✅ | `core::rules::has_legal_move()` |

### 1.5 工程化

| 项 | 状态 | 位置 |
|---|---|---|
| Cargo workspace（9 crate） | ✅ | `rust/` |
| Makefile（replay / check / rust / app / sheet / icons / test / package） | ✅ | `Makefile` |
| CMakeLists（C 对照实现） | ✅ | `reference/c-impl/CMakeLists.txt` |
| GitHub Actions（rust / C / 差分门 / 资产可复现） | ✅ | `.github/workflows/ci.yml` |
| 打包脚本（平台 bundle + README） | ✅ | `tools/package.py` |
| 零编译警告 | ✅ | `cargo build --workspace --all-features` |
| **Windows 工具链自检**（cargo build/test + make test/shot + winprobe + 黑盒管线，证据归档） | ✅ 8/8 | `tools/win/verify.ps1` · `blackbox/windows_verify/20261021-150638/` |

---

## 2. 里程碑状态

| 里程碑 | 状态 | 说明 |
|---|---|---|
| **M0 空窗口** | ✅ | winit + wgpu surface，HiDPI 正确 |
| **M1 静态帧** | ✅ | 布局门 6 种尺寸全过；精灵索引公式与原作一致 |
| **M2 输入** | ✅ | 点击选球 → 落子 → 消除；菜单优先于棋盘 |
| **M3 核心玩法** | 🟡 | 框架与四方差分门就绪；**规则语义待黑盒定标** |
| **M4 持久化** | ✅ | RON 存档 + 设置 + 高分榜 + 原作注册表导入器（`rowline-import`） |
| **M5 表现** | ✅ | 深/浅主题 · 菜单 · HUD · 粒子 · 动画 · 图标 · 噪点 · 减少动效 · 可缩放字体 |
| **截图对齐** | ✅ | 按 `docs/11` 修正：HUD 改顶部横栏 · 下一组 3 颗球 · 棋盘黑底浅灰格 · 游戏结束对话框 |
| **M6 交付** | ✅ | CI 三平台 + 资产可复现校验 + 打包脚本 |
| **发布** | ⬜ | 需商业授权（`REPORT.md` §5 R6） |

> **截图对齐（`docs/11`）已完成**：下一组 3 颗球、HUD 顶部横栏、棋盘黑底浅灰格、
> 游戏结束对话框均已实现并有回归测试。
>
> **M3 是唯一未闭环的部分**，而它**不是代码问题**：
> 规则的精确语义（补球数、拐弯上限、分值公式、初始布局）取决于黑盒采集
> （`docs/01` 的 `BB-G01..G15`）。规则模型的所有开关已集中在
> `RuleModel`（三处实现各一份），定标后改三个文件即可，
> 差分门会立刻验证三方仍一致。

---

## 3. 规则模型的待定开关

| 开关 | 默认 | 对应采集项 | 当前依据 |
|---|---|---|---|
| `threshold` | 5 | `BB-G03` | ✅ 静态确认（`0x40B07C` + 比较分支） |
| `source_keeps_colour` | true | `BB-G03` | 🟡 推断（Lines 系惯例） |
| `max_turns` | -1（不限） | `BB-G04` | ⬜ 未定 |
| `refill` | 0 | `BB-G02/G06` | ⬜ 未定 |
| `points_per_ball` | 1 | `BB-G11` | ⬜ 未定 |
| `min_score_delta` | 0 | `BB-G11` | ⬜ 未定 |
| `allow_revisit_source` | true | `BB-G05` | ⬜ 未定 |
| `initial_balls` | **5** | `BB-G01` | ✅ 截图证实 |

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
| 统计窗口（分数明细） | 菜单项 513 只切换显示 | 需要再定其内容 |
| 角色像的**原作外观** | 用自制"纹章 + 底座"代替（红冕章 / 品红剑章） | 原作角色像素属受版权保护的美术，**不得复用**（REPORT.md R2）；如需更接近的造型，须由美术重新创作 |
| 棋盘原点的常量语义 | 已按截图改为**居中** | 二进制里的 `[0x40B078]=(right-0x6C)/2` 用途未识别，见 docs/11 §5d |
| 消除动画的多步时序 | 当前是瞬时消除 + 粒子 | 依赖 `BB-G10` 的时序数据 |
| 音效 | 原作无音频；若新增属功能扩展 | 需单独立项与授权评估 |
| i18n | 界面固定为**简体中文**（与原作一致） | 文案集中在 `core::menu` 与 `app::commands`；如需英文版须另做一套并处理字体回退 |
| 浅色主题的棋盘底纹单独出图 | 目前复用 `surface`（白底 + 灰格子） | 可加 `@light` 变体 |
| `Fixed` 策略未接入 UI 菜单 | 已有 `Ctrl+R` 快捷键 | 补一个菜单项 |
| 素材只到 @3x | 500% 缩放回退 Lanczos 放大 | 美术出图时补 @4x |
| **CPU 是否真的会自动走子** | 尚未实现 AI 对手，只追踪两侧分数 | 需 `BB-N03` 录屏确认 CPU 的回合行为 |
| 字体依赖系统 | 缺少 CJK 字体时中文会显示为方框 | 已按"优先 CJK、后拉丁"的顺序查找系统字体；如需自带字体须评估授权与体积 |

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
