use bytemuck::{Pod, Zeroable};
use image::RgbaImage;
use layershellev::WindowWrapper;
use std::collections::HashMap;
use std::sync::Arc;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct CardVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub local_pos: [f32; 2],
    pub card_size: [f32; 2],
    pub border_color: [f32; 4],
    pub border_width: f32,
    pub opacity: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Uniforms {
    pub screen_size: [f32; 2],
    pub _pad: [f32; 2],
    pub bg_color: [f32; 4],
}

pub struct GpuTexture {
    pub texture: wgpu::Texture,
    pub _view: wgpu::TextureView,
    pub bind_group: wgpu::BindGroup,
    pub width: u32,
    pub height: u32,
}

pub struct RenderState {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,

    pub bg_pipeline: wgpu::RenderPipeline,
    pub bg_bind_group: wgpu::BindGroup,

    pub card_pipeline: wgpu::RenderPipeline,
    pub card_group_layout: wgpu::BindGroupLayout,
    pub card_global_bind_group: wgpu::BindGroup,
    pub uniform_buffer: wgpu::Buffer,
    pub card_vertex_buffer: wgpu::Buffer,

    pub overlay_pipeline: wgpu::RenderPipeline,
    pub overlay_group_layout: wgpu::BindGroupLayout,
    pub overlay_sampler: wgpu::Sampler,
    pub overlay_texture: Option<GpuTexture>,

    pub fallback_texture: GpuTexture,
    pub texture_cache: HashMap<usize, GpuTexture>,
}

const BG_SHADER: &str = r#"
struct Uniforms {
    screen_size: vec2<f32>,
    _pad: vec2<f32>,
    bg_color: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(in_vertex_index & 1u) * 4 - 1);
    let y = f32(i32(in_vertex_index & 2u) * 2 - 1);
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let center = vec2<f32>(0.5, 0.5);
    let dist = distance(in.uv, center);
    let vignette = smoothstep(0.15, 0.85, dist);
    let base_rgb = uniforms.bg_color.rgb;
    let edge_rgb = base_rgb * 0.45;
    let rgb = mix(base_rgb, edge_rgb, vignette);
    return vec4<f32>(rgb, uniforms.bg_color.a);
}
"#;

const CARD_SHADER: &str = r#"
struct VertexInput {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) local_pos: vec2<f32>,
    @location(3) card_size: vec2<f32>,
    @location(4) border_color: vec4<f32>,
    @location(5) border_width: f32,
    @location(6) opacity: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) local_pos: vec2<f32>,
    @location(2) card_size: vec2<f32>,
    @location(3) border_color: vec4<f32>,
    @location(4) border_width: f32,
    @location(5) opacity: f32,
};

struct Uniforms {
    screen_size: vec2<f32>,
    _pad: vec2<f32>,
    bg_color: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;
@group(0) @binding(1)
var s_diffuse: sampler;

@group(1) @binding(0)
var t_diffuse: texture_2d<f32>;

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let ndc_x = (model.pos.x / uniforms.screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (model.pos.y / uniforms.screen_size.y) * 2.0;
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = model.uv;
    out.local_pos = model.local_pos;
    out.card_size = model.card_size;
    out.border_color = model.border_color;
    out.border_width = model.border_width;
    out.opacity = model.opacity;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let w = in.card_size.x;
    let h = in.card_size.y;
    let px = in.local_pos.x;
    let py = in.local_pos.y;

    // 75 degree parallelogram:
    // cot(75°) ≈ 0.26794919243
    // sin(75°) ≈ 0.96592582629
    let cot75: f32 = 0.26794919243;
    let sin75: f32 = 0.96592582629;

    let u_slant = px + py * cot75;
    let dist_horizontal = h * 0.5 - abs(py);
    let dist_slanted = (w * 0.5 - abs(u_slant)) * sin75;
    let dist = min(dist_horizontal, dist_slanted);

    if (dist < -0.8) {
        discard;
    }

    let edge_aa = clamp(dist + 0.8, 0.0, 1.0);
    var tex_color = textureSample(t_diffuse, s_diffuse, in.uv);

    let bw = in.border_width;
    var final_rgb = tex_color.rgb;
    if (bw > 0.0) {
        let border_mix = 1.0 - smoothstep(bw - 1.0, bw + 0.5, dist);
        final_rgb = mix(final_rgb, in.border_color.rgb, border_mix * in.border_color.a);
    }

    let alpha = edge_aa * in.opacity * tex_color.a;
    return vec4<f32>(final_rgb, alpha);
}
"#;

const OVERLAY_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(in_vertex_index & 1u) * 4 - 1);
    let y = f32(i32(in_vertex_index & 2u) * 2 - 1);
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@group(0) @binding(0)
var t_overlay: texture_2d<f32>;
@group(0) @binding(1)
var s_overlay: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_overlay, s_overlay, in.uv);
}
"#;

impl RenderState {
    pub async fn new(window: Arc<WindowWrapper>, width: u32, height: u32) -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let surface = instance.create_surface(window).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Failed to find suitable GPU adapter");

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("swall-device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .expect("Failed to create device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let alpha_mode = surface_caps
            .alpha_modes
            .iter()
            .copied()
            .find(|&mode| {
                mode == wgpu::CompositeAlphaMode::PreMultiplied
                    || mode == wgpu::CompositeAlphaMode::PostMultiplied
            })
            .unwrap_or(surface_caps.alpha_modes[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: width.max(1),
            height: height.max(1),
            present_mode: surface_caps.present_modes[0],
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let blend_state = Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniform-buffer"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // 1. Background pipeline
        let bg_shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bg-shader"),
            source: wgpu::ShaderSource::Wgsl(BG_SHADER.into()),
        });

        let bg_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("bg-layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let bg_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg-bind-group"),
            layout: &bg_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let bg_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bg-pipeline-layout"),
            bind_group_layouts: &[&bg_bind_group_layout],
            push_constant_ranges: &[],
        });
        let bg_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bg-pipeline"),
            layout: Some(&bg_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &bg_shader_module,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &bg_shader_module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: blend_state,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // 2. Card pipeline
        let card_shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("card-shader"),
            source: wgpu::ShaderSource::Wgsl(CARD_SHADER.into()),
        });

        let card_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("card-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let card_global_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("card-global-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
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
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let card_global_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("card-global-bind-group"),
            layout: &card_global_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&card_sampler),
                },
            ],
        });

        let card_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("card-group-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });

        let card_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("card-pipeline-layout"),
            bind_group_layouts: &[&card_global_layout, &card_group_layout],
            push_constant_ranges: &[],
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<CardVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    offset: 52,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        };

        let card_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("card-pipeline"),
            layout: Some(&card_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &card_shader_module,
                entry_point: Some("vs_main"),
                buffers: &[vertex_layout],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &card_shader_module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: blend_state,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Vertex buffer for cards (enough for 256 vertices)
        let card_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("card-vertex-buffer"),
            size: (std::mem::size_of::<CardVertex>() * 256) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // 3. Overlay pipeline
        let overlay_shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("overlay-shader"),
            source: wgpu::ShaderSource::Wgsl(OVERLAY_SHADER.into()),
        });

        let overlay_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("overlay-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let overlay_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("overlay-group-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let overlay_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("overlay-pipeline-layout"),
                bind_group_layouts: &[&overlay_group_layout],
                push_constant_ranges: &[],
            });

        let overlay_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("overlay-pipeline"),
            layout: Some(&overlay_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &overlay_shader_module,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &overlay_shader_module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: blend_state,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // 4. Create fallback checkerboard texture
        let mut fallback_img = RgbaImage::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                let c = if ((x / 8) + (y / 8)) % 2 == 0 { 40 } else { 26 };
                fallback_img.put_pixel(x, y, image::Rgba([c, c + 8, c + 18, 255]));
            }
        }
        let fallback_texture = Self::create_texture_from_rgba(
            &device,
            &queue,
            &card_group_layout,
            &fallback_img,
            Some("fallback-texture"),
        );

        Self {
            surface,
            device,
            queue,
            config,
            bg_pipeline,
            bg_bind_group,
            card_pipeline,
            card_group_layout,
            card_global_bind_group,
            uniform_buffer,
            card_vertex_buffer,
            overlay_pipeline,
            overlay_group_layout,
            overlay_sampler,
            overlay_texture: None,
            fallback_texture,
            texture_cache: HashMap::new(),
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn create_texture_from_rgba(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        img: &RgbaImage,
        label: Option<&str>,
    ) -> GpuTexture {
        let (width, height) = (img.width(), img.height());
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label,
            size,
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
            img.as_raw(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            size,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label,
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });

        GpuTexture {
            texture,
            _view: view,
            bind_group,
            width,
            height,
        }
    }

    pub fn update_overlay(&mut self, overlay_img: &RgbaImage) {
        let (w, h) = (overlay_img.width(), overlay_img.height());
        let size = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };

        let needs_recreate = match &self.overlay_texture {
            Some(t) => t.width != w || t.height != h,
            None => true,
        };

        if needs_recreate {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("overlay-texture"),
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
                overlay_img.as_raw(),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * w),
                    rows_per_image: Some(h),
                },
                size,
            );

            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("overlay-bind-group"),
                layout: &self.overlay_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.overlay_sampler),
                    },
                ],
            });

            self.overlay_texture = Some(GpuTexture {
                texture,
                _view: view,
                bind_group,
                width: w,
                height: h,
            });
        } else if let Some(tex) = &self.overlay_texture {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &tex.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                overlay_img.as_raw(),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * w),
                    rows_per_image: Some(h),
                },
                size,
            );
        }
    }
}
