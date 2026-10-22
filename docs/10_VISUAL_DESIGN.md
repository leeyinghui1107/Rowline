# 10 · 视觉设计规范（Visual Design System）

> 目标：在**不改变玩法**的前提下，把 1999 年的 Win95 观感升级为现代、可访问、可跨平台的视觉语言。
> 状态：**已落地并可编译**。本文档中的色值、间距、曲线与代码均来自 `rust/crates/theme/` 与 `rust/crates/render/`。

---

## 0. 设计原则

| 原则 | 含义 | 落地 |
|---|---|---|
| **玩法零改动** | 棋盘 9×9、格 36 逻辑 px、消除阈值 5、精灵索引契约 —— 一个字节都不动 | 表现层完全在 `core::view` 之上，规则层不受影响 |
| **语义色** | 代码里只写 `text_primary`，不写 `#F2F4F8` | `rowline_theme::Palette`；换主题是一行 |
| **对比度由构建保证** | 不靠肉眼判断对比度 | `theme::contrast` 的单测**断言**每一组前景/背景 ≥ 4.5:1 |
| **动效可关闭** | 减少动效是一个开关，不是一次重构 | `Motion { reduced }`，所有时长归零 |
| **字体按 DPR 栅格化** | 200% 屏上文字必须锐利 | `render::font::FontAtlas`，动态字形图集 |
| **一个换算点** | 逻辑→物理只在一个地方发生 | 渲染层的 `content_scale` |

---

## 1. 设计语言

> **默认主题是 "Classic"（原作 Win95 外观）**；Midnight / Paper 是可选的
> 重设计外观，用 `Ctrl+T` 循环切换。复刻验收以 Classic 为准。

### 1.0 经典主题 "Classic"（默认，复刻原作）

| 语义令牌 | 值 | 来源 |
|---|---|---|
| `background` / `board_backdrop` | `#000000` | 原版棋盘区黑底 |
| `surface_alt`（菜单 / HUD） | `#F0F0F0` | `GetSysColor(COLOR_BTNFACE)`，Win10/11 |
| `board_cell` | `#C0C0C0` | 原版图集空格底板 |
| `score_fill` / `score_text` / `score_border` | 黑 / 白 / 白 | 实机截图 |
| `text_primary` / `text_tertiary` | 黑 / `#606060` | 菜单黑字、`下组`/`颜色` 灰字（AA 5.2:1） |
| `surface_hover` / `selection` | `#000080` | Win95 菜单高亮 / 选中 |
| 7 色球 | `#00A000` `#0000C0` `#00C0C0` `#C000C0` `#C00000` `#C0C000` `#800000` | 原作图集直方图 |

几何：

* 菜单条 **20 px**（原作的原生菜单在非客户区；本实现自绘在客户区顶部）；
* HUD **44 px**：左分数框（80×20，黑底白字、右对齐）、`下组`、3×36 px
  预览格、`颜色`、右分数框；
* 棋盘格 36 px，底板自带 Win95 立体边：上/左白、下/右 `#292929`、外沿
  1 px 黑缝；
* 圆角：**全部为 0**（`DesignSystem::radius()` 在 Classic 下返回 0）；
* 阴影/辉光：无（令牌为全透明）；
* 不绘制重设计工具栏（主题/显示两个按钮），快捷键仍然可用。

### 1.1 深色主题 "Midnight"

| 语义令牌 | HEX | 用途 |
|---|---|---|
| `background` | `#12141A` | 窗口底色 |
| `surface` | `#1B1E26` | 棋盘面 |
| `surface_alt` | `#161920` | 下沉面（侧栏） |
| `surface_raised` | `#242833` | 抬起面（按钮、卡片） |
| `surface_hover` | `#2C3140` | 悬停 |
| `surface_active` | `#343A4B` | 按下 |
| `outline` | `#3A4050` | 1 px 描边 |
| `outline_strong` | `#565F73` | 强描边 / 焦点环 |
| `text_primary` | `#F2F4F8` | 标题、正文 |
| `text_secondary` | `#B8BECE` | 次要文字 |
| `text_tertiary` | `#8E95A8` | 提示、说明 |
| `text_on_accent` | `#12141A` | 主色上的文字 |
| `accent` | `#FFC24B` | 主色（琥珀，呼应原作的棋/金意象） |
| `accent_hover` | `#FFD066` | 主色悬停 |
| `success` | `#5BD98A` | 成功 / 高分 |
| `warning` | `#FFC24B` | 警告 |
| `danger` | `#FF7B7B` | 危险 / 结束 |
| `shadow` | `#000000A8` | 投影（alpha 66%） |
| `glow` | `#FFC24B40` | 辉光（alpha 25%） |
| `grid_line` | `#FFFFFF0F` | 棋盘网格线（alpha 6%） |
| `selection` | `#FFC24B` | 选中态 |

### 1.2 浅色主题 "Paper"

| 语义令牌 | HEX | 用途 |
|---|---|---|
| `background` | `#F4F5F8` | 窗口底色 |
| `surface` | `#FFFFFF` | 棋盘面 |
| `surface_alt` | `#EDEFF3` | 下沉面（侧栏） |
| `surface_raised` | `#FFFFFF` | 抬起面 |
| `surface_hover` | `#E6E9EF` | 悬停 |
| `surface_active` | `#DCE0E8` | 按下 |
| `outline` | `#D2D6DF` | 描边 |
| `outline_strong` | `#9AA2B2` | 强描边 |
| `text_primary` | `#14161C` | 标题、正文 |
| `text_secondary` | `#454C5C` | 次要文字 |
| `text_tertiary` | `#656D7E` | 提示、说明 |
| `text_on_accent` | `#FFFFFF` | 主色上的文字 |
| `accent` | `#9A5B00` | 主色（深琥珀，保证白底可读） |
| `accent_hover` | `#7E4A00` | 主色悬停 |
| `success` | `#186C3B` | 成功 |
| `warning` | `#8A5A00` | 警告 |
| `danger` | `#B3261E` | 危险 |
| `shadow` | `#14161C24` | 投影（alpha 14%） |
| `glow` | `#9A5B0033` | 辉光 |
| `grid_line` | `#14161C12` | 网格线 |
| `selection` | `#9A5B00` | 选中态 |

### 1.3 球色板（7 色，按测量选型）

| # | 语义 | 深色 | 浅色 | 灰度亮度 |
|---|---|---|---|---|
| 1 | green | `#00DB16` | `#00850D` | 0.507 / 0.168 |
| 2 | blue | `#3870FF` | `#002EA3` | 0.197 / 0.046 |
| 3 | cyan | `#00A8B2` | `#00737A` | 0.312 / 0.137 |
| 4 | magenta | `#FF75F1` | `#8F0081` | 0.403 / 0.074 |
| 5 | red | `#FF4242` | `#FF0505` | 0.256 / 0.214 |
| 6 | yellow | `#FFC42E` | `#BD8800` | 0.609 / 0.284 |
| 7 | maroon | `#C24600` | `#A83D00` | 0.159 / 0.117 |

**选型依据（由脚本计算，非肉眼）**：

| 约束 | 深色 | 浅色 |
|---|---|---|
| 每个球 vs 棋盘面对比度 ≥ 3:1 | 最低 **3.31** | 最低 **3.14** |
| 两球灰度比 ≥ 1.10 | 最差 **1.182** | 最差 **1.120** |

> **为什么 7 种颜色还要靠形状区分**：七种颜色要在纯亮度上完全可分是极苛刻的约束。
> 本作**本来就有形状通道**——原作的"使用形状"精灵组给每种颜色配了不同的多边形。
> 因此：**颜色 = 第一通道，形状 = 第二通道**，灰度下的可辨识度由形状保证，
> 亮度只作为辅助。这是冗余编码，比强行拉 luminance 更专业。

### 1.4 8 pt 栅格

| 令牌 | 值 | 用途 |
|---|---|---|
| `Space::None` | 0 | — |
| `Space::Xs` | 4 | 图标与文字的最小间隙 |
| `Space::Sm` | 8 | 控件内边距（半档） |
| `Space::Md` | 12 | **基准间距**（默认） |
| `Space::Lg` | 16 | 面板内边距 |
| `Space::Xl` | 24 | 区块之间 |
| `Space::Xxl` | 32 | 页面级间距 |
| `Space::Huge` | 48 | 分区之间 |

> 4 pt 是半档（用于图标对齐），其余均为 8 的倍数。

### 1.5 圆角

| 令牌 | 值 | 用途 |
|---|---|---|
| `Radius::None` | 0 | 棋盘格、进度条 |
| `Radius::Sm` | 4 | 标签、chip |
| `Radius::Md` | 8 | 输入框、次级按钮（**默认**） |
| `Radius::Lg` | 12 | 卡片、侧栏 |
| `Radius::Xl` | 16 | 模态 |
| `Radius::Pill` | 999 | 药丸按钮、徽章 |

### 1.6 阴影

| 令牌 | Y 偏移 | 模糊 | 用途 |
|---|---|---|---|
| `Shadow::NONE` | 0 | 0 | 平面元素 |
| `Shadow::SM` | 1 | 3 | 卡片（**默认**） |
| `Shadow::MD` | 2 | 8 | 抬起面、按钮 |
| `Shadow::LG` | 8 | 24 | 模态、弹出层 |

> 阴影是**单一柔边**，不用"硬阴影 + 柔阴影"的双层写法——双层在深色主题上会显脏。

### 1.7 字体层级

| 角色 | 字号 | 行高 | 字重 | 字距 | 等宽数字 | 用途 |
|---|---|---|---|---|---|---|
| `Display` | 28 | 36 | 700 | −0.4 | | 屏幕标题 |
| `Title` | 20 | 28 | 600 | −0.2 | | 区块标题 |
| `Subtitle` | 16 | 24 | 600 | 0 | | 子标题 |
| `Body` | 14 | 20 | 400 | 0 | | 正文 |
| `Label` | 12 | 16 | 500 | 0 | | 控件标签 |
| `Caption` | 11 | 16 | 400 | 0 | | 提示 |
| `Numeric` | 26 | 28 | 700 | +0.5 | ✅ | **分数** |

> **数字必须用等宽数字（tabular figures）**：分数每次变化，
> 非等宽数字会让整个 HUD 左右抖动，这是游戏里最刺眼的动效。
> 字符间距 +0.5 pt 用来抵消等宽数字视觉上过紧的字面。

---

## 2. 动效

### 2.1 缓动曲线

| 曲线 | 公式 | 用途 |
|---|---|---|
| `Linear` | `t` | 进度条、纯位移 |
| `OutCubic` | `1−(1−t)³` | **默认**：悬停、按下、淡入 |
| `InOutCubic` | 对称三次 | 界面切换（进 + 出） |
| `OutQuint` | `1−(1−t)⁵` | 更"脆"的强调 |
| `OutBack` | 回弹（会过冲） | 消除时的球体弹出 |
| `OutExpo` | `1−e^(−10t)` | 辉光、快速消散 |

### 2.2 时长

| 令牌 | 毫秒 | 用途 |
|---|---|---|
| `Duration::Instant` | 0 | 减少动效时的替身 |
| `Duration::Fast` | 120 | 悬停 / 按下 |
| `Duration::Normal` | 200 | 面板展开、淡入 |
| `Duration::Slow` | 320 | 界面切换 |
| `Duration::Slower` | 480 | 消除动画 |

### 2.3 交互状态

| 状态 | 表现 | 时长 / 曲线 |
|---|---|---|
| Idle | `surface_raised` | — |
| Hover | `surface_hover` + 轻微上浮 1 px | `Fast` / `OutCubic` |
| Press | `surface_active` + 下沉 1 px + 缩放 0.98 | `Fast` / `OutCubic` |
| Focus | `outline_strong` 2 px 焦点环 | `Fast` / `Linear` |
| Disabled | `surface_alt` + `text_tertiary` | — |

```rust
// crates/theme/src/theme.rs
pub fn button_surface(&self, p: &Palette) -> Rgba {
    match self.state {
        HitState::Idle    => p.surface_raised,
        HitState::Hover   => p.surface_hover,
        HitState::Press   => p.surface_active,
        HitState::Disabled => p.surface_alt,
    }
}
```

### 2.4 界面切换淡入

| 场景 | 淡入 | 位移 | 时长 |
|---|---|---|---|
| 统计窗口打开 | 0 → 1 | Y +8 px → 0 | `Slow` / `OutCubic` |
| 侧栏出现 | 0 → 1 | X +12 px → 0 | `Normal` / `OutCubic` |
| 提示消息 | 0 → 1 | Y −8 px → 0 | `Normal` / `OutCubic` |
| 消除高亮 | 0 → 1 | 路径逐段绘制 | `Slower` / `OutCubic` |

### 2.5 粒子与光效（可选，不影响玩法）

| 效果 | 实现 | 触发 | 备注 |
|---|---|---|---|
| 消除迸发 | 8–12 个小四边形，沿路径法线方向散射，`OutQuint` 衰减 | 消除时 | 纯装饰，关闭后不影响任何判定 |
| 选中辉光 | 球体外圈 `Glow`，半径 8 px | 选中时 | 用 `accent` 的 25% alpha |
| 分数跳动 | 数字缩放 1.0 → 1.12 → 1.0，`OutBack` | 分数变化 | 必须在等宽数字下做，否则抖动叠加 |
| 路径流光 | 沿高亮路径移动的高光点 | 预览路径 | 可选 |

> **所有粒子都是纯装饰**：
> 它们不参与命中测试、不影响棋盘状态、在"减少动效"下完全不生成。

### 2.6 减少动效

```rust
// crates/theme/src/motion.rs
pub fn duration(&self, d: Duration) -> u32 {
    if self.reduced { 0 } else { d.ms() }
}

pub fn progress(&self, elapsed_ms: u32, d: Duration, easing: Easing) -> f32 {
    let total = self.duration(d);
    if total == 0 { return 1.0; }          // 立刻到达终态，不留半截动画
    let t = (elapsed_ms as f32 / total as f32).clamp(0.0, 1.0);
    easing.apply(t)
}
```

**减少动效时的行为约定**：

| 行为 | 正常 | 减少动效 |
|---|---|---|
| 悬停/按下 | 渐变过渡 | 瞬时切换 |
| 面板淡入 | 淡入 + 位移 | 直接出现（无透明中间态） |
| 消除动画 | 多步重绘 + `Sleep` | 立即呈现最终状态 |
| 粒子 | 生成 | 不生成 |
| 分数跳动 | 缩放脉冲 | 无脉冲（数字照常更新） |
| 提示消息 | 淡入后停留再淡出 | 直接显示，按时长消失 |

**切换方式**：`Ctrl+M`（运行时）或设置项持久化（`Settings`）。

---

## 3. 资产规范

### 3.1 图标风格

| 项 | 规范 |
|---|---|
| 网格 | **24 × 24**，2 px 安全边距，图形落在 20 × 20 内 |
| 线宽 | 2 px（圆头端点），小图标 1.5 px |
| 圆角 | 与容器一致（`Radius::Sm` = 4 px） |
| 风格 | **线性（outline）**，与描边体系同源；强调态可填充 |
| 颜色 | 只用 `text_primary` / `text_secondary` / `accent` 三种语义色，**不用多色图标** |
| 状态 | 默认 / 悬停（`text_secondary` → `text_primary`）/ 按下（`accent`） |
| 导出 | `@1x` 24×24、`@2x` 48×48、`@3x` 72×72、`@4x` 96×96；**单色 PNG**，颜色由着色器染 |
| 交付 | 24×24 的**简单几何**优先，复杂图标降级为 32×32 并调整网格 |

> **单色图标 + 运行时染色**是现代做法：一套资产，两套主题，零额外文件。

### 3.2 背景渐变与噪点

| 用途 | 做法 |
|---|---|
| 窗口背景 | `Gradient`，`background` → `surface`，`Vertical`，alpha 差 ≤ 6% |
| 侧栏 | `Gradient`，`surface_alt` → `surface`，`Radial`，中心亮 |
| 噪点 | 一张 **256×256 可平铺**的噪点纹理，alpha ≤ 3%，`Overlay` 混合；`@2x` 版本用同样种子生成 |
| 棋盘格线 | `grid_line`，1 px，alpha 6%（深）/ 7%（浅），仅在 ≥150% DPI 下绘制（否则 1 px 线会显得脏） |

> **噪点必须可平铺且低 alpha**：它是为了消除大面积纯色的"塑料感"，一旦看得见就失败了。

### 3.3 @2x / @3x / @4x 导出要求

| 资产 | @1x | @2x | @3x | @4x | 说明 |
|---|---|---|---|---|---|
| 精灵图集 | 576×586 | 1152×1172 | 1728×1758 | 2304×2344 | **真正的高分辨率重绘**，不是放大 |
| 图标 | 24×24 | 48×48 | 72×72 | 96×96 | 单色 PNG |
| 噪点 | 256×256 | 512×512 | 768×768 | 1024×1024 | 同种子生成 |
| 字体 | — | — | — | — | **不导出**，运行时按 DPR 栅格化 |

**命名约定**：`name.png` / `name@2x.png` / `name@3x.png` / `name@4x.png`
（与 `assets::variant_name` 一致）。噪声纹理目前只有 @1x；它是低 alpha
的平铺颗粒，放大由 GPU 采样完成，不影响清晰度。

**精灵图集的倍率约束**（`docs/05` 的扩展）：

```
格边长   = 36 × 倍率
采样带 Y = 225 × 倍率
源 X     = (value + set × 8) × 36 × 倍率
```

**为什么精灵图集必须"重绘"而非"放大"**：放大 2× 的位图在 200% 屏上
每个源像素变成 2×2 的模糊块；真正重绘则每个目标像素都是实心的。

本项目的素材是程序化生成的，所以倍率版本是**天然的高分辨率**：

```bash
python3 tools/make_sheet.py
#   -> rowline_sheet.png  (576×586)
#   -> rowline_sheet@2x.png (1152×1172)
#   -> rowline_sheet@3x.png (1728×1758)
#   -> rowline_sheet@4x.png (2304×2344)
```

### 3.4 动效资产

| 资产 | 形式 | 说明 |
|---|---|---|
| 粒子 | **程序生成**（四边形 + 颜色 + 轨迹） | 不需要美术资源 |
| 辉光 | SDF 外发光 | 参数化，不需资源 |
| 渐变 | 参数化 | 不需资源 |

> 动效一律**参数化**，不引入序列帧资源——序列帧在任意 DPI 下都要重做。

---

## 4. Rust 落地

### 4.1 为什么不用 egui

| 考虑 | 结论 |
|---|---|
| egui 适合 | 工具型 UI、即时模式、复杂表单 |
| egui 不适合 | **帧同步的网格游戏**：它每帧重建 UI 状态，与游戏循环争夺控制权 |
| 本作的实际 UI | 棋盘 + 一个侧栏 HUD（分数、最佳分、预览球）——**十来个元素** |
| 引入 egui 的代价 | 一整套依赖（egui + egui-wgpu + 字体栈）、一整套抽象、一份额外的渲染管线 |
| 自定义管线的代价 | 一个 SDF 着色器 + 5 条新指令 —— **已经在本项目里实现好了** |

**决定：自定义 wgpu 管线**（已实现），egui 留在"设置对话框"这类非游戏内 UI 的备选里。

### 4.2 配色常量

全部在 `rowline_theme::Palette`，深浅两套，见 §1.1 / §1.2。调用侧只写语义名：

```rust
use rowline_theme::{DesignSystem, Palette, ThemeKind};

let ds = DesignSystem::new(ThemeKind::Dark, /* reduced_motion */ false);
let p: &Palette = &ds.palette;
cmd(RenderCmd::RoundedRect {
    rect: panel,
    radius: ds.radius(Radius::Lg),      // 12
    color: p.surface_alt.into(),
});
```

### 4.3 圆角与阴影的绘制代码

**指令侧**（`core::render`）新增了四个图元：

```rust
pub enum RenderCmd {
    // ... 既有指令 ...
    /// A filled rounded rectangle.
    RoundedRect { rect: Rect, radius: f32, color: Color },
    /// A gradient inside a rounded rectangle.
    Gradient { rect: Rect, radius: f32, from: Color, to: Color,
               direction: GradientDirection },
    /// A soft drop shadow cast by a rounded rectangle.
    Shadow { rect: Rect, radius: f32, offset: Point, blur: f32, color: Color },
    /// An outer glow around a rounded rectangle.
    Glow { rect: Rect, radius: f32, blur: f32, color: Color },
}
```

**着色器侧**（`render/src/wgpu_backend.rs`）——全部用 **SDF**：

```wgsl
// Signed distance to a rounded box centred on the origin.
fn sd_round_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(v: VertexOut) -> @location(0) vec4<f32> {
    let radius = v.params.x;
    let blur   = max(v.params.y, 0.001);
    let mode   = v.params.z;      // 0 solid, 1 vertical, 2 horizontal, 3 radial
    let kind   = v.params.w;      // 0 shape, 1 shadow, 2 glow

    let half = v.size * 0.5;
    let p = v.local - half;
    let d = sd_round_box(p, half, radius);

    // Soft shadow / glow: the blur is just a wider smoothstep band.
    if (kind > 0.5) {
        let a = 1.0 - smoothstep(-blur, blur, d);
        let alpha = select(a, a * a, kind > 1.5);   // glow falls off faster
        return vec4<f32>(v.color_a.rgb, v.color_a.a * alpha);
    }

    // Anti-aliasing for free: the derivative of the distance field is the
    // screen-space width of one pixel.
    let aa = max(fwidth(d), 0.0001);
    let cov = 1.0 - smoothstep(-aa, aa, d);

    var col = v.color_a;
    if (mode > 0.5) {
        var t = clamp(v.local.y / max(v.size.y, 1.0), 0.0, 1.0);
        if (mode > 1.5 && mode < 2.5) {
            t = clamp(v.local.x / max(v.size.x, 1.0), 0.0, 1.0);
        } else if (mode > 2.5) {
            let c = v.local - v.size * 0.5;
            t = clamp(length(c) / max(max(v.size.x, v.size.y) * 0.5, 1.0), 0.0, 1.0);
        }
        col = mix(v.color_b, v.color_a, t);
    }
    return vec4<f32>(col.rgb, col.a * cov);
}
```

**为什么 SDF**：

| 好处 | 说明 |
|---|---|
| 抗锯齿免费 | `fwidth(d)` 就是一像素的屏幕空间宽度，无需额外采样 |
| 模糊免费 | 阴影 = 更宽的 `smoothstep` 带，不用高斯卷积 |
| 任意圆角免费 | 改一个 uniform，不用重画几何 |
| 同一套代码 | 圆角矩形、渐变、阴影、辉光共用一个管线 |

**两条管线、一个绘制列表**（保持 z 序）：

```rust
enum Batch { Sprite, Shape }
struct DrawItem { batch: Batch, first: u32, count: u32 }

// submit() 时按命令顺序追加到对应的顶点缓冲，并记录顺序
self.order.push(DrawItem { batch: Batch::Shape, first, count: 6 });

// end() 时严格按 order 绘制
for item in &self.order {
    match item.batch {
        Batch::Sprite => { pass.set_pipeline(&self.pipeline); /* ... */ }
        Batch::Shape  => { pass.set_pipeline(&self.shape_pipeline); /* ... */ }
    }
}
```

> **不能简单地把形状和精灵分成两批画**：那样所有形状都会在所有精灵之下（或之上），
> z 序就丢了。绘制列表是唯一正确的解法。

### 4.4 字体加载与 HiDPI 适配

**加载**：

```rust
// crates/app/src/main.rs
const FONT_PATH: &str = "assets/font.ttf";
const FONT_SIZE: f32 = 16.0;      // 逻辑单位

fn install_font(&mut self) {
    let Ok(bytes) = std::fs::read(FONT_PATH) else { return };
    if let Some(r) = self.renderer.as_mut() {
        let _ = r.set_font(bytes, FONT_SIZE, self.scale);
    }
}
```

**HiDPI 适配**——字形按 `size × dpr` 栅格化，但**推进宽度以逻辑单位报告**：

```rust
// crates/render/src/font.rs
fn rasterise(&mut self, ch: char) -> PlacedGlyph {
    let px_size = (self.size * self.dpr).round().max(1.0);   // 设备像素
    let scaled = self.font.as_scaled(PxScale::from(px_size));
    ...
    PlacedGlyph {
        rect: (ax, ay, w, h),                                  // 图集纹素
        offset: (bounds.min.x / self.dpr, bounds.min.y / self.dpr),  // 逻辑单位
        advance: advance_dev / self.dpr,                              // 逻辑单位
    }
}
```

**DPR 变化时重栅格化**：

```rust
WindowEvent::ScaleFactorChanged { scale_factor, mut inner_size_writer } => {
    self.scale = scale_factor as f32;
    inner_size_writer.request_inner_size(PhysicalSize::new(
        (self.logical.width * scale_factor as f32) as u32,
        (self.logical.height * scale_factor as f32) as u32,
    ))?;
    if let Some(r) = self.renderer.as_mut() {
        r.set_font_dpr(self.scale);      // ← 文字必须重新栅格化，否则发糊
    }
}
```

**可缩放字体**：字号来自 `TextRole`，再乘用户的偏好系数：

```rust
pub fn text_size(&self, role: TextRole) -> f32 {
    role.size() * self.tokens.text_scale.factor()
}
```

| 偏好 | 系数 | `Body` 实际字号 |
|---|---|---|
| Small | 0.875 | 12.25 |
| Normal | 1.0 | 14 |
| Large | 1.125 | 15.75 |
| ExtraLarge | 1.25 | 17.5 |

### 4.5 HUD 的现代画法

```rust
// crates/app/src/game.rs
pub fn frame(game: &Game, layout: Layout) -> Vec<RenderCmd> {
    let mut cmds = rowline_core::view::build_frame(&game.snapshot(layout));
    let ds = &game.design;
    let p = &ds.palette;

    let panel = Rect::new(layout.panel_x as f32, layout.panel_y as f32,
                          layout.panel_w as f32, layout.panel_h as f32);
    let radius = ds.radius(Radius::Lg);

    // 1) 柔和投影
    cmds.push(RenderCmd::Shadow {
        rect: panel, radius,
        offset: Point::new(0.0, 2.0),
        blur: 12.0,
        color: p.shadow.into(),
    });
    // 2) 面板表面
    cmds.push(RenderCmd::RoundedRect { rect: panel, radius, color: p.surface_alt.into() });
    cmds.push(RenderCmd::Outline { rect: panel, color: p.outline.into() });
    // 3) 顶部主色渐变条
    cmds.push(RenderCmd::Gradient {
        rect: Rect::new(panel.x, panel.y, panel.width, 3.0),
        radius: 0.0,
        from: p.accent.into(), to: p.accent_hover.into(),
        direction: GradientDirection::Horizontal,
    });
    // 4) 分数（等宽数字）
    cmds.push(RenderCmd::Text {
        text: format!("{}", game.engine.score()),
        at: Point::new(panel.x + ds.space(Space::Lg), panel.y + ds.space(Space::Lg)),
        size: ds.text_size(TextRole::Numeric) as u16,
        color: p.text_primary.into(),
    });
    cmds
}
```

### 4.6 主题与偏好的运行时切换

```rust
// crates/app/src/main.rs
if event.state == ElementState::Pressed && self.mods.control {
    match key {
        Key::KeyT => self.game.toggle_theme(),          // 深 / 浅
        Key::KeyM => self.game.toggle_reduced_motion(),  // 减少动效
        Key::KeyE => self.game.cycle_text_scale(),       // 文字大小
        _ => {}
    }
}
```

**这些开关只改表现，不触碰游戏状态**——`Game::toggle_theme` 只换 `design.palette`，
棋盘、分数、规则完全不受影响。

---

## 5. 无障碍

### 5.1 对比度（由构建保证）

`crates/theme/src/contrast.rs` 的单测**断言**：

| 检查 | 阈值 | 覆盖 |
|---|---|---|
| 正文/标题文字 vs 各种表面 | **≥ 4.5:1** | 深浅两主题 × 6 种表面 |
| 主色文字 vs 主色 | **≥ 4.5:1** | 两主题 |
| 球色 vs 棋盘面 | **≥ 3:1**（非文字） | 7 × 2 主题 |
| 两球灰度比 | **≥ 1.10** | 7 × 2 主题 |

```rust
#[test]
fn dark_theme_text_meets_aa() {
    let p = Palette::DARK;
    for (bg, name) in [(p.background, "background"), (p.surface, "surface"), /* ... */] {
        check(&p, p.text_primary, bg, AA_TEXT, &format!("text_primary on {name}"));
        check(&p, p.text_secondary, bg, AA_TEXT, &format!("text_secondary on {name}"));
    }
}
```

> **浅色主题的 `accent` 用 `#9A5B00` 而不是 `#FFC24B`**：
> 琥珀色在白底上的对比度只有 ~1.9:1，深色主题能用它，浅色主题必须加深。
> 这就是为什么配色必须**按用途分主题**，而不是"取反"。

### 5.2 可缩放字体

* 4 档偏好（0.875 / 1.0 / 1.125 / 1.25），运行时切换（`Ctrl+E`），可持久化到 `Settings`。
* 字号统一走 `TextRole`，因此**所有文字同比缩放**，没有遗漏的硬编码尺寸。
* 侧栏是**固定逻辑宽度**，字号放大到 1.25× 仍能容纳（数字用等宽字体，最大 5 位）。

### 5.3 减少动效

* 全局开关 `Motion { reduced }`，所有时长归零，动画直接到达终态。
* 粒子**不生成**（不是"快速播放"）。
* 分数仍会更新，但**不做缩放脉冲**。
* 消除动画直接呈现最终状态。

### 5.4 其他

| 项 | 做法 |
|---|---|
| 色觉友好 | 颜色 + **形状**双通道（原作"使用形状"精灵组天然提供） |
| 键盘可达 | 所有交互有键盘等价物（`F3/F4/F5/F6` 等沿用原作快捷键） |
| 焦点可见 | `outline_strong` 2 px 焦点环 |
| 点击目标 | 最小 36 逻辑 px（`DesignTokens::hit_target`） |
| 不闪 | 没有任何 >3 Hz 的闪烁（WCAG 2.3.1） |
| 语言 | 文案全部走资源层，可 i18n |

---

## 6. 交付清单

| 产物 | 位置 | 状态 |
|---|---|---|
| 设计令牌 crate | `rust/crates/theme/` | ✅ 25 项测试 |
| 深/浅色板 + 对比度断言 | `crates/theme/src/palette.rs`、`contrast.rs` | ✅ |
| 8 pt 栅格 / 圆角 / 阴影 / 字阶 | `crates/theme/src/tokens.rs` | ✅ |
| 缓动曲线 + 时长 + 减少动效 | `crates/theme/src/motion.rs` | ✅ |
| 新图元（圆角/渐变/投影/辉光） | `crates/core/src/render.rs` | ✅ |
| SDF 着色器 + 双管线 + 绘制列表 | `crates/render/src/wgpu_backend.rs` | ✅ |
| 软件后端对应实现 | `crates/render/src/softbuffer_backend.rs` | ✅ |
| 动态字形图集（DPR 栅格化） | `crates/render/src/font.rs` | ✅ |
| 现代 HUD + 主题/动效/字号开关 | `crates/app/src/game.rs`、`main.rs` | ✅ |
| @1x/@2x/@3x/@4x 素材 | `assets/sheet/`、`assets/icons/` | ✅ |

---

## 7. 已知限制

| 限制 | 说明 | 计划 |
|---|---|---|
| ~~图标集尚未绘制~~ | ✅ 已生成 13 个单色图标 + SVG 源，@1x–@4x | 见 §3.1 |
| ~~噪点纹理未生成~~ | ✅ 256² 可平铺且 alpha ≤3% | `tools/make_icons.py` |
| ~~粒子系统未实现~~ | ✅ 纯装饰、不影响判定 | `core::effects` |
| ~~浅色主题的棋盘格底纹未单独出图~~ | ✅ `board_backdrop` / `board_cell` 两套主题各自出图 | §1.2 |
| 侧栏固定宽度 | 字号放到 1.25× 仍够用 | 若超出则改为可折叠 |

---

## 8. 相关文档

| 文档 | 关系 |
|---|---|
| `docs/03_FEATURE_SPEC.md` | 行为规格；本方案只改表现，**不改行为** |
| `docs/05_ART_SPEC.md` | 精灵索引契约；本方案补充**多倍率变体**与**图标/噪点**规范 |
| `docs/09_DISPLAY_DPI_ADAPTATION.md` | 坐标系与 DPI；本方案的所有尺寸都是**逻辑单位** |
| `docs/04_FIDELITY_MATRIX.md` | V-01 决定"布局 1:1、素材自制"，本方案是它的具体化 |
