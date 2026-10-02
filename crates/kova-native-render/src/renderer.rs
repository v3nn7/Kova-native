//! wgpu device management and scene rendering.

use crate::atlas::{Atlas, AtlasTextures};
use crate::scene::{Effect, Instance, Op, Scene};
use crate::shaders::ShaderId;
use kova_native_core::{Color, KovaError, KovaResult};
use rustc_hash::FxHashMap;
use std::borrow::Cow;

pub(crate) const SHADER: &str = include_str!("shader.wgsl");
const POST_SHADER: &str = include_str!("post.wgsl");
/// Format of offscreen targets (intermediate scene texture, blur buffers).
const OFFSCREEN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The GPU instance, adapter, device and queue shared by all windows.
pub struct GpuContext {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl GpuContext {
    /// Creates a wgpu instance with Kova Native's native backends (DX12/Vulkan on
    /// Windows, Metal on macOS, Vulkan elsewhere). `WGPU_BACKEND` overrides.
    pub fn create_instance() -> wgpu::Instance {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = wgpu::Backends::from_env().unwrap_or(if cfg!(target_os = "windows") {
            wgpu::Backends::DX12 | wgpu::Backends::VULKAN
        } else {
            wgpu::Backends::PRIMARY
        });
        wgpu::Instance::new(desc)
    }

    /// Picks an adapter (compatible with `surface` if given) and opens a device.
    pub fn new(
        instance: wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
    ) -> KovaResult<GpuContext> {
        let adapter =
            pollster::block_on(
                instance.request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::from_env()
                        .unwrap_or(wgpu::PowerPreference::HighPerformance),
                    force_fallback_adapter: false,
                    compatible_surface: surface,
                    ..Default::default()
                }),
            )
            .map_err(|e| KovaError::Gpu(format!("no suitable GPU adapter: {e}")))?;
        let info = adapter.get_info();
        log::info!(
            "kova-native: using {} ({:?}, {:?})",
            info.name,
            info.backend,
            info.device_type
        );
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("kova-native device"),
            required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .map_err(|e| KovaError::Gpu(format!("failed to open GPU device: {e}")))?;
        device.on_uncaptured_error(std::sync::Arc::new(|e| {
            log::error!("kova-native gpu error: {e}")
        }));
        Ok(GpuContext {
            instance,
            adapter,
            device,
            queue,
        })
    }

    /// Opens a device without any surface (headless rendering, tests).
    pub fn new_headless() -> KovaResult<GpuContext> {
        Self::new(Self::create_instance(), None)
    }
}

/// Presentation configuration of a window surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceOptions {
    /// Synchronize with the display refresh (60/120/144 Hz...).
    pub vsync: bool,
    /// Frames the CPU may queue ahead of the GPU. 1 = lowest input latency.
    pub max_frame_latency: u32,
}

impl Default for SurfaceOptions {
    fn default() -> Self {
        SurfaceOptions {
            vsync: true,
            max_frame_latency: 1,
        }
    }
}

/// A window's swapchain.
pub struct WindowSurface {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}

impl WindowSurface {
    /// Creates a raw surface for a window handle.
    pub fn create_surface(
        instance: &wgpu::Instance,
        window: impl wgpu::DisplayAndWindowHandle + 'static,
    ) -> KovaResult<wgpu::Surface<'static>> {
        instance
            .create_surface(window)
            .map_err(|e| KovaError::Gpu(format!("failed to create surface: {e}")))
    }

    pub fn new(
        gpu: &GpuContext,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
        options: SurfaceOptions,
    ) -> KovaResult<WindowSurface> {
        let caps = surface.get_capabilities(&gpu.adapter);
        if caps.formats.is_empty() {
            return Err(KovaError::Gpu(
                "surface is not supported by the adapter".into(),
            ));
        }
        // Prefer a non-sRGB format: Kova Native blends in sRGB space like browsers.
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| {
                matches!(
                    f,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
                )
            })
            .unwrap_or(caps.formats[0]);
        let present_mode = if options.vsync {
            wgpu::PresentMode::Fifo
        } else {
            [wgpu::PresentMode::Mailbox, wgpu::PresentMode::Immediate]
                .into_iter()
                .find(|m| caps.present_modes.contains(m))
                .unwrap_or(wgpu::PresentMode::Fifo)
        };
        let alpha_mode = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque) {
            wgpu::CompositeAlphaMode::Opaque
        } else {
            caps.alpha_modes[0]
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: width.max(1),
            height: height.max(1),
            desired_maximum_frame_latency: options.max_frame_latency.max(1),
            present_mode,
            alpha_mode,
            view_formats: vec![],
        };
        surface.configure(&gpu.device, &config);
        Ok(WindowSurface { surface, config })
    }

    pub fn resize(&mut self, gpu: &GpuContext, width: u32, height: u32) {
        if width == 0 || height == 0 || (width == self.config.width && height == self.config.height)
        {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&gpu.device, &self.config);
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    /// Acquires the next frame, reconfiguring the swapchain if needed.
    /// Returns `None` when the frame should be skipped (minimized, timeout).
    pub fn acquire(&mut self, gpu: &GpuContext) -> Option<wgpu::SurfaceTexture> {
        for _ in 0..2 {
            match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(t)
                | wgpu::CurrentSurfaceTexture::Suboptimal(t) => return Some(t),
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                    self.surface.configure(&gpu.device, &self.config);
                }
                _ => return None,
            }
        }
        None
    }
}

struct Target {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

impl Target {
    fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        usage: wgpu::TextureUsages,
        label: &str,
    ) -> Target {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: OFFSCREEN_FORMAT,
            usage,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Target { texture, view }
    }
}

/// An offscreen layer target with bind groups to draw from it.
struct LayerTarget {
    target: Target,
    /// Scene bind group sampling this layer as `backdrop_texture`.
    composite_group: wgpu::BindGroup,
    /// Post bind group sampling this layer (backdrop blur inside layers).
    post_group: wgpu::BindGroup,
}

/// Offscreen textures used when a frame contains effects (backdrop blur).
struct EffectTargets {
    size: (u32, u32),
    scene: Target,
    blur_a: Target,
    blur_b: Target,
    /// Post bind groups sampling from `scene`, `blur_a`, `blur_b`.
    from_scene: wgpu::BindGroup,
    from_a: wgpu::BindGroup,
    from_b: wgpu::BindGroup,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    viewport: [f32; 2],
    linear_output: f32,
    _pad: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PostParams {
    texel: [f32; 2],
    direction: [f32; 2],
    sigma: f32,
    radius: f32,
    linear_output: f32,
    _pad: f32,
}

/// Renders [`Scene`]s. One renderer (and one set of atlases) is shared by
/// all windows of an application.
pub struct Renderer {
    shader: wgpu::ShaderModule,
    post_shader: wgpu::ShaderModule,
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: FxHashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    post_bind_group_layout: wgpu::BindGroupLayout,
    post_pipeline_layout: wgpu::PipelineLayout,
    blit_pipelines: FxHashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    downsample_pipeline: wgpu::RenderPipeline,
    blur_pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    post_params: wgpu::Buffer,
    post_stride: u64,
    post_capacity: u64,
    instances: wgpu::Buffer,
    instance_capacity: u64,
    sampler: wgpu::Sampler,
    atlas: Atlas,
    atlas_textures: AtlasTextures,
    dummy_backdrop: Target,
    effects: Option<EffectTargets>,
    bind_group: Option<wgpu::BindGroup>,
    /// Layer targets by nesting depth (all viewport sized).
    layers: Vec<LayerTarget>,
    custom_modules: FxHashMap<ShaderId, wgpu::ShaderModule>,
    custom_pipelines: FxHashMap<(ShaderId, wgpu::TextureFormat), wgpu::RenderPipeline>,
}

fn is_srgb(format: wgpu::TextureFormat) -> bool {
    format.is_srgb()
}

impl Renderer {
    pub fn new(gpu: &GpuContext) -> Renderer {
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("kova-native shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADER)),
        });
        let post_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("kova-native post shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(POST_SHADER)),
        });
        let texture_entry =
            |binding: u32, dim: wgpu::TextureViewDimension| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: dim,
                    multisampled: false,
                },
                count: None,
            };
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("kova-native bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                texture_entry(1, wgpu::TextureViewDimension::D2Array),
                texture_entry(2, wgpu::TextureViewDimension::D2Array),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture_entry(4, wgpu::TextureViewDimension::D2),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("kova-native pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let post_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("kova-native post bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: wgpu::BufferSize::new(
                                std::mem::size_of::<PostParams>() as u64,
                            ),
                        },
                        count: None,
                    },
                    texture_entry(1, wgpu::TextureViewDimension::D2),
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let post_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("kova-native post pipeline layout"),
            bind_group_layouts: &[Some(&post_bind_group_layout)],
            immediate_size: 0,
        });
        let post_stride = (std::mem::size_of::<PostParams>() as u64)
            .next_multiple_of(device.limits().min_uniform_buffer_offset_alignment as u64);
        let post_capacity = 16;
        let post_params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("kova-native post params"),
            size: post_stride * post_capacity,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("kova-native globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instance_capacity = 1024;
        let instances = Self::create_instance_buffer(device, instance_capacity);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("kova-native sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let downsample_pipeline = Self::create_post_pipeline(
            device,
            &post_pipeline_layout,
            &post_shader,
            "fs_downsample",
            OFFSCREEN_FORMAT,
        );
        let blur_pipeline = Self::create_post_pipeline(
            device,
            &post_pipeline_layout,
            &post_shader,
            "fs_blur",
            OFFSCREEN_FORMAT,
        );
        let dummy_backdrop = Target::new(
            device,
            1,
            1,
            wgpu::TextureUsages::TEXTURE_BINDING,
            "kova-native dummy backdrop",
        );
        Renderer {
            shader,
            post_shader,
            bind_group_layout,
            pipeline_layout,
            pipelines: FxHashMap::default(),
            post_bind_group_layout,
            post_pipeline_layout,
            blit_pipelines: FxHashMap::default(),
            downsample_pipeline,
            blur_pipeline,
            globals,
            post_params,
            post_stride,
            post_capacity,
            instances,
            instance_capacity,
            sampler,
            atlas: Atlas::new(),
            atlas_textures: AtlasTextures::new(device),
            dummy_backdrop,
            effects: None,
            bind_group: None,
            layers: Vec::new(),
            custom_modules: FxHashMap::default(),
            custom_pipelines: FxHashMap::default(),
        }
    }

    /// The glyph/image atlas. Painting code inserts tiles here; they are
    /// uploaded to the GPU on the next [`Renderer::render`].
    pub fn atlas(&mut self) -> &mut Atlas {
        &mut self.atlas
    }

    fn create_instance_buffer(device: &wgpu::Device, capacity: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("kova-native instances"),
            size: capacity * std::mem::size_of::<Instance>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn create_post_pipeline(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        module: &wgpu::ShaderModule,
        entry: &str,
        format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(entry),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module,
                entry_point: Some("vs_fullscreen"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    }

    /// Creates an instanced quad pipeline (built-in or custom fragment entry).
    fn create_scene_pipeline(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        module: &wgpu::ShaderModule,
        fragment: &str,
        format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        const ATTRIBUTES: [wgpu::VertexAttribute; 13] = wgpu::vertex_attr_array![
            0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4,
            4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4,
            8 => Float32x4, 9 => Unorm8x4, 10 => Unorm8x4, 11 => Unorm8x4,
            12 => Uint32
        ];
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("kova-native pipeline"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Instance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &ATTRIBUTES,
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: Some(fragment),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    }

    fn pipeline(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        let (shader, layout) = (&self.shader, &self.pipeline_layout);
        self.pipelines.entry(format).or_insert_with(|| {
            Self::create_scene_pipeline(device, layout, shader, "fs_main", format)
        });
    }

    /// Compiles the pipeline of a registered custom shader for `format`.
    fn custom_pipeline(
        &mut self,
        device: &wgpu::Device,
        id: ShaderId,
        format: wgpu::TextureFormat,
    ) {
        if self.custom_pipelines.contains_key(&(id, format)) {
            return;
        }
        let Some((label, source)) = crate::shaders::shader_module_source(id) else {
            log::error!("kova-native: unknown custom shader {id:?}");
            return;
        };
        let module = self.custom_modules.entry(id).or_insert_with(|| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&label),
                source: wgpu::ShaderSource::Wgsl(Cow::Owned(source.to_string())),
            })
        });
        let pipeline =
            Self::create_scene_pipeline(device, &self.pipeline_layout, module, "fs_custom", format);
        self.custom_pipelines.insert((id, format), pipeline);
    }

    /// Makes sure `depth` layer targets of `size` exist with current bind groups.
    fn ensure_layers(&mut self, device: &wgpu::Device, depth: usize, size: (u32, u32)) {
        if self.layers.first().is_some_and(|l| {
            (l.target.texture.width(), l.target.texture.height()) != (size.0.max(1), size.1.max(1))
        }) {
            self.layers.clear();
        }
        while self.layers.len() < depth {
            let usage =
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
            let target = Target::new(device, size.0, size.1, usage, "kova-native layer");
            let composite_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("kova-native layer composite bind group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.globals.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(
                            &self.atlas_textures.mono.view,
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(
                            &self.atlas_textures.color.view,
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(&target.view),
                    },
                ],
            });
            let post_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("kova-native layer post bind group"),
                layout: &self.post_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.post_params,
                            offset: 0,
                            size: wgpu::BufferSize::new(std::mem::size_of::<PostParams>() as u64),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&target.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            self.layers.push(LayerTarget {
                target,
                composite_group,
                post_group,
            });
        }
    }

    fn ensure_effect_targets(&mut self, device: &wgpu::Device, size: (u32, u32)) -> bool {
        if self.effects.as_ref().is_some_and(|e| e.size == size) {
            return false;
        }
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let scene = Target::new(device, size.0, size.1, usage, "kova-native scene target");
        let (bw, bh) = (size.0.div_ceil(2), size.1.div_ceil(2));
        let blur_a = Target::new(device, bw, bh, usage, "kova-native blur a");
        let blur_b = Target::new(device, bw, bh, usage, "kova-native blur b");
        let post_group = |view: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("kova-native post bind group"),
                layout: &self.post_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.post_params,
                            offset: 0,
                            size: wgpu::BufferSize::new(std::mem::size_of::<PostParams>() as u64),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            })
        };
        let from_scene = post_group(&scene.view);
        let from_a = post_group(&blur_a.view);
        let from_b = post_group(&blur_b.view);
        self.effects = Some(EffectTargets {
            size,
            scene,
            blur_a,
            blur_b,
            from_scene,
            from_a,
            from_b,
        });
        true
    }

    fn rebuild_bind_group(&mut self, device: &wgpu::Device) {
        let backdrop = self
            .effects
            .as_ref()
            .map_or(&self.dummy_backdrop.view, |e| &e.blur_a.view);
        self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("kova-native bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.atlas_textures.mono.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.atlas_textures.color.view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(backdrop),
                },
            ],
        }));
    }

    fn ensure_post_capacity(&mut self, device: &wgpu::Device, slots: u64) {
        if slots <= self.post_capacity {
            return;
        }
        self.post_capacity = slots.next_power_of_two();
        self.post_params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("kova-native post params"),
            size: self.post_stride * self.post_capacity,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Post bind groups reference the buffer; force their recreation.
        let size = self.effects.take().map(|e| e.size);
        if let Some(size) = size {
            self.ensure_effect_targets(device, size);
            // The scene bind group also samples an effect texture. Replacing
            // those targets must refresh it along with the post bind groups.
            self.bind_group = None;
        }
    }

    /// Renders `scene` into `target` (a view of a `format` texture of `size`
    /// pixels), clearing it to `clear` first. Submits its own command buffer.
    pub fn render(
        &mut self,
        gpu: &GpuContext,
        scene: &Scene,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        size: (u32, u32),
        clear: Color,
    ) {
        let device = &gpu.device;
        let queue = &gpu.queue;
        let mut rebuild =
            self.atlas_textures.sync(device, queue, &mut self.atlas) || self.bind_group.is_none();

        let ops = scene.ops();
        let captures = ops.iter().filter(|op| matches!(op, Op::Effect(_))).count() as u64;
        let mut depth = 0usize;
        let mut max_depth = 0usize;
        for op in &ops {
            match op {
                Op::PushLayer => {
                    depth += 1;
                    max_depth = max_depth.max(depth);
                }
                Op::PopLayer { .. } => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        let use_effects = captures > 0 || max_depth > 0;
        if use_effects {
            self.ensure_post_capacity(device, captures * 3 + 1);
            rebuild |= self.ensure_effect_targets(device, size);
            rebuild |= self.bind_group.is_none();
        }
        if rebuild {
            self.rebuild_bind_group(device);
            // Layer bind groups reference the atlas and parameter buffers too.
            self.layers.clear();
        }
        if max_depth > 0 {
            self.ensure_layers(device, max_depth, size);
        }

        // Upload instances.
        let count = scene.instances.len() as u64;
        if count > self.instance_capacity {
            self.instance_capacity = count.next_power_of_two();
            self.instances = Self::create_instance_buffer(device, self.instance_capacity);
        }
        if count > 0 {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&scene.instances));
        }
        let draw_format = if use_effects {
            OFFSCREEN_FORMAT
        } else {
            format
        };
        let globals = Globals {
            viewport: [size.0 as f32, size.1 as f32],
            linear_output: if !use_effects && is_srgb(format) {
                1.0
            } else {
                0.0
            },
            _pad: 0.0,
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

        // sRGB targets expect linear clear values.
        let clear_color = if !use_effects && is_srgb(format) {
            let lin = |c: f32| {
                if c <= 0.04045 {
                    c / 12.92
                } else {
                    ((c + 0.055) / 1.055).powf(2.4)
                }
            };
            wgpu::Color {
                r: lin(clear.r) as f64,
                g: lin(clear.g) as f64,
                b: lin(clear.b) as f64,
                a: clear.a as f64,
            }
        } else {
            wgpu::Color {
                r: clear.r as f64,
                g: clear.g as f64,
                b: clear.b as f64,
                a: clear.a as f64,
            }
        };

        self.pipeline(device, draw_format);
        for op in &ops {
            if let Op::Draw {
                shader: Some(id), ..
            } = op
            {
                self.custom_pipeline(device, *id, draw_format);
            }
        }
        let pipeline = &self.pipelines[&draw_format];
        let bind_group = self.bind_group.as_ref().expect("bind group");
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("kova-native frame"),
        });
        let root = match (&self.effects, use_effects) {
            (Some(e), true) => &e.scene.view,
            _ => target,
        };
        let layers = &self.layers;
        let view_at = |depth: usize| {
            if depth == 0 {
                root
            } else {
                &layers[depth - 1].target.view
            }
        };
        let clear_at = |depth: usize| {
            if depth == 0 {
                clear_color
            } else {
                wgpu::Color::TRANSPARENT
            }
        };
        let begin = begin_pass;

        let mut post_slot = 0u64;
        let mut post_writes: Vec<(u64, PostParams)> = Vec::new();
        // `fresh[d]`: target at depth `d` has not been cleared this frame.
        let mut fresh = vec![true];
        for op in &ops {
            let depth = fresh.len() - 1;
            match op {
                Op::Draw { instances, shader } => {
                    let load = if fresh[depth] {
                        wgpu::LoadOp::Clear(clear_at(depth))
                    } else {
                        wgpu::LoadOp::Load
                    };
                    fresh[depth] = false;
                    let pipeline = match shader {
                        None => Some(pipeline),
                        Some(id) => self.custom_pipelines.get(&(*id, draw_format)),
                    };
                    let mut pass = begin(&mut encoder, view_at(depth), load);
                    if let Some(pipeline) = pipeline
                        && !instances.is_empty()
                    {
                        pass.set_pipeline(pipeline);
                        pass.set_bind_group(0, bind_group, &[]);
                        pass.set_vertex_buffer(0, self.instances.slice(..));
                        pass.draw(0..4, instances.clone());
                    }
                }
                Op::Effect(Effect::CaptureBackdrop { blur }) => {
                    if fresh[depth] {
                        fresh[depth] = false;
                        drop(begin(
                            &mut encoder,
                            view_at(depth),
                            wgpu::LoadOp::Clear(clear_at(depth)),
                        ));
                    }
                    if let Some(effects) = &self.effects {
                        let source = if depth == 0 {
                            &effects.from_scene
                        } else {
                            &layers[depth - 1].post_group
                        };
                        Self::encode_blur(
                            &mut encoder,
                            effects,
                            source,
                            &self.downsample_pipeline,
                            &self.blur_pipeline,
                            *blur,
                            self.post_stride,
                            &mut post_slot,
                            &mut post_writes,
                        );
                    }
                }
                Op::PushLayer => fresh.push(true),
                Op::PopLayer { composite } => {
                    if depth == 0 {
                        continue;
                    }
                    let empty = fresh.pop().unwrap_or(true);
                    let parent = depth - 1;
                    if let Some(index) = composite
                        && !empty
                    {
                        let load = if fresh[parent] {
                            wgpu::LoadOp::Clear(clear_at(parent))
                        } else {
                            wgpu::LoadOp::Load
                        };
                        fresh[parent] = false;
                        let mut pass = begin(&mut encoder, view_at(parent), load);
                        pass.set_pipeline(pipeline);
                        pass.set_bind_group(0, &layers[depth - 1].composite_group, &[]);
                        pass.set_vertex_buffer(0, self.instances.slice(..));
                        pass.draw(0..4, *index..*index + 1);
                    }
                }
            }
        }
        if fresh[0] {
            drop(begin(&mut encoder, root, wgpu::LoadOp::Clear(clear_color)));
        }

        if use_effects && let Some(effects) = &self.effects {
            let linear = if is_srgb(format) { 1.0 } else { 0.0 };
            let slot = post_slot;
            post_writes.push((
                slot,
                PostParams {
                    texel: [1.0 / size.0 as f32, 1.0 / size.1 as f32],
                    direction: [0.0; 2],
                    sigma: 0.0,
                    radius: 0.0,
                    linear_output: linear,
                    _pad: 0.0,
                },
            ));
            let post_shader = &self.post_shader;
            let post_layout = &self.post_pipeline_layout;
            let blit = self.blit_pipelines.entry(format).or_insert_with(|| {
                Self::create_post_pipeline(device, post_layout, post_shader, "fs_blit", format)
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("kova-native blit"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(blit);
            pass.set_bind_group(0, &effects.from_scene, &[(slot * self.post_stride) as u32]);
            pass.draw(0..3, 0..1);
        }

        for (slot, params) in post_writes {
            queue.write_buffer(
                &self.post_params,
                slot * self.post_stride,
                bytemuck::bytes_of(&params),
            );
        }
        queue.submit(Some(encoder.finish()));
    }

    #[allow(clippy::too_many_arguments)]
    fn encode_blur(
        encoder: &mut wgpu::CommandEncoder,
        effects: &EffectTargets,
        source: &wgpu::BindGroup,
        downsample: &wgpu::RenderPipeline,
        blur: &wgpu::RenderPipeline,
        blur_radius: f32,
        stride: u64,
        slot: &mut u64,
        writes: &mut Vec<(u64, PostParams)>,
    ) {
        let (w, h) = effects.size;
        let (bw, bh) = (w.div_ceil(2) as f32, h.div_ceil(2) as f32);
        // CSS blur radius = standard deviation; halved at half resolution.
        let sigma = (blur_radius * 0.5).max(0.5);
        let radius = (sigma * 2.5).ceil().min(64.0);
        let steps: [(
            &wgpu::RenderPipeline,
            &wgpu::BindGroup,
            &wgpu::TextureView,
            PostParams,
        ); 3] = [
            (
                downsample,
                source,
                &effects.blur_a.view,
                PostParams {
                    texel: [1.0 / w as f32, 1.0 / h as f32],
                    direction: [0.0; 2],
                    sigma: 0.0,
                    radius: 0.0,
                    linear_output: 0.0,
                    _pad: 0.0,
                },
            ),
            (
                blur,
                &effects.from_a,
                &effects.blur_b.view,
                PostParams {
                    texel: [1.0 / bw, 1.0 / bh],
                    direction: [1.0, 0.0],
                    sigma,
                    radius,
                    linear_output: 0.0,
                    _pad: 0.0,
                },
            ),
            (
                blur,
                &effects.from_b,
                &effects.blur_a.view,
                PostParams {
                    texel: [1.0 / bw, 1.0 / bh],
                    direction: [0.0, 1.0],
                    sigma,
                    radius,
                    linear_output: 0.0,
                    _pad: 0.0,
                },
            ),
        ];
        for (pipeline, bind_group, target, params) in steps {
            writes.push((*slot, params));
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("kova-native blur"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind_group, &[(*slot * stride) as u32]);
            pass.draw(0..3, 0..1);
            *slot += 1;
        }
    }

    /// Renders `scene` offscreen and returns tightly packed RGBA8 pixels
    /// (premultiplied alpha). Used for screenshots and visual tests.
    pub fn render_to_rgba(
        &mut self,
        gpu: &GpuContext,
        scene: &Scene,
        width: u32,
        height: u32,
        clear: Color,
    ) -> Vec<u8> {
        let device = &gpu.device;
        let target = Target::new(
            device,
            width,
            height,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            "kova-native screenshot",
        );
        self.render(
            gpu,
            scene,
            &target.view,
            OFFSCREEN_FORMAT,
            (width, height),
            clear,
        );
        read_texture(gpu, &target.texture, width, height)
    }

    /// Copies a texture's contents back to the CPU as RGBA8.
    pub fn read_texture_rgba(
        &self,
        gpu: &GpuContext,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        read_texture(gpu, texture, width, height)
    }
}

fn begin_pass<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> wgpu::RenderPass<'e> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("kova-native scene pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

fn read_texture(gpu: &GpuContext, texture: &wgpu::Texture, width: u32, height: u32) -> Vec<u8> {
    let device = &gpu.device;
    let padded_row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("kova-native readback"),
        size: (padded_row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("kova-native readback"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    if let Ok(view) = slice.get_mapped_range() {
        for row in 0..height {
            let start = (row * padded_row) as usize;
            pixels.extend_from_slice(&view[start..start + (width * 4) as usize]);
        }
    }
    buffer.unmap();
    pixels
}
