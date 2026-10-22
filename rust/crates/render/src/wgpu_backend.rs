//! The GPU renderer (wgpu).
//!
//! ## Coordinate systems
//!
//! The renderer works in **physical pixels**.  The game hands it commands
//! whose positions are in *logical* units plus a destination size in
//! *physical* pixels, so the conversion happens exactly once, here.
//!
//! Two rectangles matter:
//!
//! * the **target**, i.e. the swap chain, in physical pixels;
//! * the **content rect**, the sub-rectangle of the target the game is drawn
//!   into.  With a letterboxed policy the content rect is smaller than the
//!   target and the rest becomes bars.
//!
//! Vertex positions are therefore stored **relative to the content rect**
//! (0..1) and mapped to clip space by the shader, which keeps the letterbox
//! logic in a single place.
//!
//! ## Two pipelines
//!
//! | pipeline | draws | technique |
//! |---|---|---|
//! | the sprite pipeline | sprites, font glyphs | textured quads |
//! | the shape pipeline | rounded rectangles, gradients, shadows, glows | **signed distance fields**, anti-aliased with `fwidth` |
//!
//! Both pipelines share one vertex buffer but are drawn in **command order**
//! through a draw list, so z-order is preserved.
//!
//! ## Text
//!
//! Text needs a font pipeline; it is drawn as a placeholder box for now.
//! The `text` feature of this crate adds an `ab_glyph`-based rasteriser that
//! renders glyphs at the *device* pixel ratio, so the type stays sharp.

use std::borrow::Cow;

use rowline_core::input::Size;
use rowline_core::render::{Color, FrameStyle, GradientDirection, Rect, RenderCmd};

use crate::Renderer;

/// One textured quad, positioned relative to the content rect.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    /// 0..1 within the content rect.
    pos: [f32; 2],
    uv: [f32; 2],
    tint: [f32; 4],
    /// 0 = the sprite sheet, 1 = the font atlas.
    layer: f32,
}

/// One shape vertex, in the shape's own pixel space.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ShapeVertex {
    /// The pixel offset inside the shape's rectangle (0..w, 0..h).
    local: [f32; 2],
    /// The rectangle's size, in pixels.
    size: [f32; 2],
    /// The rectangle's top-left corner inside the content rect, in physical
    /// pixels.  `local` alone is 0..w and would otherwise put every shape at
    /// the top-left corner.
    origin: [f32; 2],
    /// x = corner radius, y = blur, z = mode, w = is_shadow.
    params: [f32; 4],
    /// The primary colour.
    color_a: [f32; 4],
    /// The secondary colour (gradient end).
    color_b: [f32; 4],
}

/// The uniform block: target size and content rectangle, both in physical px.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    /// The size of the swap chain, in physical pixels.
    viewport: [f32; 2],
    /// WGSL aligns `vec4` to 16 bytes.
    _pad: [f32; 2],
    /// x, y, w, h of the content rectangle, in physical pixels.
    content_rect: [f32; 4],
}

/// One textured quad: where it goes, which texels it samples and with which
/// texture slot.
#[derive(Clone, Copy, Debug)]
struct QuadSpec {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    uv: [f32; 4],
    tint: Color,
    layer: f32,
}

/// The description of one SDF shape.
#[derive(Clone, Copy, Debug)]
struct ShapeSpec {
    rect: Rect,
    radius: f32,
    /// 0 = solid, 1 = vertical, 2 = horizontal, 3 = radial gradient.
    mode: f32,
    /// 0 = shape, 1 = shadow, 2 = glow.
    kind: f32,
    blur: f32,
    color_a: Color,
    color_b: Color,
}

/// Which pipeline a draw item belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Batch {
    Sprite,
    Shape,
}

/// A draw item, recorded so that the passes can be issued in command order.
#[derive(Clone, Copy, Debug)]
struct DrawItem {
    batch: Batch,
    first: u32,
    count: u32,
}

/// The wgpu renderer.
pub struct WgpuRenderer<'w> {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    shape_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    globals_buffer: wgpu::Buffer,
    solid_bind_group: wgpu::BindGroup,
    shape_bind_group: wgpu::BindGroup,
    sheet_bind_group: Option<wgpu::BindGroup>,
    /// The 1x1 white fallback used until a sprite sheet is installed.
    solid_view: wgpu::TextureView,
    /// The font atlas (only with the `text` feature).
    font: Option<crate::font::FontAtlas>,
    /// The sprite sheet's texture view (kept so the bind group can reference it).
    sheet_view: Option<wgpu::TextureView>,
    /// The sprite sheet's size in texels.  Source rectangles are expressed in
    /// texels, so they must be divided by this before sampling.
    sheet_size: (u32, u32),
    /// The current font texture (a 1x1 white placeholder until a font is set).
    font_texture: wgpu::TextureView,
    /// The icon atlas texture (a 1x1 white placeholder until icons are set).
    icon_texture: wgpu::TextureView,
    /// The number of icons in the atlas.
    icon_count: u32,
    /// The font file, kept so that the atlas can be re-rasterised when the
    /// device pixel ratio changes.
    #[cfg(feature = "text")]
    font_bytes: Option<Vec<u8>>,
    vertex_buffer: wgpu::Buffer,
    shape_vertex_buffer: wgpu::Buffer,
    surface: Option<wgpu::Surface<'w>>,
    surface_format: wgpu::TextureFormat,
    present_mode: wgpu::PresentMode,
    alpha_mode: wgpu::CompositeAlphaMode,
    clear_color: Option<Color>,
    verts: Vec<Vertex>,
    shapes: Vec<ShapeVertex>,
    order: Vec<DrawItem>,
    /// The size of the swap chain, in physical pixels.
    size: Size,
    /// The content rectangle, in physical pixels.
    content_rect: Rect,
    /// The physical pixels per logical unit.
    content_scale: f32,
}

impl<'w> WgpuRenderer<'w> {
    /// Create a renderer for the given surface.
    ///
    /// The surface format and present mode are chosen from the adapter's
    /// capabilities: an **sRGB** format is preferred (so that the colours
    /// match what the game intends), and **vsync** is preferred with a
    /// FIFO fallback.
    pub fn new(
        instance: &wgpu::Instance,
        surface: wgpu::Surface<'w>,
        width: u32,
        height: u32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // The surface belongs to `instance`: requesting the adapter from a
        // different instance makes `get_capabilities` / `configure` trip over
        // an `AdapterId` that the surface's instance has never seen.  Asking
        // for an adapter that is compatible with the surface matters on
        // multi-GPU machines, where the first adapter may not be able to
        // present to this window at all.
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))?;

        let capabilities = surface.get_capabilities(&adapter);
        let format = pick_format(&capabilities.formats);
        let present_mode = pick_present_mode(&capabilities.present_modes);
        let alpha_mode = capabilities
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);

        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;

        surface.configure(
            &device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                color_space: wgpu::SurfaceColorSpace::Srgb,
                width: width.max(1),
                height: height.max(1),
                present_mode,
                alpha_mode,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            },
        );

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rowline"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADER)),
        });
        let shape_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rowline-shape"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHAPE_SHADER)),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals+sheet"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // The font atlas lives in the same bind group; the vertex
                // shader picks the texture with a per-vertex layer flag.
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // The icon atlas: a horizontal strip of square cells.
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // The shape pipeline only needs the globals.
        let shape_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("globals"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rowline"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let shape_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("rowline-shape"),
                bind_group_layouts: &[Some(&shape_bind_group_layout)],
                immediate_size: 0,
            });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            // Location 3 is the single `layer` scalar, not a vec4: the
            // shader declares `layer: f32`.
            attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32],
        };
        let shape_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ShapeVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x2, 1 => Float32x2, 2 => Float32x2,
                3 => Float32x4, 4 => Float32x4, 5 => Float32x4
            ],
        };

        // The font atlas and the icon strip carry transparent texels, and the
        // SDF shapes use alpha for their soft edges.  Alpha blending is what
        // keeps those texels from painting opaque black/white boxes.
        let color_target = wgpu::ColorTargetState {
            format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rowline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(vertex_layout)],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(color_target.clone())],
            }),
            multiview_mask: None,
            cache: None,
        });

        let shape_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rowline-shape"),
            layout: Some(&shape_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shape_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(shape_vertex_layout)],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shape_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(color_target)],
            }),
            multiview_mask: None,
            cache: None,
        });

        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let solid_texture = solid_texture(&device, &queue);
        // Until a font is installed the atlas slot points at a 1x1 white
        // texture, so the pipeline is always complete.
        let font_texture = crate::wgpu_backend::solid_texture(&device, &queue);
        // The icon atlas slot starts as a 1x1 white texture.
        let icon_texture = crate::wgpu_backend::solid_texture(&device, &queue);
        let sampler = white_sampler(&device);
        let solid_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals+solid"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&solid_texture),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&font_texture),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&icon_texture),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let shape_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &shape_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vertices"),
            size: 128 * 1024,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shape_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shape-vertices"),
            size: 64 * 1024,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(WgpuRenderer {
            device,
            queue,
            pipeline,
            shape_pipeline,
            bind_group_layout,
            globals_buffer,
            solid_bind_group,
            shape_bind_group,
            sheet_bind_group: None,
            sheet_view: None,
            // The fallback is the 1x1 white texture, used both by
            // `solid_bind_group` and until a sheet is installed.
            solid_view: solid_texture,
            sheet_size: (1, 1),
            font: None,
            font_bytes: None,
            font_texture,
            icon_texture,
            icon_count: 0,
            vertex_buffer,
            shape_vertex_buffer,
            surface: Some(surface),
            surface_format: format,
            present_mode,
            alpha_mode,
            clear_color: None,
            verts: Vec::new(),
            shapes: Vec::new(),
            order: Vec::new(),
            size: Size::new(width as f32, height as f32),
            content_rect: Rect::new(0.0, 0.0, width as f32, height as f32),
            content_scale: 1.0,
        })
    }

    /// The surface format the swap chain was configured with.
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_format
    }

    /// Upload the sprite sheet.
    pub fn set_sheet(&mut self, rgba: &[u8], width: u32, height: u32) {
        let size = wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sheet"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.sheet_view = Some(view);
        self.sheet_size = (width.max(1), height.max(1));
        self.rebuild_bind_group();
    }

    /// Rebuild the single bind group after any of its four textures changes.
    ///
    /// The layout has seven entries: globals, sheet, its sampler, font, its
    /// sampler, icons and their sampler.  Keeping one constructor avoids the
    /// classic "bind group has 5 entries, layout has 7" validation error.
    fn rebuild_bind_group(&mut self) {
        let sampler = white_sampler(&self.device);
        self.sheet_bind_group = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sheet+font+icons"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.globals_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.sheet_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.font_texture),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&self.icon_texture),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        }));
    }

    /// Upload the icon atlas.
    ///
    /// The atlas is a horizontal strip of `count` square cells; `rgba` holds
    /// the whole strip.
    pub fn set_icons(&mut self, rgba: &[u8], width: u32, height: u32, count: u32) {
        if count == 0 || width == 0 || height == 0 {
            return;
        }
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("icons"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.icon_texture = view;
        self.icon_count = count;
        self.rebuild_bind_group();
    }

    /// The number of icons in the atlas.
    pub fn icon_count(&self) -> u32 {
        self.icon_count
    }

    /// The sprite sheet's texture view.
    fn sheet_view(&self) -> wgpu::TextureView {
        self.sheet_view
            .clone()
            .unwrap_or_else(|| self.solid_view.clone())
    }

    /// Install a font.
    ///
    /// `size` is the nominal em size in **logical** units; `dpr` is the
    /// number of device pixels per logical unit, so the glyphs are
    /// rasterised at the display's real resolution.
    #[cfg(feature = "text")]
    pub fn set_font(&mut self, font_bytes: Vec<u8>, size: f32, dpr: f32) -> Result<(), String> {
        self.font = Some(crate::font::FontAtlas::new(font_bytes.clone(), size, dpr)?);
        self.font_bytes = Some(font_bytes);
        self.upload_font();
        Ok(())
    }

    /// Change the device pixel ratio of the installed font.
    ///
    /// The glyphs have to be rasterised again, because their pixel size
    /// follows the display's resolution.
    #[cfg(feature = "text")]
    pub fn set_font_dpr(&mut self, dpr: f32) {
        let Some(bytes) = self.font_bytes.clone() else {
            return;
        };
        let size = self.font.as_ref().map(|f| f.size()).unwrap_or(16.0);
        let _ = self.set_font(bytes, size, dpr);
    }

    /// Re-upload the font atlas after it changed.
    #[cfg(feature = "text")]
    fn upload_font(&mut self) {
        let Some(atlas) = self.font.as_mut() else {
            return;
        };
        let img = atlas.image();
        let (w, h) = (img.width().max(1), img.height().max(1));
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("font"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            img.as_raw(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.font_texture = view;
        self.rebuild_bind_group();
        if let Some(a) = self.font.as_mut() {
            a.clear_dirty();
        }
    }

    /// Re-configure the swap chain for a new size.
    ///
    /// Must be called whenever the surface size changes (a resize, a
    /// monitor change, or a full-screen transition).
    pub fn configure_surface(&mut self, width: u32, height: u32) {
        let Some(surface) = self.surface.as_ref() else {
            return;
        };
        surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: self.surface_format,
                color_space: wgpu::SurfaceColorSpace::Srgb,
                width: width.max(1),
                height: height.max(1),
                present_mode: self.present_mode,
                alpha_mode: self.alpha_mode,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            },
        );
    }

    /// Set where the content is drawn inside the target.
    pub fn set_content_rect_scaled(&mut self, rect: Rect, content_scale: f32) {
        self.content_rect = rect;
        self.content_scale = content_scale;
    }

    /// The physical pixels per logical unit.
    pub fn content_scale(&self) -> f32 {
        self.content_scale
    }

    /// The content rectangle, in physical pixels.
    pub fn content_rect(&self) -> Rect {
        self.content_rect
    }

    /// Push a quad given in **logical** units.
    ///
    /// The logical size is multiplied by `content_scale` to obtain physical
    /// pixels; the result is then expressed relative to the content rect so
    /// that the shader only has to do the final mapping.  This is the single
    /// place where the logical-to-physical conversion happens.
    ///
    /// `layer` selects the texture the fragment shader samples: 0 = sprite
    /// sheet, 1 = font atlas, 2 = icon atlas.
    fn push_quad_layer(&mut self, q: QuadSpec) {
        let QuadSpec {
            x,
            y,
            w,
            h,
            uv,
            tint,
            layer,
        } = q;
        let r = self.content_rect;
        let sx = if r.width > 0.0 { r.width } else { 1.0 };
        let sy = if r.height > 0.0 { r.height } else { 1.0 };
        let k = self.content_scale;
        let px = r.x + x * k;
        let py = r.y + y * k;
        let pw = w * k;
        let ph = h * k;
        let (x0, y0) = ((px - r.x) / sx, (py - r.y) / sy);
        let (x1, y1) = ((px + pw - r.x) / sx, (py + ph - r.y) / sy);
        let t = [
            tint.r as f32 / 255.0,
            tint.g as f32 / 255.0,
            tint.b as f32 / 255.0,
            tint.a as f32 / 255.0,
        ];
        let first = self.verts.len() as u32;
        let mk = |px: f32, py: f32, u: f32, v: f32| Vertex {
            pos: [px, py],
            uv: [u, v],
            tint: t,
            layer,
        };
        push_quad_vertices(
            &mut self.verts,
            mk(x0, y0, uv[0], uv[1]),
            mk(x1, y0, uv[2], uv[1]),
            mk(x0, y1, uv[0], uv[3]),
            mk(x1, y1, uv[2], uv[3]),
        );
        self.order.push(DrawItem {
            batch: Batch::Sprite,
            first,
            count: 6,
        });
    }

    /// Push a sprite-sheet quad.
    fn push_quad(&mut self, x: f32, y: f32, w: f32, h: f32, uv: [f32; 4], tint: Color) {
        self.push_quad_layer(QuadSpec {
            x,
            y,
            w,
            h,
            uv,
            tint,
            layer: 0.0,
        });
    }

    /// Push a quad that samples the **icon atlas**.
    fn push_quad_icon(&mut self, x: f32, y: f32, w: f32, h: f32, uv: [f32; 4], tint: Color) {
        self.push_quad_layer(QuadSpec {
            x,
            y,
            w,
            h,
            uv,
            tint,
            layer: 2.0,
        });
    }

    /// Push a quad that samples the **font atlas** instead of the sheet.
    fn push_quad_font(&mut self, x: f32, y: f32, w: f32, h: f32, uv: [f32; 4], tint: Color) {
        self.push_quad_layer(QuadSpec {
            x,
            y,
            w,
            h,
            uv,
            tint,
            layer: 1.0,
        });
    }

    /// Push a flat, solid rectangle through the shape pipeline.
    ///
    /// Outlines and frame edges must **not** sample the sprite sheet: they
    /// are solid colour, and the sheet may be any atlas.
    fn push_solid(&mut self, rect: Rect, color: Color) {
        self.push_shape(ShapeSpec {
            rect,
            radius: 0.0,
            mode: 0.0,
            kind: 0.0,
            blur: 0.0,
            color_a: color,
            color_b: color,
        });
    }

    /// Push a rounded rectangle (or gradient / shadow / glow).
    ///
    /// The shape is described in its own pixel space so that the fragment
    /// shader can evaluate the signed distance field exactly.
    fn push_shape(&mut self, s: ShapeSpec) {
        let k = self.content_scale;
        // The shape's rectangle in physical pixels.  The vertex carries the
        // *shape-local* pixel coordinates plus the rectangle's origin inside
        // the content rect, so the shader can place it and still evaluate the
        // signed distance field in shape-local space.
        let pw = (s.rect.width * k).max(0.0);
        let ph = (s.rect.height * k).max(0.0);
        if pw <= 0.0 || ph <= 0.0 {
            return;
        }
        let radius_px = (s.radius * k).clamp(0.0, pw.min(ph) * 0.5);

        let ca = [
            s.color_a.r as f32 / 255.0,
            s.color_a.g as f32 / 255.0,
            s.color_a.b as f32 / 255.0,
            s.color_a.a as f32 / 255.0,
        ];
        let cb = [
            s.color_b.r as f32 / 255.0,
            s.color_b.g as f32 / 255.0,
            s.color_b.b as f32 / 255.0,
            s.color_b.a as f32 / 255.0,
        ];
        let params = [radius_px, s.blur * k, s.mode, s.kind];

        let origin = [s.rect.x * k, s.rect.y * k];
        let mk = |lx: f32, ly: f32| ShapeVertex {
            local: [lx, ly],
            size: [pw, ph],
            origin,
            params,
            color_a: ca,
            color_b: cb,
        };
        let first = self.shapes.len() as u32;
        self.shapes.extend_from_slice(&[
            mk(0.0, 0.0),
            mk(pw, 0.0),
            mk(0.0, ph),
            mk(0.0, ph),
            mk(pw, 0.0),
            mk(pw, ph),
        ]);
        self.order.push(DrawItem {
            batch: Batch::Shape,
            first,
            count: 6,
        });
    }
}

impl Renderer for WgpuRenderer<'_> {
    fn begin(&mut self, size: Size, _scale: f32) {
        self.size = size;
        self.verts.clear();
        self.shapes.clear();
        self.order.clear();
    }

    fn submit(&mut self, cmd: RenderCmd) {
        match cmd {
            RenderCmd::Clear(c) => self.clear_color = Some(c),
            RenderCmd::Sprite {
                src,
                dst,
                size,
                tint,
                ..
            } => {
                // `src` is in texels, but `textureSample` wants 0..1.
                let sw = self.sheet_size.0.max(1) as f32;
                let sh = self.sheet_size.1.max(1) as f32;
                self.push_quad(
                    dst.x,
                    dst.y,
                    size.width,
                    size.height,
                    [
                        src.x / sw,
                        src.y / sh,
                        (src.x + src.width) / sw,
                        (src.y + src.height) / sh,
                    ],
                    tint,
                );
            }
            RenderCmd::Outline { rect, color } => {
                self.push_solid(Rect::new(rect.x, rect.y, rect.width, 1.0), color);
                self.push_solid(
                    Rect::new(rect.x, rect.y + rect.height - 1.0, rect.width, 1.0),
                    color,
                );
                self.push_solid(Rect::new(rect.x, rect.y, 1.0, rect.height), color);
                self.push_solid(
                    Rect::new(rect.x + rect.width - 1.0, rect.y, 1.0, rect.height),
                    color,
                );
            }
            RenderCmd::Frame { rect, style } => {
                let (dark, light) = match style {
                    FrameStyle::Sunken => (Color::rgb(128, 128, 128), Color::rgb(255, 255, 255)),
                    FrameStyle::Raised => (Color::rgb(255, 255, 255), Color::rgb(128, 128, 128)),
                    FrameStyle::Flat => (Color::rgb(128, 128, 128), Color::rgb(128, 128, 128)),
                };
                self.push_solid(Rect::new(rect.x, rect.y, rect.width, 1.0), dark);
                self.push_solid(
                    Rect::new(rect.x, rect.y + rect.height - 1.0, rect.width, 1.0),
                    light,
                );
                self.push_solid(Rect::new(rect.x, rect.y, 1.0, rect.height), dark);
                self.push_solid(
                    Rect::new(rect.x + rect.width - 1.0, rect.y, 1.0, rect.height),
                    light,
                );
            }
            RenderCmd::RoundedRect {
                rect,
                radius,
                color,
            } => {
                self.push_shape(ShapeSpec {
                    rect,
                    radius,
                    mode: 0.0,
                    kind: 0.0,
                    blur: 0.0,
                    color_a: color,
                    color_b: color,
                });
            }
            RenderCmd::Gradient {
                rect,
                radius,
                from,
                to,
                direction,
            } => {
                let mode = match direction {
                    GradientDirection::Vertical => 1.0,
                    GradientDirection::Horizontal => 2.0,
                    GradientDirection::Radial => 3.0,
                };
                let _ = direction;
                self.push_shape(ShapeSpec {
                    rect,
                    radius,
                    mode,
                    kind: 0.0,
                    blur: 0.0,
                    color_a: from,
                    color_b: to,
                });
            }
            RenderCmd::Shadow {
                rect,
                radius,
                offset,
                blur,
                color,
            } => {
                let r = Rect::new(
                    rect.x + offset.x,
                    rect.y + offset.y,
                    rect.width,
                    rect.height,
                );
                self.push_shape(ShapeSpec {
                    rect: r,
                    radius,
                    mode: 0.0,
                    kind: 1.0,
                    blur,
                    color_a: color,
                    color_b: color,
                });
            }
            RenderCmd::Glow {
                rect,
                radius,
                blur,
                color,
            } => {
                self.push_shape(ShapeSpec {
                    rect,
                    radius,
                    mode: 0.0,
                    kind: 2.0,
                    blur,
                    color_a: color,
                    color_b: color,
                });
            }
            RenderCmd::Icon {
                index,
                dst,
                size,
                tint,
            } => {
                // The atlas is a horizontal strip of square cells.
                let n = self.icon_count.max(1);
                let u0 = index as f32 / n as f32;
                let u1 = (index + 1) as f32 / n as f32;
                self.push_quad_icon(
                    dst.x,
                    dst.y,
                    size.width,
                    size.height,
                    [u0, 0.0, u1, 1.0],
                    tint,
                );
            }
            RenderCmd::Text {
                text,
                at,
                size: _size,
                color,
            } => {
                // Collect the glyph quads first: the atlas is borrowed
                // mutably while it rasterises.
                let mut quads: Vec<(f32, f32, f32, f32, [f32; 4])> = Vec::new();
                let mut dirty = false;
                if let Some(atlas) = self.font.as_mut() {
                    let dpr = atlas.dpr();
                    let aw = atlas.width as f32;
                    let ah = atlas.height as f32;
                    // `at` is the top-left of the line; the glyph offsets are
                    // measured from the baseline, so the ascent is added.
                    let ascent = atlas.ascent();
                    for (g, pen_x) in atlas.layout(&text) {
                        if g.is_empty() {
                            continue;
                        }
                        let (ax, ay, gw, gh) = g.rect;
                        quads.push((
                            at.x + pen_x + g.offset.0,
                            at.y + g.offset.1 + ascent,
                            gw as f32 / dpr,
                            gh as f32 / dpr,
                            [
                                ax as f32 / aw,
                                ay as f32 / ah,
                                (ax + gw) as f32 / aw,
                                (ay + gh) as f32 / ah,
                            ],
                        ));
                    }
                    dirty = atlas.dirty();
                }
                for (x, y, w, h, uv) in quads {
                    self.push_quad_font(x, y, w, h, uv, color);
                }
                if dirty {
                    self.upload_font();
                }
            }
        }
    }

    fn end(&mut self) {
        let Some(surface) = self.surface.take() else {
            return;
        };
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            // The frame was skipped (occluded / timed out / outdated); try
            // again on the next tick.
            _ => {
                self.surface = Some(surface);
                return;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rowline"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(to_wgpu_color(
                            self.clear_color.unwrap_or(Color::BLACK),
                        )),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if !self.verts.is_empty() {
                let data: &[u8] = bytemuck::cast_slice(&self.verts);
                ensure_vertex_buffer(
                    &mut self.vertex_buffer,
                    &self.device,
                    "vertices",
                    data.len() as u64,
                );
                self.queue.write_buffer(&self.vertex_buffer, 0, data);
            }
            if !self.shapes.is_empty() {
                let data: &[u8] = bytemuck::cast_slice(&self.shapes);
                ensure_vertex_buffer(
                    &mut self.shape_vertex_buffer,
                    &self.device,
                    "shape-vertices",
                    data.len() as u64,
                );
                self.queue.write_buffer(&self.shape_vertex_buffer, 0, data);
            }
            self.queue.write_buffer(
                &self.globals_buffer,
                0,
                bytemuck::bytes_of(&Globals {
                    viewport: [self.size.width, self.size.height],
                    _pad: [0.0; 2],
                    content_rect: [
                        self.content_rect.x,
                        self.content_rect.y,
                        self.content_rect.width,
                        self.content_rect.height,
                    ],
                }),
            );

            // Draw in command order so that z-order is preserved.
            for item in &self.order {
                match item.batch {
                    Batch::Sprite => {
                        pass.set_pipeline(&self.pipeline);
                        let bg = self
                            .sheet_bind_group
                            .as_ref()
                            .unwrap_or(&self.solid_bind_group);
                        pass.set_bind_group(0, bg, &[]);
                        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                        pass.draw(item.first..item.first + item.count, 0..1);
                    }
                    Batch::Shape => {
                        pass.set_pipeline(&self.shape_pipeline);
                        pass.set_bind_group(0, &self.shape_bind_group, &[]);
                        pass.set_vertex_buffer(0, self.shape_vertex_buffer.slice(..));
                        pass.draw(item.first..item.first + item.count, 0..1);
                    }
                }
            }
        }
        self.queue.submit(Some(enc.finish()));
        self.queue.present(frame);
        self.surface = Some(surface);
    }

    fn size(&self) -> Size {
        self.size
    }

    fn set_viewport(&mut self, rect: Rect, content_scale: f32) {
        self.content_rect = rect;
        self.content_scale = content_scale;
    }
}

/// Prefer an sRGB surface format; fall back to the first available one.
pub fn pick_format(formats: &[wgpu::TextureFormat]) -> wgpu::TextureFormat {
    formats
        .iter()
        .copied()
        .find(|f| f.is_srgb())
        .unwrap_or_else(|| {
            formats
                .first()
                .copied()
                .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb)
        })
}

/// Prefer vsync; fall back to FIFO, then to whatever is available.
pub fn pick_present_mode(modes: &[wgpu::PresentMode]) -> wgpu::PresentMode {
    if modes.contains(&wgpu::PresentMode::AutoVsync) {
        wgpu::PresentMode::AutoVsync
    } else if modes.contains(&wgpu::PresentMode::Fifo) {
        wgpu::PresentMode::Fifo
    } else {
        modes.first().copied().unwrap_or(wgpu::PresentMode::Fifo)
    }
}

fn to_wgpu_color(c: Color) -> wgpu::Color {
    wgpu::Color {
        r: c.r as f64 / 255.0,
        g: c.g as f64 / 255.0,
        b: c.b as f64 / 255.0,
        a: c.a as f64 / 255.0,
    }
}

fn solid_texture(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("solid"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[255u8, 255, 255, 255],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Grow a vertex buffer when the frame needs more room than it has.
///
/// The command count depends on the window size and on the number of
/// outlined cells, so a fixed allocation will eventually overflow.  Growing
/// to the next power of two keeps the reallocations amortised.
fn ensure_vertex_buffer(
    buffer: &mut wgpu::Buffer,
    device: &wgpu::Device,
    label: &str,
    required: u64,
) {
    let needed = required.next_power_of_two().max(64 * 1024);
    if needed <= buffer.size() {
        return;
    }
    *buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: needed,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
}

/// Indices of the six vertices that make a textured quad from its corners in
/// top-left, top-right, bottom-left, bottom-right order.
///
/// The split is `(TL, TR, BL)` + `(TR, BR, BL)`.  The tempting
/// `(TL, BL, BR)` second triangle does not tile the quad: it leaves an
/// unfilled wedge along the right edge, which shows up as a bitten-out ball
/// and clipped glyphs.
const QUAD_INDICES: [usize; 6] = [0, 1, 2, 1, 3, 2];

fn push_quad_vertices(verts: &mut Vec<Vertex>, a: Vertex, b: Vertex, c: Vertex, d: Vertex) {
    let corners = [a, b, c, d];
    verts.extend(QUAD_INDICES.iter().map(|&i| corners[i]));
}

fn white_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("linear"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    })
}

const SHADER: &str = r#"
struct Globals {
    // Size of the swap chain, in physical pixels.
    viewport: vec2<f32>,
    // x, y, w, h of the content rectangle, in physical pixels.
    content_rect: vec4<f32>,
};
@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var sheet: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;
@group(0) @binding(3) var font_tex: texture_2d<f32>;
@group(0) @binding(4) var font_samp: sampler;
@group(0) @binding(5) var icon_tex: texture_2d<f32>;
@group(0) @binding(6) var icon_samp: sampler;

struct VertexIn {
    // 0..1 within the content rectangle.
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) tint: vec4<f32>,
    @location(3) layer: f32,
};
struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
    @location(2) layer: f32,
};

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    var out: VertexOut;
    // content-local -> physical pixels -> clip space
    let px = globals.content_rect.xy + v.pos * globals.content_rect.zw;
    let ndc = vec2<f32>(
        px.x / max(globals.viewport.x, 1.0) * 2.0 - 1.0,
        1.0 - px.y / max(globals.viewport.y, 1.0) * 2.0
    );
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = v.uv;
    out.tint = v.tint;
    out.layer = v.layer;
    return out;
}

@fragment
fn fs_main(v: VertexOut) -> @location(0) vec4<f32> {
    // layer 0 -> the sprite sheet, 1 -> the font atlas, 2 -> the icon atlas.
    if (v.layer > 1.5) {
        return textureSample(icon_tex, icon_samp, v.uv) * v.tint;
    }
    if (v.layer > 0.5) {
        return textureSample(font_tex, font_samp, v.uv) * v.tint;
    }
    return textureSample(sheet, samp, v.uv) * v.tint;
}
"#;

/// The shape pipeline: rounded rectangles, gradients, shadows and glows.
///
/// Everything is evaluated as a **signed distance field**, which gives
/// resolution-independent anti-aliasing for free and makes the blur of a
/// shadow a single parameter.
const SHAPE_SHADER: &str = r#"
struct Globals {
    viewport: vec2<f32>,
    content_rect: vec4<f32>,
};
@group(0) @binding(0) var<uniform> globals: Globals;

struct VertexIn {
    // The pixel offset inside the shape's rectangle.
    @location(0) local: vec2<f32>,
    // The rectangle's size, in pixels.
    @location(1) size: vec2<f32>,
    // The rectangle's top-left corner inside the content rect.
    @location(2) origin: vec2<f32>,
    // x = corner radius, y = blur, z = mode, w = kind.
    @location(3) params: vec4<f32>,
    @location(4) color_a: vec4<f32>,
    @location(5) color_b: vec4<f32>,
};
struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) params: vec4<f32>,
    @location(3) color_a: vec4<f32>,
    @location(4) color_b: vec4<f32>,
};

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    var out: VertexOut;
    let px = globals.content_rect.xy + v.origin + v.local;
    let ndc = vec2<f32>(
        px.x / max(globals.viewport.x, 1.0) * 2.0 - 1.0,
        1.0 - px.y / max(globals.viewport.y, 1.0) * 2.0
    );
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.local = v.local;
    out.size = v.size;
    out.params = v.params;
    out.color_a = v.color_a;
    out.color_b = v.color_b;
    return out;
}

// Signed distance to a rounded box centred on the origin.
fn sd_round_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(v: VertexOut) -> @location(0) vec4<f32> {
    let radius = v.params.x;
    let blur = max(v.params.y, 0.001);
    let mode = v.params.z;
    let kind = v.params.w;

    let half = v.size * 0.5;
    let p = v.local - half;
    let d = sd_round_box(p, half, radius);

    // kind 1 = shadow (soft), kind 2 = glow (soft, outside)
    if (kind > 0.5) {
        let a = 1.0 - smoothstep(-blur, blur, d);
        let alpha = select(a, a * a, kind > 1.5);
        return vec4<f32>(v.color_a.rgb, v.color_a.a * alpha);
    }

    // Anti-alias using the screen-space derivative of the distance field.
    let aa = max(fwidth(d), 0.0001);
    let cov = 1.0 - smoothstep(-aa, aa, d);

    var col = v.color_a;
    if (mode > 0.5) {
        // mode 1 = vertical, 2 = horizontal, 3 = radial.
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
"#;

#[cfg(test)]
mod tests {
    use super::QUAD_INDICES;

    fn edge(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
        (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
    }

    fn inside(p: [f32; 2], tri: [[f32; 2]; 3]) -> bool {
        let d1 = edge(p, tri[0], tri[1]);
        let d2 = edge(p, tri[1], tri[2]);
        let d3 = edge(p, tri[2], tri[0]);
        (d1 >= -1e-5 && d2 >= -1e-5 && d3 >= -1e-5) || (d1 <= 1e-5 && d2 <= 1e-5 && d3 <= 1e-5)
    }

    #[test]
    fn the_quad_triangles_cover_every_point() {
        let corners = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
        let triangles: Vec<[[f32; 2]; 3]> = QUAD_INDICES
            .chunks(3)
            .map(|t| [corners[t[0]], corners[t[1]], corners[t[2]]])
            .collect();
        for i in 0..=32 {
            for j in 0..=32 {
                let p = [i as f32 / 32.0, j as f32 / 32.0];
                assert!(
                    triangles.iter().any(|t| inside(p, *t)),
                    "point {p:?} is not covered"
                );
            }
        }
    }
}
