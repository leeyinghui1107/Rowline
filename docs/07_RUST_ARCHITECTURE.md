# 07 · Rust 架构设计

> **状态：本文档是逻辑层架构的演进记录，已被
> [`docs/08_RUST_ENGINE_DESIGN.md`](08_RUST_ENGINE_DESIGN.md)（完整工程架构）、
> [`09_DISPLAY_DPI_ADAPTATION.md`](09_DISPLAY_DPI_ADAPTATION.md)（显示适配）与
> [`10_VISUAL_DESIGN.md`](10_VISUAL_DESIGN.md)（视觉规范）扩展并取代。
> 实现以 `rust/` 为准。**

> 目标：定义一个**可验证、可移植、与原作行为等价但实现独立**的 Rust 工程结构。
> 本文档描述**架构与决策**，不复制任何原程序代码；所有行为依据 `docs/03_FEATURE_SPEC.md`。

---

## 1. 架构总览

```
┌──────────────────────────────────────────────────────────────────────┐
│  crates/tools        rowline-replay · rowline-import · rowline-layout-check          │
│  (binaries)          只做 I/O 与格式化，逻辑全部委托给 core            │
└───────────────┬──────────────────────────────────────────────────────┘
                │
┌───────────────▼──────────────────────────────────────────────────────┐
│  crates/app           游戏装配：事件循环、状态编排、系统集成           │
│  (thin shell)         不含任何规则逻辑                                 │
└───────────────┬───────────────────────────────┬──────────────────────┘
                │                               │
┌───────────────▼───────────────┐   ┌───────────▼──────────────────────┐
│  crates/platform               │   │  crates/core                    │
│  trait 边界：                   │   │  纯逻辑，零依赖，无 I/O         │
│   Renderer / Window / Storage  │   │   board · rules · rng · layout  │
│   Clock / AssetSource           │   │   trace · engine               │
│  SDL2 / winit / wasm / headless│   │  可 no_std + alloc              │
└───────────────────────────────┘   └──────────────────────────────────┘
```

**分层铁律**

| 规则 | 含义 |
|---|---|
| **L1** | `core` **不得**依赖任何其它 crate，**不得**做 I/O，**不得**有全局可变状态 |
| **L2** | `platform` 只实现 `core` 定义的抽象，不含游戏规则 |
| **L3** | `app` 只做装配与编排，规则一律下沉到 `core` |
| **L4** | `tools` 只做 I/O，逻辑一律委托给 `core` |

> 这条分层线是**可执行的**：`cargo tree` 检查、`core` 的 `no_std` 编译测试、
> 以及三方差分门都在持续验证它。

---

## 2. Crate 划分

| Crate | 职责 | 依赖 | 状态 |
|---|---|---|---|
| `rowline-core` | 棋盘、规则、RNG、布局、trace、引擎 | **无** | ✅ 已实现 |
| `rowline-tools` | `rowline-replay` 等 CLI | core | ✅ 已实现（`rowline-replay`） |
| `rowline-platform` | 平台 trait 与后端 | core | 🟡 设计已定，见 §5 |
| `rowline-app` | 装配与主循环 | core + platform | ⬜ 未开始 |

### 2.1 为什么 `core` 零依赖

| 理由 | 后果 |
|---|---|
| 差分门必须**可复现** | 不引入 `rand`/`serde` 等会变行为的依赖 |
| 行为规格是唯一契约 | 常量与规则显式写在代码里，可被 `rg` 定位 |
| 未来可移植到 `no_std` / wasm | 只需 `#![no_std]` + `alloc` 适配 |
| 构建快、审计面小 | 供应链风险最低 |

代价是自己写 `trace` 解析（约 100 行）与错误类型（约 20 行）——这个代价比引入依赖小得多。

---

## 3. `rowline-core` 模块

| 模块 | 类型 | 职责 | 关键不变量 |
|---|---|---|---|
| `board` | 值类型 | 9×9 网格（`[Cell; 81]`） | 行主序；低 6 位是颜色；越界读返回 `EMPTY`，越界写被忽略 |
| `rules` | 纯函数 | 寻路、连通块、消除集合 | 方向顺序固定；结果**排序**输出（消除集） |
| `rng` | 值类型 | Borland LCG | 与 `state`/`colour` 逐位对齐 C 与 Python |
| `layout` | 值类型 | 客户区几何 | 公式来自规格 S3；`clamp()` 修正原版溢出缺陷 |
| `trace` | I/O 边界 | 回放格式解析 | 行式 TSV；未知 key/op 报错而非忽略 |
| `engine` | 状态所有者 | 局面 + 操作 + 计分 | **唯一**可变状态持有者；无全局变量 |

### 3.1 状态所有权

```rust
pub struct Engine {
    board: Board,          // 唯一局面
    score: i64,
    use_figures: bool,
    rng: Lcg,
    model: RuleModel,      // 全部"待定语义"集中于此
    step: u32,
}
```

- **无 `static mut`、无 `RefCell`、无 `Arc<Mutex<>>`**
- 无 `unsafe`（整个 crate 零 unsafe，编译期即可保证）
- `Engine` 天然可 `Send`（全 `Copy`/普通字段），未来多线程渲染无需额外同步

### 3.2 `RuleModel`：已定标的规则参数

```rust
pub struct RuleModel {
    pub threshold: usize,           // 5，0x401AF8 硬编码
    pub spawn_per_move: usize,      // 3，仅未消线的回合
    pub initial_balls: usize,       // 5，0x402801 新局
    pub source_keeps_colour: bool,  // false，原作的源格清空
}
```

**设计要点**：清除形状（过落点的整条直线）、4 邻域移动、非线性分值
（5→10 / 6→12 / 7→18 / 8→28 / 9→42）和“满盘才结束”都是固定语义，
不再是可调开关。三个实现（Python / C / Rust）保持逐字节一致，
差分门验证四方仍然相同。

### 3.3 确定性保证

差分门要求三方输出**逐字节相同**，因此：

| 风险 | 处理 |
|---|---|
| 消除集顺序 | 统一 `sort_unstable()` 后输出 |
| 寻路顺序 | 固定 8 方向数组 `DIR_R`/`DIR_C`，三份实现逐项对齐 |
| 连通块遍历顺序 | 固定邻接数组 `NBR_R`/`NBR_C`（与寻路方向数组刻意不同） |
| 浮点 | **核心逻辑零浮点**，全部整数运算 |
| 哈希表迭代顺序 | 不使用 `HashMap` 参与逻辑；仅 `HashMap` 用于纯查表 |

---

## 4. `rowline-platform`：trait 边界

> 本节是**设计**，实现排在 M2 之后。trait 签名先定，避免平台细节渗进 `core`。

```rust
// crates/platform/src/render.rs
pub trait Renderer {
    fn begin_frame(&mut self, size: (u32, u32));
    fn draw_sprite(&mut self, atlas: AssetId, src: Rect, dst: Point);
    fn draw_frame(&mut self, rect: Rect, style: FrameStyle);
    fn draw_text(&mut self, s: &str, at: Point, size: u16);
    fn end_frame(&mut self);
}

// crates/platform/src/window.rs
pub enum Event { Key(Key), Mouse{pos: Point, button: Button, pressed: bool},
                 Resize(u32, u32), Quit }

pub trait Window {
    fn poll(&mut self) -> Vec<Event>;
    fn title(&self) -> &str;
    fn set_min_size(&mut self, w: u32, h: u32);   // 用 Layout::min_client_size()
}

// crates/platform/src/storage.rs
pub trait Storage {
    fn read_score_table(&self) -> Result<Vec<ScoreRecord>, StorageError>;
    fn write_score_table(&mut self, recs: &[ScoreRecord]) -> Result<(), StorageError>;
    fn read_setting(&self, key: &str) -> Option<i32>;
    fn write_setting(&mut self, key: &str, value: i32) -> Result<(), StorageError>;
}

// crates/platform/src/clock.rs
pub trait Clock { fn now_millis(&self) -> u64; }
```

| trait | 平台无关原因 | 后端（未来） |
|---|---|---|
| `Renderer` | 只认"精灵 + 矩形 + 文本" | SDL2 / wgpu / web canvas |
| `Window` | 只认事件枚举 | winit / SDL2 / wasm |
| `Storage` | 只认"设置项 + 榜单记录" | JSON 文件 / 平台目录 |
| `Clock` | 只认毫秒 | `std::time` / 平台时钟 |
| `AssetSource` | 只认"图集句柄 + 源矩形" | PNG 解码 / 内嵌字节 |

> **关键收益**：`core` 里出现的所有"外部世界"都被这五个 trait 覆盖，
> 因此 `core` 可以在 wasm、桌面、服务器、甚至测试桩上运行——
> 这也是三方差分门能自动化的前提。

---

## 5. `rowline-app`：装配层

```
事件循环
 ├ Window::poll()  → Event
 ├ Event::Mouse → core::Layout::hit_cell → Engine::move_ball
 ├ Event::Key    → 命令路由 (Cmd::New / NextSet / UseFigures / ...)
 └ 每帧: Renderer::begin_frame
           ├ Renderer::draw_frame(board_frame)
           ├ Renderer::draw_sprite(逐格, src = value*36 + set*288, srcY = 225)
           └ Renderer::draw_text(score / next preview)
```

**命令路由**沿用原作的命令 ID（`docs/03` §12.1），这样快捷键行为可对照：

| ID | 键 | 动作 | Rust 侧 |
|---|---|---|---|
| 256 | F4 | 新开一局 | `Engine::new_game()` |
| 512 | F3 | 切换"显示下一组" | `settings.show_next` |
| 513 | F5 | 切换统计窗口（8 行：数量 / 百分比 / `消除-N`） | `settings.show_statistics` + `Engine::colour_counts()` / `cleared_counts()` |
| 514 | F6 | 切换形状精灵集 | `Engine::set_use_figures()` |
| 515 | Ctrl+R | 切换固定分辨率显示策略（重设计扩展） | `Game::toggle_retro_mode()` → `take_viewport_dirty()` |
| 768 | F1 | 打开帮助 | `platform::help::open()` |
| 769 | — | 关于框 | `platform::dialog::about()` |
| 258 | Alt+F9 | 退出 | `Window::request_quit()` |

---

## 6. 测试与验证策略

| 层次 | 手段 | 门 |
|---|---|---|
| 单元 | `cargo test`（crate 内 `#[cfg(test)]`） | 15 项，覆盖 RNG 序列、消除阈值、寻路、布局、trace 解析 |
| 文档 | `cargo test --doc` | 1 项，示例代码可编译 |
| **差分** | `tools/replay/diff_replay.py` | **Python ≡ C ≡ Rust ≡ golden**，5 用例 / 300+ 步 |
| 布局 | `build/rowline-layout-check` | 6 种窗口尺寸 + 81 格映射往返 |
| 行为 | 黑盒采集（`tools/blackbox/`） | 54 项清单 |

### 6.1 三方差分门（当前最关键的资产）

```
python-reference   ← 规格的第一次实现
c-implementation   ← 原型的实现
rust-implementation← 新架构的实现
golden             ← 冻结的回归基线
```

> **为什么要三份**：单一实现只能证明"它能跑"，三份独立实现互相印证才能证明
> "它对规格的理解是对的"。Rust 架构的任何重构若破坏行为，这里会立刻报警。

### 6.2 属性测试（下一阶段）

差分门验证的是**已知场景**；属性测试验证**不变量**：

| 属性 | 断言 |
|---|---|
| 确定性 | 同 seed 同操作序列 → 同输出（Rust/Python/C 三方） |
| 棋盘不变式 | 任何时刻格值 ∈ 0..=7，低 6 位有效 |
| 分数单调 | 分数只增不减 |
| 消除合法性 | 消除集的元素颜色相同、连通、数量 ≥ 阈值 |
| 布局幂等 | `Layout::compute` 后 `clamp()` 再 `compute` 结果不变 |

---

## 7. 错误处理

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceError { UnknownKey(String), UnknownOp(String), BadNumber(String), ... }
impl std::fmt::Display for TraceError { ... }
impl std::error::Error for TraceError {}
```

| 原则 | 落实 |
|---|---|
| 不引入 `anyhow`/`thiserror` | 保持零依赖；错误类型小而具体 |
| 库返回 `Result`，**不 `panic!`** | `core` 的公开 API 除 `debug_assert` 外不 panic |
| `panic = "abort"` 于 release | 与零依赖、可嵌入目标一致 |
| 输入解析失败**报错而非静默忽略** | 避免"坏 trace 被当成好 trace"这类隐蔽分歧 |

---

## 8. 并发与渲染

当前**单线程**足够（9×9 棋盘、120 颗球）。架构预留：

| 需求 | 方案 |
|---|---|
| 60 FPS | 渲染层独立线程 + 命令队列（`crossbeam` 之类在需要时再引入） |
| 动画 | 引擎产生"事件序列"，渲染层消费；引擎保持纯同步 |
| 存档 IO | 放 `platform` 的后台线程，通过 trait 边界回传 |

**前提**：`core::Engine` 全部字段都是 `Send + Sync`（无 `Rc`、无 `RefCell`），
所以可以直接跨线程移动或加锁，**不需要重构**。

---

## 9. 与 C 原型的关系

| 方面 | C 对照实现（`reference/c-impl/`） | Rust 实现（`rust/`） | 关系 |
|---|---|---|---|
| 定位 | M1 骨架 + 差分门的一方 | **新架构的正式实现** | 并行 |
| 保留理由 | 差分门需要第三方实现来交叉验证 | 长期产品实现 | C 保留为"对照实现" |
| 去向 | 归档为 `reference/c-impl/` | 取代 C 成为主实现 | 见 §11 |

---

## 10. 构建与 CI

```bash
cd rust
cargo build --release      # 产出 rust/target/release/rowline-replay
cargo test --release       # 单元 + 文档测试
cargo clippy -- -D warnings
cargo fmt --check
```

顶层 `Makefile` 集成：

```bash
make rust        # 构建 Rust workspace
make test        # 布局门 + 三方差分门（含 Rust）
```

---

## 11. 迁移路线（M1 → M6）

| 里程碑 | Rust 侧动作 | 状态 |
|---|---|---|
| **M1 骨架** | `core` 全量 + `layout` 单测 + `rowline-replay` | ✅ |
| **M2 玩法** | 语义定标后更新 `RuleModel`；补属性测试 | ✅ 已定标 |
| **M3 计分/结束** | `engine` 增加分值表与满盘结束态 | ✅ |
| **M4 持久化** | `platform::Storage` + JSON/INI 后端 + 原作导入器（`rowline-import`） | ✅ |
| **M5 表现** | `platform::Renderer` + winit/wgpu 后端 + 自制美术 | ✅ |
| **M6 交付** | i18n 未做（与原作一致固定简体中文）+ 帮助手册 + CI 多平台 | ✅ |

**切换条件**：当 Rust 实现通过三方差分门 **且** 覆盖 `BB-M01..G15` 全部规则用例，
即可弃用 C 原型，把它降级为"对照实现"留在 `reference/`。

---

## 12. 风险与对策

| 风险 | 等级 | 对策 |
|---|---|---|
| 黑盒语义定标后 Rust 与 Python/C 分歧 | **高** | 差分门立刻报警；三方必须同时改 |
| `core` 悄悄长出依赖 | 中 | `cargo tree` 检查 + `no_std` 编译测试纳入 CI |
| trait 抽象过度导致写起来别扭 | 中 | trait 数量控制在 5 个；不够再加，不提前泛化 |
| Rust 生态与原作 Win32 观感差距 | 低 | `docs/04` 已判定外观只需 ~30% 等价 |
| 美术与 36 px 网格不匹配 | 低 | `docs/05` 规格 + 占位图对齐基准 |

---

## 13. 相关文档

| 文档 | 关系 |
|---|---|
| `docs/03_FEATURE_SPEC.md` | `core` 的**唯一行为契约** |
| `docs/04_FIDELITY_MATRIX.md` | 哪些 1:1、哪些重设计 → 决定 trait 的粒度 |
| `docs/05_ART_SPEC.md` | `platform::AssetSource` 的契约 |
| `docs/06_IMPLEMENTATION_STATUS.md` | 里程碑现状 |
| `tools/blackbox/README.md` | 黑盒数据如何回填 `RuleModel` |
