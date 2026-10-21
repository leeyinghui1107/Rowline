# Rowline — Rust workspace

完整设计见 **[../docs/08_RUST_ENGINE_DESIGN.md](../docs/08_RUST_ENGINE_DESIGN.md)**，
显示适配见 **[../docs/09_DISPLAY_DPI_ADAPTATION.md](../docs/09_DISPLAY_DPI_ADAPTATION.md)**，
视觉规范见 **[../docs/10_VISUAL_DESIGN.md](../docs/10_VISUAL_DESIGN.md)**。

## Crate 结构

```
crates/
├── core/       rowline-core      纯逻辑，零依赖
│                             board · rules · rng · layout · engine · input
│                             render · view · state · save · menu · effects · trace
│                             无 I/O · 无全局状态 · 无 unsafe
├── display/    rowline-display   显示适配：ScalePolicy · Viewport · Lanczos-3
│                             纯算术，无 I/O，可完整单测
├── theme/      rowline-theme     设计令牌：深/浅色板 · 8pt 栅格 · 字阶
│                             缓动曲线 · 减少动效 · WCAG 对比度断言
├── storage/    rowline-storage   持久化：RON 存档 · 文件存储 · 原作榜单导入
├── platform/   rowline-platform  窗口事件 · 时钟 · 文件系统
│                             + winit / 无窗口 / 内存三种后端
├── render/     rowline-render    Renderer trait + wgpu 后端 + softbuffer 后端
│                             + 动态字形图集（DPR 栅格化）
├── assets/     rowline-assets   精灵图集 · 图标条 · 变体命名 · 系统字体解析
├── app/        rowline-app       装配层：事件循环 · 命令路由 · HUD · 菜单 · 特效
└── tools/      rowline-tools     wl-replay · wl-layout-check · wl-dpi-probe · wl-import
```

## 快速开始

```bash
cargo build --release --workspace --all-features
cargo test --release --workspace --all-features     # 97 项
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check

# 纯逻辑构建（零图形依赖，CI 用）
cargo build --release --workspace --no-default-features
```

从仓库根目录：`make rust` / `make rust-test` / `make test`。

## Feature 矩阵

| crate | feature | 默认 | 依赖 |
|---|---|---|---|
| `core` | `serde` | ✗ | serde（仅 derive，用于存档模型） |
| `platform` | `winit` | ✗ | winit 0.30（rwh_06） |
| | `fs` | ✗ | 真实文件系统 |
| `render` | `wgpu` | ✗ | wgpu 30 · pollster · bytemuck · glam |
| | `softbuffer` | ✗ | softbuffer 0.4 · image |
| | `text` | ✗ | ab_glyph + image（按 DPR 栅格化的动态字形图集） |
| `assets` | `jpg` / `bmp` | ✗ | 更多图片格式 |
| `storage` | `fs` | ✗ | 落盘 |
| `app` | `gpu` | ✓ | 传递 platform/winit + render/wgpu + render/text |
| | `software` | ✗ | 传递 render/softbuffer |
| | `fs` | ✗ | 传递 platform/fs + storage/fs |
| `tools` | `dpi` | ✗ | 传递 winit（显示环境探针） |

> 平台差异**不需要** feature：winit/wgpu 已覆盖三平台；
> 需要区分的是**能力**（GPU / 软件 / 文件系统 / 音频），不是操作系统。

## 设计要点

| 决策 | 理由 |
|---|---|
| `core` **零依赖** | 差分门必须可复现；行为规格是唯一契约；未来可 `no_std` |
| 不确定性集中在 `RuleModel` | 8 个字段的文档注释逐一标注对应采集项编号 |
| 引擎是**唯一状态所有者** | 无 `static mut` / `RefCell` / `Arc<Mutex>`；`Engine` 天然 `Send` |
| 核心逻辑**零浮点** | 逐字节比对的前提 |
| 渲染是**纯函数** `build_frame(&GameState) -> Vec<RenderCmd>` | 可单测；保证各后端画出来一致 |
| 两条管线 + 一个绘制列表 | 精灵与形状必须**按命令顺序**绘制，否则 z 序丢失 |
| 形状用 **SDF** | 抗锯齿、模糊、任意圆角全部免费 |
| `Key` 用 **USB HID 用量码** | 跨平台、跨键盘布局稳定的物理键位 |
| 菜单是**数据模型**（`core::menu`）+ 绘制 | 模型可单测；命令 ID 沿用原作，便于对拍 |
| 特效是**纯装饰**（`core::effects`） | 不参与判定；减少动效时直接不生成 |

## 四方差分门

本 workspace 是三方差分门的一方：

```
python-reference  ≡  c-implementation  ≡  rust-implementation  ≡  golden
```

`tools/replay/diff_replay.py` 逐字节比对四者。单一实现只能证明"能跑"；
多份独立实现互相印证，才能证明"对规格的理解是对的"。

## 待办

- [x] 设计令牌 crate（深/浅色板 + 对比度断言 + 8pt 栅格 + 缓动曲线）
- [x] SDF 形状管线（圆角 / 渐变 / 投影 / 辉光）与双管线绘制列表
- [x] 动态字形图集（按 DPR 栅格化）与系统字体回退
- [x] 菜单模型 + 绘制 + 键盘/鼠标导航 + 命令参考浮层
- [x] 粒子特效（纯装饰，可关闭）
- [x] 图标集（24px 单色 + SVG 源 + @1x/@2x/@3x）与噪点纹理
- [x] RON 存档 + 设置持久化 + 原作榜单导入器
- [ ] 属性测试：棋盘不变式 / 分数单调 / 消除合法性 / 布局幂等
- [ ] 高分榜窗口 / 统计窗口 / 关于对话框
- [ ] 消除动画的多步时序（依赖 `BB-G10`）
- [ ] wasm 后端（wgpu 已支持，缺 wasm-bindgen 装配）
- [ ] i18n（文案集中在 `core::menu` 与 `app::commands`）
