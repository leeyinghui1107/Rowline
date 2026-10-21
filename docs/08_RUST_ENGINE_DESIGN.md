# 08 · 跨平台 Rust 工程设计

> 依据：`docs/03_FEATURE_SPEC.md`（行为规格）、`docs/04_FIDELITY_MATRIX.md`（复刻/重设计边界）
> 状态：**已落地并可编译**——本文档描述的目录树、trait 与依赖矩阵均已存在于 `rust/`。

---

## 1. 技术选型与理由

| 领域 | 选型 | 理由 | 备选与否决理由 |
|---|---|---|---|
| **窗口/输入** | **winit 0.30** | 事实标准；Windows/macOS/Linux/Web 同一套代码；事件模型直接映射到本作的输入需求（`PhysicalKey` 跨布局稳定）；`rwh_06` 与 wgpu 30 的 `raw-window-handle 0.6` **版本一致**，可零胶水互操作 | ❌ `glutin`+`winit`：多一层抽象，本作用不上；❌ 直接用 `sdl2`：Rust 生态弱、winit 的 HiDPI 处理更成熟 |
| **渲染** | **wgpu 30**（主）+ **softbuffer 0.4**（次） | wgpu 覆盖 Vulkan/Metal/DX12/GL/Web，一套 WGSL 着色器跑所有平台；**softbuffer 作为"像素级参考实现"**，用于 CI 无 GPU 环境与双后端差分 | ❌ 只用 wgpu：CI/无显卡机器无法跑；❌ `macroquad`：抽象太厚，无法控制 36 px 网格的像素对齐；❌ `egui`：本作 UI 极简（一个分数 + 一个预览），全量引入不划算 |
| **音频** | **rodio 0.23**（可选 feature） | 纯 Rust、跨平台、基于 `cpal` | ⚠️ **原作完全无音频**（已确认）。因此音频是**新增功能**而非复刻项，默认关闭；仅当需要时才引入 `rodio`（其底层即 `cpal`） |
| **UI 框架** | **不用** egui/iced | 本作 UI = 一个棋盘 + 一个侧栏 + 几个按钮，**egui/iced 的即时模式对 2D 网格游戏是负担**（额外依赖、额外布局层、难以做像素级对齐） | ❌ egui：适合工具型 UI，不适合帧同步的网格游戏；❌ iced：MVVM 架构与游戏循环模型不匹配 |
| **资源** | **image 0.25** + **serde 1** + **ron 0.12** | `image` 解 PNG/BMP；`ron` 是可读、可手改、可 diff 的存档格式（**不复制原作的 XOR 混淆**，见 `docs/04` P-04） | ❌ `bincode`：不可读，破坏"用户能自己改存档"；❌ 直接用 `serde_json`：RON 保留注释，更适合配置 |
| **随机** | **不引入 `rand`** | 原作的 Borland LCG 必须**逐位复刻**才能做行为差分（见 `docs/03` S10），`rand` 的算法不可控 | ❌ `rand`：无法与原作对齐 |
| **数学** | 自带 `Point`/`Size`/`Rect`/`Color` | `core` 保持**零依赖**；`glam` 只在 wgpu 后端里用 | ⚠️ `glam` 在渲染层是合理的，但不该进 `core` |

### 1.1 依赖策略

```
core      : 零依赖（只有 std）      ← 差分门必须可复现
platform  : 只依赖 core (+ winit)   ← 平台细节止步于此
render    : 只依赖 core (+ wgpu/softbuffer)
assets    : core + image + serde + ron
app       : 上述全部 + (winit, wgpu)
```

**硬规则**：`core` 的依赖树必须为空，且能通过 `#![no_std]` + `alloc` 编译。

---

## 2. 工程结构（目录树）

```
rust/
├── Cargo.toml                       workspace：5 个 crate + 依赖表
├── README.md
└── crates/
    ├── core/            rowline-core       纯逻辑：零依赖、无 I/O、无全局状态、无 unsafe
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs        重导出与 crate 文档
    │       ├── board.rs      9×9 棋盘值类型（[Cell; 81]）
    │       ├── rules.rs      寻路 / 连通块 / RuleModel
    │       ├── rng.rs        Borland LCG
    │       ├── layout.rs     客户区几何 + clamp + min_client_size
    │       ├── engine.rs     唯一状态所有者
    │       ├── input.rs      InputEvent / Key / Modifiers / Point / Size
    │       ├── render.rs     RenderCmd / Color / Rect / FrameStyle
    │       ├── view.rs       GameState → Vec<RenderCmd>（纯函数）
    │       ├── state.rs      GameState / Phase（只读快照）
    │       ├── save.rs       SaveData / ScoreRecord / SaveFormat / legacy 布局
    │       └── trace.rs      回放 trace 格式
    │
    ├── platform/        rowline-platform   平台抽象：trait 边界 + winit / 内存实现
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs            WindowBackend / Clock / FileSystem / IoError
    │       ├── headless.rs       无窗口实现（CI / 回放）
    │       └── winit_backend.rs  winit 事件 → InputEvent 翻译
    │
    ├── render/          rowline-render     渲染抽象 + wgpu / softbuffer 后端
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs            Renderer trait + RecordingRenderer
    │       ├── wgpu_backend.rs   GPU 后端（WGSL 着色器）
    │       └── softbuffer_backend.rs  CPU 后端（像素级参考）
    │
    ├── assets/          rowline-assets    资源加载：AssetLoader / SpriteSheet / GridSpec
    │   ├── Cargo.toml
    │   └── src/lib.rs
    │
    ├── app/             rowline-app       装配层：事件循环、命令路由、入口
    │   ├── Cargo.toml
    │   └── src/
    │       ├── main.rs        入口（gpu / headless 两条路径）
    │       ├── game.rs        Game：状态机 + 输入 → 引擎操作
    │       └── commands.rs    菜单命令 ID（与原作一致）
    │
    └── tools/           rowline-tools     CLI：rowline-replay / rowline-layout-check /
                                           rowline-import / rowline-dpi-probe / rowline-shot
        ├── Cargo.toml
        └── src/bin/
            ├── rowline-replay.rs
            └── rowline-layout-check.rs
```

### 2.1 分层铁律

| 规则 | 含义 | 如何强制 |
|---|---|---|
| L1 | `core` 不依赖任何 crate、不做 I/O、无全局可变状态 | `cargo tree -p rowline-core` 必须为空；`no_std` 编译测试 |
| L2 | `platform` / `render` 只实现 `core` 定义的抽象，不含游戏规则 | 代码评审 + 差分门 |
| L3 | `app` 只做装配与编排，规则一律下沉到 `core` | 差分门（C/Python/Rust 三方一致） |
| L4 | `tools` 只做 I/O，逻辑委托 `core` | 同上 |

---

## 3. 核心 trait 与数据结构

### 3.1 `core::input` —— 平台无关输入

```rust
pub enum InputEvent {
    KeyPressed   { key: Key, modifiers: Modifiers },
    KeyReleased  { key: Key, modifiers: Modifiers },
    MousePressed { pos: Point, button: MouseButton, modifiers: Modifiers },
    MouseReleased{ pos: Point, button: MouseButton, modifiers: Modifiers },
    MouseMoved   { pos: Point },
    MouseWheel   { delta: f32 },
    Resized      { size: Size },
    ScaleFactorChanged { scale: f32 },   // HiDPI
    QuitRequested,
}
```

`Key` 使用 **USB HID 用量码**（`KeyA = 0x04` … `F12 = 0x45`），跨平台、跨键盘布局稳定——
这正是游戏需要的（布局无关的物理键位）。

### 3.2 `core::render` —— 渲染命令集

```rust
pub enum RenderCmd {
    Clear(Color),
    Sprite { sheet: SpriteSheetId, src: Rect, dst: Point, size: Size, tint: Color },
    Frame  { rect: Rect, style: FrameStyle },          // Sunken / Raised / Flat
    Text   { text: String, at: Point, size: u16, color: Color },
    Outline{ rect: Rect, color: Color },
}
```

**游戏逻辑永不接触图形 API**，只产出命令；后端决定如何变成像素。

### 3.3 `core::state` —— 状态快照

```rust
pub struct GameState {
    pub phase: Phase,          // Boot | Ready | Selected | Animating | GameOver | Tampered
    pub score: i64,
    pub best_score: i64,
    pub board: Board,
    pub selected: Option<usize>,
    pub path: Vec<usize>,
    pub show_next: bool,
    pub show_statistics: bool,
    pub use_figures: bool,
    pub next_colour: u8,
    pub layout: Layout,
}
```

`GameState` 是**只读快照**；权威可变状态在 `Engine` 里。UI 与渲染层拿不到可变引用。

### 3.4 `core::view` —— 纯函数式出图

```rust
pub fn build_frame(state: &GameState) -> Vec<RenderCmd>;
```

整帧的视觉计划**完全由状态决定**，无 I/O、无平台类型 → 可单测，且保证各后端画出来一致。

### 3.5 `core::save` —— 可插拔存档编码

```rust
pub trait SaveFormat {
    fn encode(&self, save: &SaveData) -> Result<Vec<u8>, SaveError>;
    fn decode(&self, bytes: &[u8]) -> Result<SaveData, SaveError>;
    fn extension(&self) -> &'static str;
}

pub struct SaveData { pub version: u32, pub scores: Vec<ScoreRecord>, pub settings: Settings }
```

同时保留**原作表布局**以便导入旧数据：

```rust
pub mod legacy {
    pub const ENTRIES: usize = 16;
    pub const RECORD_LEN: usize = 32;
    pub const NAME_LEN: usize = 28;
    pub const SCORE_OFFSET: usize = 0x1C;
    pub fn decode_table(blob: &[u8]) -> Vec<ScoreRecord>;
    pub fn encode_table(records: &[ScoreRecord]) -> Vec<u8>;
}
```

### 3.6 `platform` —— 平台边界

```rust
pub trait WindowBackend {
    fn poll_events(&mut self) -> Vec<InputEvent>;
    fn size(&self) -> Size;
    fn scale_factor(&self) -> f32;
    fn request_quit(&mut self);
    fn set_min_size(&mut self, size: Size);
    fn set_title(&mut self, title: &str);
}
pub trait Clock { fn now_millis(&self) -> u64; }
pub trait FileSystem {
    fn app_dir(&self) -> PathBuf;
    fn read(&self, path: &Path) -> Result<Vec<u8>, IoError>;
    fn write(&mut self, path: &Path, bytes: &[u8]) -> Result<(), IoError>;
    fn exists(&self, path: &Path) -> bool;
}
```

`FileSystem::app_dir()` 遵循各平台约定：

| 平台 | 路径 |
|---|---|
| Linux | `$XDG_DATA_HOME/rowline` 或 `~/.local/share/rowline` |
| macOS | `~/Library/Application Support/rowline` |
| Windows | `%APPDATA%\rowline` |

### 3.7 `render` —— 渲染边界

```rust
pub trait Renderer {
    fn begin(&mut self, size: Size, scale: f32);
    fn submit(&mut self, cmd: RenderCmd);
    fn end(&mut self);
    fn size(&self) -> Size;
}
```

`RecordingRenderer` 把命令流录下来，用于断言与差分。

### 3.8 `assets` —— 资源边界

```rust
pub trait AssetLoader {
    fn load_sheet(&mut self, key: &AssetKey, grid: GridSpec) -> Result<SpriteSheet, AssetError>;
    fn load_bytes(&mut self, key: &AssetKey) -> Result<Vec<u8>, AssetError>;
}

pub struct GridSpec { pub cell: u32, pub cols: u32, pub rows: u32, pub origin_y: u32 }
impl GridSpec { pub const ORIGINAL: GridSpec = GridSpec { cell: 36, cols: 16, rows: 10, origin_y: 225 }; }
```

**索引契约**（`docs/05`）在类型层面被强制：

```rust
impl SpriteSheet {
    pub fn ball(&self, value: u8, sprite_set: u8) -> Rect {
        let col = value as u32 + sprite_set as u32 * 8;
        self.slot(col.min(self.grid.cols.saturating_sub(1)))
    }
}
```

---

## 4. Cargo.toml 与 feature 配置

### 4.1 workspace

```toml
[workspace]
resolver = "2"
members = ["crates/core", "crates/platform", "crates/render",
           "crates/assets", "crates/app", "crates/tools"]

[workspace.dependencies]
rowline-core     = { path = "crates/core" }
rowline-platform = { path = "crates/platform" }
rowline-render   = { path = "crates/render" }
rowline-assets   = { path = "crates/assets" }
rowline-app      = { path = "crates/app" }
```

### 4.2 feature 矩阵

| crate | feature | 默认 | 作用 | 依赖 |
|---|---|---|---|---|
| `core` | — | — | 纯逻辑 | 无 |
| `platform` | `winit` | ✗ | 真实窗口 | `winit 0.30`（rwh_06） |
| | `fs` | ✗ | 真实文件系统 | — |
| `render` | `wgpu` | ✗ | GPU 后端 | `wgpu 30`, `pollster`, `bytemuck`, `glam` |
| | `softbuffer` | ✗ | CPU 后端 | `softbuffer 0.4`, `image` |
| | `text` | ✗ | 字体栅格化 | `ab_glyph` |
| `assets` | `jpg` / `bmp` | ✗ | 更多图片格式 | `image/jpeg`, `image/bmp` |
| `app` | `gpu` | ✓ | winit + wgpu 产品路径 | 传递 `platform/winit`, `render/wgpu` |
| | `software` | ✗ | softbuffer 路径 | 传递 `render/softbuffer` |
| | `fs` | ✗ | 存档落盘 | 传递 `platform/fs` |

> **默认只有 `gpu`**：一条命令即可得到可运行的桌面程序；
> `cargo build --workspace --no-default-features` 得到零图形依赖的纯逻辑构建（CI 用）。

### 4.3 平台差异不需要 feature

`winit` 与 `wgpu` 已经覆盖三平台，**不需要** `windows`/`macos`/`linux` 这类平台 feature。
真正需要区分的是**能力**（GPU / 软件 / 文件系统 / 音频），而不是操作系统。
平台差异在 `platform::OsFileSystem::app_dir()` 内部用 `cfg!(target_os = ...)` 处理。

---

## 5. 迁移路线图

| 阶段 | 目标 | 验收门 | 状态 |
|---|---|---|---|
| **M0 空窗口** | 打开窗口、HiDPI 正确、清屏、退出 | 窗口可开可关；`scale_factor` 正确传递到渲染 | ✅ 已实现（`app` 的 gpu 路径） |
| **M1 静态帧** | 用 `build_frame` 画出棋盘 | 渲染结果与 `geometry.py` 实测几何**逐像素一致** | ✅ 已实现（`core::view` + wgpu/softbuffer 后端） |
| **M2 输入** | 点击 → 选球 → 落子 → 消除 | `BB-K01..K10` 通过；输入映射与 `Command` 表一致 | 🟡 框架已实现（`Game::handle_event`），**语义待黑盒定标** |
| **M3 核心玩法** | 连通判定、寻路、补球、计分 | **四方差分门**（Python ≡ C ≡ Rust ≡ golden）全绿 | ✅ 差分门已就位；语义待定标 |
| **M4 存档** | INI/JSON 设置 + 高分榜 + 原作导入器 | 老注册表数据可导入；往返一致 | 🟡 布局与 `legacy` 编解码已实现，**读写器与导入器未写** |
| **M5 表现** | 动画、消除高亮、菜单、关于框、帮助 | 评审通过 | ⬜ 菜单/对话框未做 |
| **M6 打包** | Windows/macOS/Linux 分发包、CI | 三平台产物可运行 | ⬜ |

### 5.1 每阶段的"不变量"

迁移过程中**任何一步都不得破坏**：

| 不变量 | 检查方式 |
|---|---|
| 行为与原作一致 | `make test`（四方差分门） |
| 布局公式与实测一致 | `build/rowline-layout-check` + `tools/blackbox/analyze.py shot` |
| `core` 零依赖 | `cargo tree -p rowline-core` |
| 无 `unsafe`（core） | `grep -rn unsafe crates/core/src` |
| 精灵索引契约 | `core::view::ball_src` 的单测 |

---

## 6. 关键接口代码骨架

### 6.1 `core` 的规则模型（不确定性集中处）

```rust
#[derive(Clone, Copy, Debug)]
pub struct RuleModel {
    pub threshold: usize,            // BB-G03   已确认 = 5
    pub source_keeps_colour: bool,   // BB-G03
    pub max_turns: Option<usize>,    // BB-G04
    pub refill: usize,               // BB-G02 / BB-G06
    pub initial_balls: usize,         // BB-G01 -> 5 (screenshot)
    pub points_per_ball: i64,        // BB-G11
    pub min_score_delta: i64,        // BB-G11
    pub allow_revisit_source: bool,  // BB-G05
}
```

> 每个字段都标注**对应的黑盒采集项编号**。定标时只改这一个结构体
> （Python / C / Rust 各一份），差分门立刻验证三方是否仍一致。

### 6.2 引擎的一次移动

```rust
pub fn move_ball(&mut self, r0: i32, c0: i32, r1: i32, c1: i32) -> MoveResult {
    // 1) 边界与合法性
    // 2) find_path（8 连通、空格行走、方向序固定）
    // 3) 落子
    // 4) connected_group（阈值 model.threshold）
    // 5) 消除 + 计分 + 补球
}
```

### 6.3 渲染一帧

```rust
// app
renderer.begin(physical, scale);
for cmd in rowline_core::view::build_frame(&game.snapshot(layout)) {
    renderer.submit(cmd);
}
renderer.end();
```

### 6.4 wgpu 后端的最小实现要点

| 环节 | 要点 |
|---|---|
| 表面 | `wgpu::Instance::new(InstanceDescriptor::new_without_display_handle())` → `create_surface(&'static Window)`（winit 的 `Window` 泄漏以获得 `'static`，进程级一次性） |
| 管线 | 单管线覆盖一切：纹理四边形（精灵）+ 1×1 白纹理着色（边框/高亮） |
| 顶点 | `pos(clip) + uv + tint`，6 顶点/四边形，写入常驻 `VERTEX|COPY_DST` buffer |
| 着色器 | WGSL，`textureSample(sheet, samp, uv) * tint` |
| HiDPI | `scale` 参与 clip 变换，逻辑像素 → 物理像素只在这一处换算 |
| 文本 | **未接字体管线**，暂用占位方块（`RenderCmd::Text` 已在抽象里就位，接 `ab_glyph` 即可） |

### 6.5 softbuffer 后端与软件画布

纯 CPU：精灵按最近邻缩放 blit，边框/高亮为 1 px 矩形。
用途：**无 GPU 驱动的 CI** 与**像素级参考实现**（未来可与 wgpu 后端做像素差分）。

光栅化的核心抽到了 `render::canvas::SoftwareCanvas`：

| 组件 | 职责 |
|---|---|
| `canvas::SoftwareCanvas` | 与窗口无关的光栅器：矩形 / 圆角 / 渐变 / 阴影 / 图标 / 精灵 / 文字 |
| `softbuffer_backend` | 把画布的像素搬到窗口 surface 上呈现 |
| `tools/rowline-shot` | 把画布直接写成 PNG，**不需要 GPU、不需要显示器** |

因为两者共用同一份光栅代码，离线 PNG 与软件后端的画面**逐像素一致**；
这让"视觉是否正确"变成一个可以在 CI 里复核的事情（`make shot`）。

文字由 `render::font::FontAtlas`（`ab_glyph`）在 **DPR** 下栅格化成动态图集：
字形按需光栅化 → 打进图集 → 以**逻辑单位**的四边形提交。
图集按字号各一份，因此 `RenderCmd::Text` 的 `size` 是真正生效的。

> ⚠️ 一个已修掉的坑：`ab_glyph 0.2` 的 `OutlinedGlyph::draw` 回调坐标
> **已经相对边界框偏移过**，且类型是 `u32`。
> 按"绝对 i32 坐标"去换算会把所有负坐标判成越界 → **字形全部被丢弃，文字全白**。
> 现在有单测盯着这件事（`font::tests::the_atlas_image_actually_holds_the_glyphs`）。

---

## 7. 风险与对策

| 风险 | 等级 | 对策 |
|---|---|---|
| winit / wgpu 版本漂移（本次即遇到 API 变更 3 轮） | **高** | 锁定 `Cargo.lock`；CI 里 `cargo update --dry-run` 报警；后端隔离在单个文件，升级影响面可控 |
| `raw-window-handle` 版本错配 | **高** | wgpu 30 与 winit 0.30 都用 **0.6**，已验证；升级任一方时**先查这个** |
| 文本渲染缺失 | 中 | 抽象已就位；接 `ab_glyph` 或内置位图字体 |
| `core` 被污染（长出依赖/unsafe） | 中 | `cargo tree` 检查 + `no_std` 编译测试进 CI |
| 语义定标导致三方分歧 | 中 | 四方差分门当场报警 |
| HiDPI 下 36 px 网格错位 | 中 | 逻辑/物理像素分离；渲染前用 `geometry.py` 校验 |

---

## 8. 与其它文档的关系

| 文档 | 关系 |
|---|---|
| `docs/03_FEATURE_SPEC.md` | `core` 的**唯一行为契约** |
| `docs/04_FIDELITY_MATRIX.md` | 决定 trait 粒度与哪些能力要做 |
| `docs/05_ART_SPEC.md` | `assets::GridSpec` / `SpriteSheet::ball` 的契约 |
| `docs/07_RUST_ARCHITECTURE.md` | 前一版（逻辑层架构）；本文档是**完整工程架构**，取代之 |
| `tools/blackbox/README.md` | 黑盒数据如何回填 `RuleModel` |
| `docs/06_IMPLEMENTATION_STATUS.md` | 里程碑现状 |
