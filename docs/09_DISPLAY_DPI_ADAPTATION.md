# 09 · 显示适配方案（现代高分辨率屏 / 任意 DPI）

> 状态：**已落地并可编译**。本文档描述的 crate、trait、算法与代码均存在于 `rust/`。
> 适用范围：`docs/03_FEATURE_SPEC.md` 的全部绘制需求 + `docs/05_ART_SPEC.md` 的素材契约。

---

## 0. 总览

```
逻辑坐标 (logical units)                物理像素 (physical pixels)
┌───────────────────────┐              ┌───────────────────────────┐
│ 游戏逻辑 / 布局        │  ×DPR        │ 交换链 (swap chain)        │
│ · 36 px 一格           │ ─────────►   │ · 5760×3840 (300% 4K)     │
│ · 文本 16 px           │              │                           │
│ · 布局公式产出         │              │  content_rect (letterbox) │
└───────────────────────┘              └───────────────────────────┘
         │                                        │
         │ RenderCmd (逻辑)                        │ Viewport
         ▼                                        ▼
   ┌───────────┐   ×content_scale   ┌──────────────────────┐
   │  渲染层    │ ────────────────► │  wgpu / softbuffer    │
   └───────────┘                   └──────────────────────┘
```

**核心原则**：**整个游戏只使用逻辑坐标**；物理像素只在渲染层出现一次转换。

| 层 | 坐标单位 | 谁负责换算 |
|---|---|---|
| `core`（规则/布局/视图） | 逻辑单位 | 无 |
| `display`（视口计算） | 输入物理、输出逻辑+物理矩形 | `compute_viewport` |
| `render`（wgpu/softbuffer） | 逻辑 → 物理 | `content_scale`（唯一换算点） |
| `platform`（winit） | 物理 + scale factor | 上报 `ScaleFactorChanged` |

---

## 1. winit 的 scale_factor 与事件

### 1.1 各平台的来源

| 平台 | `scale_factor` 含义 | 备注 |
|---|---|---|
| **Windows** | 每显示器 DPI 缩放（100/125/150/175/200/250/300%） | 拖到不同显示器会**逐显示器**变化；需 `PerMonitorV2` 感知 |
| **macOS** | Retina = 2.0（整数倍） | 窗口跨 Retina/非 Retina 边界时变化 |
| **Linux** | 分数缩放（X11 的 `Xft.dpi` / Wayland 的 fractional-scale） | 常见 1.0 / 1.25 / 1.5 / 1.75 / 2.0 |
| **Wayland** | 分数缩放由合成器决定 | 应用只拿到 scale factor，不负责缩放 |

### 1.2 事件监听

winit 0.30 的关键点：**修饰键不再挂在 `KeyEvent` 上**，而是独立的 `ModifiersChanged` 事件；
`Window::scale_factor()` 返回 **f64**。

```rust
// crates/app/src/main.rs
WindowEvent::ScaleFactorChanged {
    scale_factor,        // f64
    mut inner_size_writer,
} => {
    let old_logical_w = self.logical.width;
    let old_logical_h = self.logical.height;
    self.scale = scale_factor as f32;

    // winit 0.30 要求写回 **物理** 尺寸：
    //   物理 = 逻辑 × 新的 scale factor
    inner_size_writer.request_inner_size(PhysicalSize::new(
        (old_logical_w * scale_factor as f32) as u32,
        (old_logical_h * scale_factor as f32) as u32,
    ))?;

    let physical = self.window.map(|w| w.inner_size());
    if let Some(s) = physical {
        self.set_physical_size(s.width, s.height);
    }
    // 字形必须按新分辨率重新栅格化，否则文字发糊
    if let Some(r) = self.renderer.as_mut() {
        r.set_font_dpr(self.scale);
    }
}
```

> `request_inner_size` 传的是**物理**尺寸（winit 0.30 的签名是 `PhysicalSize<u32>`）。
> 这一步是"多显示器不同 DPI 下拖动窗口"体验正确的关键：**窗口的视觉大小保持不变**，
> 只是像素密度变了。

---

## 2. 双坐标系

### 2.1 定义

| 名称 | 定义 | 用途 |
|---|---|---|
| **逻辑单位** | 1 逻辑单位 = 1 CSS 像素 | 布局、尺寸、字号、命中测试 |
| **物理像素** | 交换链的实际像素 | GPU、surface 尺寸 |
| **DPR** | `scale_factor` = 物理像素 / 逻辑单位 | 二者换算系数 |
| **content_scale** | 每个逻辑单位占多少物理像素 | 渲染层唯一换算点 |

```
logical_size   = physical_size / scale_factor
content_scale  = scale_factor            （Logical 策略）
physical_px    = logical_px * content_scale
```

### 2.2 视口计算（`crates/display/src/viewport.rs`）

```rust
pub enum ScalePolicy {
    /// 内容铺满窗口；布局按客户区重新计算。
    Logical,
    /// 内容保持固定逻辑分辨率，等比缩放后居中。
    Fixed { width: u32, height: u32, allow_non_integer: bool },
}

pub struct Viewport {
    pub logical_size: Size,      // 客户区，逻辑单位
    pub physical_size: Size,     // 客户区，物理像素
    pub scale_factor: f32,
    pub content_rect: Rect,      // 内容在目标中的位置（物理像素）
    pub content_scale: f32,      // 每个逻辑单位占多少物理像素
    pub letterboxed: bool,       // 是否有黑边
    pub integer_scale: bool,     // content_scale 是否为整数
}

pub fn compute_viewport(policy, physical, scale_factor) -> Viewport
```

**`Logical` 策略**（WinLinez 默认）：

```rust
ScalePolicy::Logical => Viewport {
    logical_size: Size::new(physical.width / sf, physical.height / sf),
    physical_size: physical,
    scale_factor: sf,
    content_rect: Rect::new(0.0, 0.0, physical.width, physical.height),
    content_scale: sf,
    letterboxed: false,
    integer_scale: (sf - sf.round()).abs() < 1e-4,
},
```

**`Fixed` 策略**（像素艺术 / 复古外观）：等比缩放 + 居中 + **整数倍优先**。

```rust
let mut s = (physical.width / fw).min(physical.height / fh);
if !allow_non_integer {
    let f = s.floor();
    if f >= 1.0 { s = f; integer = true; }   // 整数倍，避免非整数采样失真
}
let cw = fw * s;
let ch = fh * s;
let x = (physical.width - cw) * 0.5;   // 居中 → 左右留 pillarbox
let y = (physical.height - ch) * 0.5;  // 上下留 letterbox
```

> **为什么整数倍优先**：非整数缩放的像素艺术会出现**像素宽度不均**（有的 2 px、有的 3 px）
> 和**闪烁**。整数倍时每个源像素严格映射到 N×N 的目标块，边缘锐利。
> `allow_non_integer: true` 留给矢量 UI 或本来就该平滑缩放的场景。

### 2.3 渲染层的唯一换算点

```rust
// crates/render/src/wgpu_backend.rs
fn push_quad(&mut self, x: f32, y: f32, w: f32, h: f32, uv: [f32; 4], tint: Color) {
    let r = self.content_rect;
    let k = self.content_scale;              // ← 唯一的换算系数

    let px = r.x + x * k;                    // 逻辑 → 物理
    let py = r.y + y * k;
    let pw = w * k;
    let ph = h * k;

    // 转成 content_rect 的局部空间，交给着色器做最后的映射
    let x0 = (px - r.x) / r.width;
    let y0 = (py - r.y) / r.height;
    let x1 = (px + pw - r.x) / r.width;
    let y1 = (py + ph - r.y) / r.height;
    ...
}
```

**顶点位置在着色器里映射**（因此 letterbox 逻辑只有一处）：

```wgsl
@vertex fn vs_main(v: VertexIn) -> VertexOut {
    // content-local -> 物理像素 -> clip space
    let px = globals.content_rect.xy + v.pos * globals.content_rect.zw;
    let ndc = vec2<f32>(
        px.x / max(globals.viewport.x, 1.0) * 2.0 - 1.0,
        1.0 - px.y / max(globals.viewport.y, 1.0) * 2.0
    );
    ...
}
```

---

## 3. 渲染策略

### 3.1 矢量 / UI 元素 —— 按物理像素重绘

| 元素 | 做法 |
|---|---|
| 边框、斜角、实心块 | 直接用几何四边形，颜色与尺寸在逻辑单位给出，渲染时乘 `content_scale` → **边缘永远是物理像素整数倍，锐利** |
| 文本 | **按 DPR 栅格化**（见 3.2），字形位图以 1:1 采样 |
| 路径高亮折线 | 逻辑单位定义，渲染时换算；线宽至少 1 物理像素 |

### 3.2 字体：按 scale_factor 栅格化

动态字形图集（`crates/render/src/font.rs`）：

```rust
pub struct FontAtlas {
    font: FontArc,
    size: f32,          // 名义 em 大小，逻辑单位
    dpr: f32,           // 每逻辑单位的设备像素数
    glyphs: HashMap<char, PlacedGlyph>,
    image: RgbaImage,   // 动态增长的图集
    ...
}

/// 关键：字形按 `size * dpr` 栅格化，但**推进宽度以逻辑单位报告**。
fn rasterise(&mut self, ch: char) -> PlacedGlyph {
    let px_size = (self.size * self.dpr).round().max(1.0);
    let scaled = self.font.as_scaled(PxScale::from(px_size));
    ...
    PlacedGlyph {
        rect: (ax, ay, w, h),                       // 图集内纹素
        offset: (bounds.min.x / dpr, bounds.min.y / dpr),  // 逻辑单位
        advance: advance_dev / dpr,                       // 逻辑单位
    }
}
```

* **按需栅格化**：用到的字形才进图集。
* **图集自动增长**：行满换行，高度不够就翻倍并保留内容。
* **DPR 变化时重栅格化**：`set_font_dpr()` 重建图集（`WindowEvent::ScaleFactorChanged` 里调用）。
* 字形四边形的**逻辑尺寸 = 纹素尺寸 / dpr**，于是渲染层的 `content_scale` 把它还原成物理像素，
  整个链条闭环。

### 3.3 位图资产：@1x / @2x / @3x 多倍图 + Lanczos 兜底

**索引契约不变**（`docs/05`），只有**纹素坐标**随倍率放大：

```rust
// crates/core/src/view.rs
pub fn ball_src(value: u8, sprite_set: u8, sheet_scale: u32) -> Rect {
    let s = sheet_scale.max(1) as f32;
    let col = (value as u32) + (sprite_set as u32) * 8;
    Rect::new(col as f32 * 36.0 * s, 225.0 * s, 36.0 * s, 36.0 * s)
}
```

**变体选择**（`crates/display/src/resample.rs`）：

```rust
/// 变体 v 提供 logical_size * v 物理像素，
/// 当 v >= scale_factor 时足够。
pub fn select_variant(available: &[u32], scale_factor: f32) -> Option<u32> {
    let need = scale_factor.ceil().max(1.0) as u32;
    available.iter().copied()
        .filter(|v| *v > 0)
        .filter(|v| *v >= need)
        .min()
        .or_else(|| available.iter().copied().filter(|v| *v > 0).max())
}
```

| DPR | 选用变体 | 结果 |
|---|---|---|
| 100% | `@1x` | 1:1 采样，最锐利 |
| 125% / 150% | `@2x` | 1.33× / 1.6× 超采样 |
| 200% | `@2x` | 1:1 采样 |
| 300% | `@3x` | 1:1 采样 |

**没有足够变体时的兜底**：运行时 **Lanczos-3** 重采样。

```rust
// crates/display/src/resample.rs
pub fn lanczos3_rgba(src: &[u8], src_w: u32, src_h: u32,
                     dst_w: u32, dst_h: u32) -> Vec<u8>
```

* 可分离实现（水平一遍 + 垂直一遍），复杂度 `O(w·h)` 而非 `O(w·h·k)`。
* 权重归一化（`Σw = 1`），有单测断言。
* 比双线性更适合**缩小**（不会出现硬边），比最近邻更适合**放大**（不丢细节）。

**本项目的素材是程序化生成的**（`tools/make_sheet.py`），因此 `@2x`/`@3x` 是**真正的高分辨率重绘**，
而不是放大：

```bash
python3 tools/make_sheet.py
#   -> assets/sheet/rowline_sheet.png       (576×586)
#   -> assets/sheet/rowline_sheet@2x.png   (1152×1172)
#   -> assets/sheet/rowline_sheet@3x.png   (1728×1758)
```

### 3.4 固定逻辑分辨率：letterbox / pillarbox

启用 `ScalePolicy::Fixed` 后：

* 内容按 `min(sx, sy)` 等比缩放，**居中**；
* 剩余空间成为黑边（左右 = pillarbox，上下 = letterbox）；
* `integer_scale = true` 时缩放因子被**向下取整**到整数。

渲染层通过 `content_rect` 把内容限制在黑边内：

```
target 3840×2160 (4K, 200%)
content_rect = 居中的 1920×1080 区域
content_scale = 1.0     ← 注意：这是"内容逻辑单位→物理像素"的比值
```

> **注意区分两个 scale**：
> `scale_factor`（显示器 DPR）与 `content_scale`（内容缩放）在 `Fixed` 策略下**不是同一个数**。
> 前者决定**字体与位图素材**的栅格化倍率，后者决定**内容几何**的缩放。

---

## 4. 完整代码示例

### 4.1 窗口创建

```rust
// crates/app/src/main.rs —— GpuApp::resumed
fn resumed(&mut self, event_loop: &ActiveEventLoop) {
    // 窗口将出现在哪个显示器上？先取它的 scale factor。
    let monitor_scale = event_loop
        .primary_monitor()
        .map(|m| m.scale_factor() as f32)
        .unwrap_or(1.0);
    self.scale = monitor_scale;

    let attrs = Window::default_attributes()
        .with_title("WinLinez")
        // 物理尺寸：逻辑尺寸 × 目标显示器的 DPR
        .with_inner_size(PhysicalSize::new(
            (INITIAL_LOGICAL.0 * monitor_scale as f64) as u32,
            (INITIAL_LOGICAL.1 * monitor_scale as f64) as u32,
        ))
        // 布局至少需要这么大（docs/03 S3）
        .with_min_inner_size(PhysicalSize::new(
            (game::minimum_client_size().width * monitor_scale) as u32,
            (game::minimum_client_size().height * monitor_scale) as u32,
        ));

    let window = event_loop.create_window(attrs).expect("cannot create window");
    let size = window.inner_size();
    self.scale = window.scale_factor() as f32;      // 以窗口实际值为准
    self.set_physical_size(size.width, size.height);

    // 窗口与交换链互相借用，用 Box::leak 取得 'static 生命周期
    // （游戏窗口本来就活到进程结束）。
    let window: &'static Window = Box::leak(Box::new(window));

    let instance = wgpu::Instance::new(
        wgpu::InstanceDescriptor::new_without_display_handle());
    let surface = instance.create_surface(window)?;

    self.renderer = Some(rowline_render::wgpu_backend::WgpuRenderer::new(
        surface, size.width.max(1), size.height.max(1))?);

    self.window = Some(window);
    self.install_font();          // 按当前 DPR 栅格化字形
    self.sync_viewport();
}
```

> **为什么 `Box::leak`**：`wgpu::Surface<'w>` 借用窗口，而窗口又要活到渲染器之后。
> `Box::leak` 一次性把窗口变成 `'static`，避免自引用借用。
> 对游戏外壳是安全的（窗口本来就随进程结束）。

### 4.2 尺寸变化（resize）

```rust
fn set_physical_size(&mut self, width: u32, height: u32) {
    self.physical = Size::new(width as f32, height as f32);
    self.logical  = Size::new(width as f32 / self.scale,
                              height as f32 / self.scale);
    if let Some(r) = self.renderer.as_mut() {
        r.configure_surface(width, height);   // 必须重建 surface 配置
    }
    self.sync_viewport();                      // 重新计算 content_rect
}
```

**交换链必须重新配置**：wgpu 的 `SurfaceConfiguration` 里带着宽高，
尺寸变了不重新 `configure` 就会画到错误的区域。

### 4.3 缩放变化（scale change）

见 §1.2。要点：**写回物理尺寸以保持逻辑尺寸不变**，并**重新栅格化字体**。

### 4.4 surface 格式与 present mode 选择

```rust
// crates/render/src/wgpu_backend.rs
/// 优先 sRGB 表面格式；否则退回第一个可用格式。
pub fn pick_format(formats: &[wgpu::TextureFormat]) -> wgpu::TextureFormat {
    formats.iter().copied()
        .find(|f| f.is_srgb())
        .unwrap_or(formats[0])
}

/// 优先垂直同步；退回 FIFO；再退回第一个可用模式。
pub fn pick_present_mode(modes: &[wgpu::PresentMode]) -> wgpu::PresentMode {
    if modes.contains(&wgpu::PresentMode::AutoVsync) { wgpu::PresentMode::AutoVsync }
    else if modes.contains(&wgpu::PresentMode::Fifo) { wgpu::PresentMode::Fifo }
    else { modes[0] }
}
```

配置：

```rust
surface.configure(&device, &wgpu::SurfaceConfiguration {
    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
    format,                                   // pick_format(&capabilities.formats)
    color_space: wgpu::SurfaceColorSpace::Srgb,
    width: size.width.max(1),
    height: size.height.max(1),
    present_mode,                             // pick_present_mode(&capabilities.present_modes)
    alpha_mode: capabilities.alpha_modes[0],
    view_formats: vec![],
    desired_maximum_frame_latency: 2,
});
```

**为什么 sRGB**：交换链使用 sRGB 格式时，GPU 会自动做线性→sRGB 编码；
用非 sRGB 格式则需要应用自己编码，否则颜色会偏暗。`SurfaceColorSpace::Srgb` 明确了这一点。

**为什么 AutoVsync**：等价于"跟随系统的垂直同步设置"，
Windows 的"动态刷新率"、Wayland 的分数缩放都需要它；不可用时退回 FIFO（必然被支持）。

**重建时的格式保持**：重建 surface 时**必须复用同一个 format 与 present_mode**，
否则会出现"重建后颜色变了"。因此渲染器把两者存了下来：

```rust
pub fn configure_surface(&mut self, width: u32, height: u32) {
    let Some(surface) = self.surface.as_ref() else { return };
    surface.configure(&self.device, &wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: self.surface_format,      // 与创建时一致
        color_space: wgpu::SurfaceColorSpace::Srgb,
        width: width.max(1),
        height: height.max(1),
        present_mode: self.present_mode,   // 与创建时一致
        alpha_mode: self.alpha_mode,
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    });
}
```

### 4.5 视口同步

```rust
fn sync_viewport(&mut self) {
    let vp = compute_viewport(&ScalePolicy::Logical, self.physical, self.scale);
    let rect = Rect::new(vp.content_rect.x, vp.content_rect.y,
                         vp.content_rect.width, vp.content_rect.height);
    if let Some(r) = self.renderer.as_mut() {
        r.set_viewport(rect, vp.content_scale);
    }
}
```

---

## 5. 多显示器不同 DPI 下拖动窗口

### 5.1 完整时序

```
用户把窗口从 100% 显示器拖到 200% 显示器
   │
   ├─ 操作系统改变窗口的 DPI 上下文
   │
   ├─ winit 发出 WindowEvent::ScaleFactorChanged { scale_factor: 2.0, inner_size_writer }
   │
   ├─ 应用：记录新 scale
   │      写回物理尺寸 = 逻辑尺寸 × 2.0   ← 窗口视觉大小不变
   │      重新配置 surface（宽高变了）
   │      重新计算 content_rect
   │      重新栅格化字体（字形必须按 2.0 重新画）
   │
   ├─ 操作系统再发一个 WindowEvent::Resized { 新的物理尺寸 }
   │
   └─ 应用：set_physical_size() → 同步视口 → 正常绘制
```

### 5.2 为什么必须写回尺寸

不写回的话，窗口的**像素**尺寸不变而 DPR 变了，
于是**逻辑尺寸减半**——布局会突然缩小一半，棋盘挤到左上角。
写回 `逻辑 × 新DPR` 后，窗口的**视觉**尺寸保持不变，只有像素密度提高。

### 5.3 全屏切换

```rust
fn toggle_fullscreen(&mut self) {
    let Some(window) = self.window else { return };
    // winit 0.30 没有 `Fullscreen::None`；退出全屏就是 `set_fullscreen(None)`。
    let next = if self.fullscreen { None } else { Some(Fullscreen::Borderless(None)) };
    window.set_fullscreen(next);
    self.fullscreen = !self.fullscreen;
}
```

切换全屏会触发 `Resized`（有时还有 `ScaleFactorChanged`），由上面两条路径统一处理。
用 **Borderless** 而不是 `Exclusive`：独占全屏会改变刷新率与色彩格式，
在多显示器 + 混合 DPI 的环境里体验很差，而且很多平台根本不支持。

---

## 6. 目录树

```
rust/crates/
├── display/                rowline-display    纯显示适配逻辑（只依赖 core）
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── viewport.rs     ScalePolicy · Viewport · compute_viewport
│       └── resample.rs     Lanczos-3 · select_variant
├── core/                   rowline-core       逻辑 + 视图（逻辑坐标）
│   └── src/
│       ├── layout.rs       布局公式（逻辑单位）
│       ├── view.rs         build_frame：逻辑坐标的 RenderCmd
│       └── state.rs        GameState（新增 sheet_scale）
├── platform/               rowline-platform   窗口事件与 scale factor
│   └── src/winit_backend.rs
├── render/                 rowline-render     渲染后端
│   └── src/
│       ├── lib.rs          Renderer trait（新增 set_viewport）
│       ├── wgpu_backend.rs 视口 uniform · surface 重建 · 格式/present mode 选择
│       ├── softbuffer_backend.rs  CPU 参考后端
│       └── font.rs         动态字形图集（DPR 栅格化）
├── assets/                 rowline-assets     素材变体与索引契约
│   └── src/lib.rs          SpriteSheet(scale) · variant_name · upscale_lanczos
├── app/                    rowline-app        外壳：窗口、事件循环、DPI 接线
│   └── src/main.rs
└── tools/                  rowline-tools
    └── src/bin/rowline-dpi-probe.rs   显示环境探针
```

---

## 7. 测试清单

### 7.1 自动化（CI 可跑）

| # | 项目 | 方法 | 通过判据 |
|---|---|---|---|
| A1 | 视口数学：100/125/150/175% | `display::viewport` 单测 | 逻辑尺寸 = 物理 / DPR，精确到 0.01 |
| A2 | 整数倍优先 | `Fixed` 策略单测 | 1.5625 → 1.0；`allow_non_integer` 时保留小数 |
| A3 | letterbox 居中 | 2560×1440 + 640×480 内容 | `content_rect.x = 320`，`letterboxed = true` |
| A4 | 零 DPR 防护 | `scale_factor = 0` | 不除零，退化为 1.0 |
| A5 | Lanczos 权重归一 | `resample` 单测 | `Σw = 1`（误差 < 1e-4） |
| A6 | Lanczos 保留纯色 | 纯色块放大 | 内部像素颜色偏差 ≤ 2 |
| A7 | 变体选择 | 100/125/150/200/300% | 分别选到 @1x/@2x/@2x/@2x/@3x |
| A8 | 精灵索引随倍率缩放 | `core::view` 单测 | @2x 时 `src_x`、`src_y`、尺寸全部 ×2 |
| A9 | 字形按 DPR 栅格化 | 用系统字体实测 | 200% 下字形高度 ≈ 名义尺寸 × 2；推进宽度仍是逻辑单位 |
| A10 | 图集自动增长 | 灌入 1000+ 字形 | 高度自动翻倍，内容保留 |

### 7.2 人工（Windows / macOS / Linux 各一轮）

| # | 场景 | 步骤 | 通过判据 |
|---|---|---|---|
| M1 | **100% 缩放** | 1920×1080 / 100% | 棋盘 36 逻辑 px；精灵 1:1 采样，无插值模糊 |
| M2 | **125% 缩放** | 改显示缩放 125%，重启 | 棋盘视觉大小不变；字形与精灵均无模糊（用 @2x） |
| M3 | **150% 缩放** | 同上 | 同上 |
| M4 | **175% / 200%** | 同上 | 同上；200% 下 @2x 素材 1:1 |
| M5 | **300%（5K）** | 2880×1860 @ 300% | @3x 素材被选中；无放大伪影 |
| M6 | **4K 3840×2160** | 全屏 | 布局按逻辑尺寸重算，棋盘居中，不贴边 |
| M7 | **5K 5120×2880** | 全屏 | 同上 |
| M8 | **超宽 3440×1440** | 窗口化 | 侧栏不与棋盘重叠；棋盘水平居中 |
| M9 | **竖屏 1080×1920** | 窗口化 | 布局不越界（最小客户区 880×892 的钳制生效） |
| M10 | **多显示器异 DPI 拖动** | 100% 屏 ↔ 200% 屏来回拖 5 次 | 窗口**视觉大小不变**；棋盘不跳变；文字不糊；无闪烁 |
| M11 | **窗口 ↔ 全屏切换**（F11） | 反复切换 5 次 | 视口正确重建；黑边/拉伸符合策略；无残影 |
| M12 | **窗口缩放** | 拖边框连续缩放 | 布局实时重算；最小尺寸被强制（880×892 逻辑） |
| M13 | **全屏时拖到另一显示器** | F11 后跨屏拖动 | 自动退出全屏或跟随新显示器；scale 正确更新 |
| M14 | **分数缩放（Linux）** | GNOME 125%/150% | 与整数缩放表现一致；不出现半像素接缝 |
| M15 | **Retina ↔ 非 Retina（macOS）** | 跨屏拖动 | scale 在 1.0 ↔ 2.0 间正确切换；素材变体随之切换 |

### 7.3 诊断工具

```bash
cd rust
cargo run --release -p rowline-tools --features dpi --bin rowline-dpi-probe
```

输出每个显示器的名称、物理尺寸、scale factor，以及**该显示器上游戏会得到的逻辑客户区**
和"是否满足最小尺寸 880×892"：

```
rowline-dpi-probe: display environment
  monitor DELL U2720Q                     3840x2160 px  scale 1.00  at (0, 0)
      -> logical client area 3840 x 2160  (board needs >= 880 x 892: ok)
  monitor DELL U2720Q                     3840x2160 px  scale 1.50  at (3840, 0)
      -> logical client area 2560 x 1440  (board needs >= 880 x 892: ok)
```

### 7.4 与既有验收门的关系

| 门 | 命令 | 与 DPI 的关系 |
|---|---|---|
| M1 布局门 | `build/rowline-layout-check` | 6 种客户区尺寸下布局不越界 |
| M2 四方差分门 | `make test` | 逻辑层不受 DPI 影响，四方必须仍一致 |
| 截图几何 | `tools/blackbox/analyze.py shot` | 测量真实截图的棋盘原点，与公式比对 |

> **新增建议**：用 `analyze.py shot` 对 **125% / 150% / 200%** 三张真实截图各跑一次，
> 验证"逻辑原点相同 → 物理原点按 DPR 等比放大"。这是把显示适配纳入回归的最低成本做法。

---

## 8. 已知限制与后续

| 限制 | 影响 | 计划 |
|---|---|---|
| 文本暂用占位块（无 `text` feature 时） | HUD 不显示分数 | `text` feature 已实现，接 `assets/font.ttf` 即可（`app` 已接线） |
| 无字体文件时静默跳过 | HUD 无字 | 加一个内置位图字体作为兜底 |
| `Fixed` 策略未接入 UI | 只能 `Logical` | 需要一个复古模式开关 |
| Wayland 分数缩放由合成器负责 | 应用内仍是整数坐标 | 逻辑不变，物理尺寸由合成器给 |
| 独占全屏未支持 | 不影响 | 刻意选择 Borderless |
| 素材只到 @3x | 500% 缩放会回退到 Lanczos 放大 | 美术出图时补 @4x |

---

## 9. 相关文档

| 文档 | 关系 |
|---|---|
| `docs/05_ART_SPEC.md` | 精灵索引契约；本方案补充了**多倍率变体**的约定 |
| `docs/03_FEATURE_SPEC.md` S3 / S5 | 布局公式与渲染公式（本方案不改变其语义，只增加坐标系） |
| `docs/08_RUST_ENGINE_DESIGN.md` | 工程结构；本方案是其"显示适配"专章 |
| `tools/blackbox/README.md` | 截图测量与一致性比对的操作流程 |
