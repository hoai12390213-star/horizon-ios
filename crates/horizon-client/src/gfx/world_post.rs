//! Resolution scale and FXAA (Horizon settings): the world renders into its
//! own target at the scaled size, then one full-screen pass copies or
//! anti-aliases it onto the screen, under the hand and HUD. With neither on,
//! the world renders straight to the screen.
use std::cell::Cell;
use wgpu::util::DeviceExt;

struct WorldTarget {
    size: [u32; 2],
    color: wgpu::TextureView,
    bloom_mips: Vec<wgpu::TextureView>,
    bloom_mip_groups: Vec<wgpu::BindGroup>,
    taa_history: Vec<wgpu::TextureView>,
    taa_input_groups: Vec<wgpu::BindGroup>,
    taa_copy_groups: Vec<wgpu::BindGroup>,
    water_mask: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
    scratch: Option<wgpu::TextureView>,
    group: wgpu::BindGroup,
}

pub struct Post {
    layout: wgpu::BindGroupLayout,
    mip_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    settings: wgpu::Buffer,
    exposure: wgpu::Buffer,
    environment: wgpu::Buffer,
    history_fallback: wgpu::TextureView,
    water_mask_fallback: wgpu::TextureView,
    exposure_pipeline: wgpu::ComputePipeline,
    mip_pipeline: wgpu::RenderPipeline,
    bloom_extract_pipeline: wgpu::RenderPipeline,
    makeup_taa: wgpu::RenderPipeline,
    makeup_fxaa_taa: wgpu::RenderPipeline,
    makeup_chroma_taa: wgpu::RenderPipeline,
    makeup_fxaa_chroma_taa: wgpu::RenderPipeline,
    copy: wgpu::RenderPipeline,
    fxaa: wgpu::RenderPipeline,
    dof: wgpu::RenderPipeline,
    fxaa_dof: wgpu::RenderPipeline,
    color_blind: wgpu::RenderPipeline,
    fxaa_color_blind: wgpu::RenderPipeline,
    makeup_pipeline: wgpu::RenderPipeline,
    makeup_fxaa: wgpu::RenderPipeline,
    makeup_chroma: wgpu::RenderPipeline,
    makeup_fxaa_chroma: wgpu::RenderPipeline,
    chroma: wgpu::RenderPipeline,
    fxaa_chroma: wgpu::RenderPipeline,
    format: wgpu::TextureFormat,
    /// World resolution relative to the window (0.5..=2.0).
    pub scale: f32,
    pub antialias: bool,
    /// Enable the first MakeUp-inspired WGSL effects.
    pub makeup_enabled: bool,
    /// Temporal accumulation with camera reprojection and depth rejection.
    pub temporal_aa_enabled: bool,
    pub water_reflections_enabled: bool,
    /// Enable MakeUp's optional chromatic aberration.
    pub chroma_enabled: bool,
    /// Enable MakeUp's depth-of-field effect.
    pub dof_enabled: bool,
    pub volumetric_light_enabled: bool,
    pub volumetric_clouds_enabled: bool,
    pub contact_shadows_enabled: bool,
    contact_strength: f32,
    pub motion_blur_enabled: bool,
    focus_distance: f32,
    bloom_strength: f32,
    ao_strength: f32,
    color_blind_mode: u32,
    taa_write_index: Cell<usize>,
    taa_history_valid: Cell<bool>,
    target: Option<WorldTarget>,
}

impl Post {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Horizon post"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../world_post.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Horizon post bindings"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let mip_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("MakeUp bloom mip bindings"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Horizon post"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let mip_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("MakeUp bloom mip pipeline"),
            bind_group_layouts: &[Some(&mip_layout)],
            ..Default::default()
        });
        let pipeline = |entry: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Horizon post"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_post"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
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
        };
        let taa_pipeline = |entry: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("MakeUp TAA history"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_post"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Horizon post"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bloom_pipeline = |entry, label| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&mip_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_post"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
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
        };
        let mip_pipeline = bloom_pipeline("fs_downsample", "MakeUp bloom downsample");
        let bloom_extract_pipeline = bloom_pipeline("fs_bloom_extract", "MakeUp bloom extract");
        let exposure = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("MakeUp adapted exposure"),
            contents: bytemuck::cast_slice(&[1.0_f32]),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let exposure_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("MakeUp auto exposure"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("cs_exposure"),
            compilation_options: Default::default(),
            cache: None,
        });
        let environment = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("MakeUp sun and atmosphere"),
            contents: bytemuck::cast_slice(&[0.0_f32; 52]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let settings = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Horizon MakeUp post settings"),
            contents: bytemuck::cast_slice(&[
                0.0_f32, 16.0, 0.0, 0.4, 0.7, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            ]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let history_fallback_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("MakeUp empty TAA history"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let history_fallback = history_fallback_texture.create_view(&Default::default());
        let water_mask_fallback_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("MakeUp empty water mask"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let water_mask_fallback = water_mask_fallback_texture.create_view(&Default::default());
        Self {
            copy: pipeline("fs_copy"),
            fxaa: pipeline("fs_fxaa"),
            dof: pipeline("fs_dof"),
            fxaa_dof: pipeline("fs_fxaa_dof"),
            color_blind: pipeline("fs_color_blind"),
            fxaa_color_blind: pipeline("fs_fxaa_color_blind"),
            makeup_pipeline: pipeline("fs_makeup"),
            makeup_fxaa: pipeline("fs_makeup_fxaa"),
            makeup_chroma: pipeline("fs_makeup_chroma"),
            makeup_fxaa_chroma: pipeline("fs_makeup_fxaa_chroma"),
            makeup_taa: taa_pipeline("fs_makeup_taa"),
            makeup_fxaa_taa: taa_pipeline("fs_makeup_fxaa_taa"),
            makeup_chroma_taa: taa_pipeline("fs_makeup_chroma_taa"),
            makeup_fxaa_chroma_taa: taa_pipeline("fs_makeup_fxaa_chroma_taa"),
            chroma: pipeline("fs_chroma"),
            fxaa_chroma: pipeline("fs_fxaa_chroma"),
            layout,
            sampler,
            settings,
            exposure,
            environment,
            history_fallback,
            water_mask_fallback,
            exposure_pipeline,
            mip_pipeline,
            bloom_extract_pipeline,
            mip_layout,
            format,
            scale: 1.0,
            antialias: false,
            makeup_enabled: false,
            temporal_aa_enabled: false,
            water_reflections_enabled: false,
            chroma_enabled: false,
            dof_enabled: false,
            volumetric_light_enabled: false,
            volumetric_clouds_enabled: false,
            contact_shadows_enabled: false,
            contact_strength: 0.45,
            motion_blur_enabled: false,
            focus_distance: 16.0,
            bloom_strength: 0.4,
            ao_strength: 0.7,
            color_blind_mode: 0,
            taa_write_index: Cell::new(0),
            taa_history_valid: Cell::new(false),
            target: None,
        }
    }

    /// The world's colour and depth targets this frame, or `None` to draw
    /// it straight to the screen.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        window: [u32; 2],
        always: bool,
    ) -> Option<(wgpu::TextureView, wgpu::TextureView)> {
        if self.scale == 1.0
            && !self.antialias
            && !self.makeup_enabled
            && !self.chroma_enabled
            && !self.dof_enabled
            && !self.volumetric_light_enabled
            && !self.volumetric_clouds_enabled
            && !self.contact_shadows_enabled
            && !self.motion_blur_enabled
            && self.color_blind_mode == 0
            && !always
        {
            self.target = None;
            return None;
        }
        let size = window.map(|v| ((v as f32 * self.scale).round() as u32).clamp(1, 8192));
        let maximum_dimension = size[0].max(size[1]);
        let taa_active = self.makeup_enabled && self.temporal_aa_enabled;
        let water_active = self.makeup_enabled && self.water_reflections_enabled;
        let bloom_level_count = if self.makeup_enabled && self.bloom_strength > 0.001 {
            (u32::BITS - maximum_dimension.leading_zeros()).min(6)
        } else {
            1
        };
        // Only the shader-pack path needs an intermediate before its own
        // post-processing. Standalone MakeUp renders directly to the screen.
        let needs_scratch = always && self.makeup_effects_enabled();
        if self.target.as_ref().is_none_or(|t| {
            t.size != size
                || t.scratch.is_some() != needs_scratch
                || t.bloom_mips.len() + 1 != bloom_level_count as usize
                || t.taa_history.len() != if taa_active { 2 } else { 0 }
                || t.water_mask.is_some() != water_active
        }) {
            let extent = wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            };
            let make_tex = |label, format, usage, mip_level_count| {
                device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: extent,
                    mip_level_count,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
            };
            let color_texture = make_tex(
                "Scaled world",
                self.format,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                bloom_level_count,
            );
            let color = color_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("Scaled world color attachment"),
                base_mip_level: 0,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let sample_color = color_texture.create_view(&Default::default());
            let bloom_mips = (1..bloom_level_count)
                .map(|level| {
                    color_texture.create_view(&wgpu::TextureViewDescriptor {
                        label: Some("MakeUp bloom mip"),
                        base_mip_level: level,
                        mip_level_count: Some(1),
                        ..Default::default()
                    })
                })
                .collect::<Vec<_>>();
            let bloom_mip_groups = bloom_mips
                .iter()
                .enumerate()
                .map(|(index, _)| {
                    let source = if index == 0 {
                        &color
                    } else {
                        &bloom_mips[index - 1]
                    };
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("MakeUp bloom downsample source"),
                        layout: &self.mip_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(source),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&self.sampler),
                            },
                            wgpu::BindGroupEntry {
                                binding: 4,
                                resource: self.exposure.as_entire_binding(),
                            },
                        ],
                    })
                })
                .collect();
            // TEXTURE_BINDING: occlusion culling reads it.
            let depth = make_tex(
                "Scaled world depth",
                wgpu::TextureFormat::Depth32Float,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                1,
            )
            .create_view(&Default::default());
            let scratch = needs_scratch.then(|| {
                make_tex(
                    "MakeUp post intermediate",
                    self.format,
                    wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    1,
                )
                .create_view(&Default::default())
            });
            let taa_history = if taa_active {
                (0..2)
                    .map(|_| {
                        make_tex(
                            "MakeUp TAA history",
                            wgpu::TextureFormat::Rgba16Float,
                            wgpu::TextureUsages::RENDER_ATTACHMENT
                                | wgpu::TextureUsages::TEXTURE_BINDING,
                            1,
                        )
                        .create_view(&Default::default())
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let water_mask = water_active.then(|| {
                make_tex(
                    "MakeUp water surface mask",
                    wgpu::TextureFormat::R8Unorm,
                    wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    1,
                )
                .create_view(&Default::default())
            });
            let water_mask_view = water_mask.as_ref().unwrap_or(&self.water_mask_fallback);
            let taa_input_groups = (0..taa_history.len())
                .map(|write_index| {
                    self.bind(
                        device,
                        &sample_color,
                        &depth,
                        &taa_history[1 - write_index],
                        water_mask_view,
                    )
                })
                .collect();
            let taa_copy_groups = taa_history
                .iter()
                .map(|history| {
                    self.bind(
                        device,
                        history,
                        &depth,
                        &self.history_fallback,
                        water_mask_view,
                    )
                })
                .collect();
            let group = self.bind(
                device,
                &sample_color,
                &depth,
                &self.history_fallback,
                water_mask_view,
            );
            self.target = Some(WorldTarget {
                size,
                color,
                bloom_mips,
                bloom_mip_groups,
                taa_history,
                taa_input_groups,
                taa_copy_groups,
                water_mask,
                depth,
                scratch,
                group,
            });
            self.taa_write_index.set(0);
            self.taa_history_valid.set(false);
            queue.write_buffer(&self.settings, 44, bytemuck::cast_slice(&[0.0_f32]));
        }
        self.target
            .as_ref()
            .map(|t| (t.color.clone(), t.depth.clone()))
    }

    pub fn size(&self) -> Option<[u32; 2]> {
        self.target.as_ref().map(|t| t.size)
    }

    pub fn water_mask_view(&self) -> Option<&wgpu::TextureView> {
        self.target.as_ref()?.water_mask.as_ref()
    }

    pub fn makeup_effects_enabled(&self) -> bool {
        self.makeup_enabled
            || self.chroma_enabled
            || self.dof_enabled
            || self.volumetric_light_enabled
            || self.volumetric_clouds_enabled
            || self.contact_shadows_enabled
            || self.motion_blur_enabled
            || self.color_blind_mode != 0
    }

    pub fn finish_for_pack(
        &self,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Option<&wgpu::TextureView> {
        let target = self.target.as_ref()?;
        let scratch = target.scratch.as_ref()?;
        self.finish_to(encoder, scratch, &target.group);
        Some(scratch)
    }

    /// Bindings that copy (or anti-alias) `view` onto the screen in `finish_with`.
    pub fn bind(
        &self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        history: &wgpu::TextureView,
        water_mask: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Horizon post"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.settings.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.exposure.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: self.environment.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(history),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(water_mask),
                },
            ],
        })
    }

    pub fn set_effect_settings(
        &mut self,
        queue: &wgpu::Queue,
        enabled: bool,
        focus: f32,
        bloom_strength: f32,
        ao_strength: f32,
        volumetric_light: bool,
        volumetric_clouds: bool,
        temporal_aa: bool,
        water_reflections: bool,
        contact_shadows: bool,
        contact_strength: f32,
        motion_blur: bool,
        color_blind_mode: u32,
    ) {
        if self.temporal_aa_enabled != temporal_aa {
            self.taa_history_valid.set(false);
        }
        self.temporal_aa_enabled = temporal_aa;
        self.water_reflections_enabled = water_reflections;
        self.dof_enabled = enabled;
        self.volumetric_light_enabled = volumetric_light;
        self.volumetric_clouds_enabled = volumetric_clouds;
        self.contact_shadows_enabled = contact_shadows;
        self.contact_strength = contact_strength.clamp(0.0, 1.0);
        self.motion_blur_enabled = motion_blur;
        self.focus_distance = focus.clamp(2.0, 64.0);
        self.bloom_strength = bloom_strength.clamp(0.0, 1.0);
        self.ao_strength = ao_strength.clamp(0.0, 1.0);
        self.color_blind_mode = color_blind_mode.min(3);
        let settings = [
            f32::from(enabled),
            self.focus_distance,
            self.color_blind_mode as f32,
            self.bloom_strength,
            self.ao_strength,
            f32::from(volumetric_light),
            f32::from(contact_shadows),
            self.contact_strength,
            f32::from(motion_blur),
            f32::from(volumetric_clouds),
            f32::from(temporal_aa),
            f32::from(self.taa_history_valid.get()),
        ];
        queue.write_buffer(&self.settings, 0, bytemuck::cast_slice(&settings));
    }

    pub fn set_environment(
        &self,
        queue: &wgpu::Queue,
        sun_uv: [f32; 2],
        visible: bool,
        dimension_style: f32,
        sun_direction: [f32; 4],
        motion_uv: [f32; 2],
        motion_strength: f32,
        cloud_origin_time: [f32; 4],
        view_right: [f32; 4],
        view_up: [f32; 4],
        view_back: [f32; 4],
        cloud_params: [f32; 4],
        previous_right: [f32; 4],
        previous_up: [f32; 4],
        previous_back: [f32; 4],
        camera_delta: [f32; 4],
        taa_jitter: [f32; 4],
    ) {
        let params = [
            sun_uv[0],
            sun_uv[1],
            f32::from(visible),
            dimension_style,
            sun_direction[0],
            sun_direction[1],
            sun_direction[2],
            sun_direction[3],
            motion_uv[0],
            motion_uv[1],
            motion_strength.clamp(0.0, 1.0),
            f32::from(self.water_reflections_enabled),
            cloud_origin_time[0],
            cloud_origin_time[1],
            cloud_origin_time[2],
            cloud_origin_time[3],
            view_right[0],
            view_right[1],
            view_right[2],
            view_right[3],
            view_up[0],
            view_up[1],
            view_up[2],
            view_up[3],
            view_back[0],
            view_back[1],
            view_back[2],
            view_back[3],
            cloud_params[0],
            cloud_params[1],
            cloud_params[2],
            cloud_params[3],
            previous_right[0],
            previous_right[1],
            previous_right[2],
            previous_right[3],
            previous_up[0],
            previous_up[1],
            previous_up[2],
            previous_up[3],
            previous_back[0],
            previous_back[1],
            previous_back[2],
            previous_back[3],
            camera_delta[0],
            camera_delta[1],
            camera_delta[2],
            camera_delta[3],
            taa_jitter[0],
            taa_jitter[1],
            taa_jitter[2],
            taa_jitter[3],
        ];
        queue.write_buffer(&self.environment, 0, bytemuck::cast_slice(&params));
    }

    /// Enable history sampling from the next submitted frame onward.
    pub fn mark_taa_history_valid(&self, queue: &wgpu::Queue) {
        if self.makeup_enabled && self.temporal_aa_enabled && !self.taa_history_valid.get() {
            self.taa_history_valid.set(true);
            queue.write_buffer(&self.settings, 44, bytemuck::cast_slice(&[1.0_f32]));
        }
    }

    /// Draw the scaled world onto `screen`.
    pub fn finish(&self, encoder: &mut wgpu::CommandEncoder, screen: &wgpu::TextureView) {
        let Some(target) = &self.target else { return };
        self.finish_with(encoder, screen, &target.group);
    }

    /// Draw what `group` binds (`bind`) onto `screen`.
    pub fn finish_with(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        screen: &wgpu::TextureView,
        group: &wgpu::BindGroup,
    ) {
        self.finish_to(encoder, screen, group);
    }

    pub fn finish_to(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
        group: &wgpu::BindGroup,
    ) {
        if self.makeup_enabled {
            {
                let mut compute = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("MakeUp exposure adaptation"),
                    timestamp_writes: None,
                });
                compute.set_pipeline(&self.exposure_pipeline);
                compute.set_bind_group(0, group, &[]);
                compute.dispatch_workgroups(1, 1, 1);
            }
            if let Some(target) = &self.target {
                for (index, (destination, mip_group)) in target
                    .bloom_mips
                    .iter()
                    .zip(&target.bloom_mip_groups)
                    .enumerate()
                {
                    let attachments = [Some(wgpu::RenderPassColorAttachment {
                        view: destination,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })];
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("MakeUp bloom downsample"),
                        color_attachments: &attachments,
                        ..Default::default()
                    });
                    pass.set_pipeline(if index == 0 {
                        &self.bloom_extract_pipeline
                    } else {
                        &self.mip_pipeline
                    });
                    pass.set_bind_group(0, mip_group, &[]);
                    pass.draw(0..3, 0..1);
                }
            }
        }
        if self.makeup_enabled && self.temporal_aa_enabled {
            if let Some(target) = &self.target {
                let index = self.taa_write_index.get() % target.taa_history.len().max(1);
                let history_group = &target.taa_input_groups[index];
                let history_target = &target.taa_history[index];
                let attachments = [Some(wgpu::RenderPassColorAttachment {
                    view: history_target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })];
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("MakeUp temporal accumulation"),
                    color_attachments: &attachments,
                    ..Default::default()
                });
                let pipeline = match (self.chroma_enabled, self.antialias) {
                    (true, true) => &self.makeup_fxaa_chroma_taa,
                    (true, false) => &self.makeup_chroma_taa,
                    (false, true) => &self.makeup_fxaa_taa,
                    (false, false) => &self.makeup_taa,
                };
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, history_group, &[]);
                pass.draw(0..3, 0..1);
                drop(pass);

                let attachments = [Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })];
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("MakeUp temporal present"),
                    color_attachments: &attachments,
                    ..Default::default()
                });
                pass.set_pipeline(&self.copy);
                pass.set_bind_group(0, &target.taa_copy_groups[index], &[]);
                pass.draw(0..3, 0..1);
                self.taa_write_index.set(1 - index);
                return;
            }
        }
        let attachments = [Some(wgpu::RenderPassColorAttachment {
            view: output,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Horizon post"),
            color_attachments: &attachments,
            ..Default::default()
        });
        let pipeline = match (self.makeup_enabled, self.chroma_enabled, self.antialias) {
            (true, true, true) => &self.makeup_fxaa_chroma,
            (true, true, false) => &self.makeup_chroma,
            (true, false, true) => &self.makeup_fxaa,
            (true, false, false) => &self.makeup_pipeline,
            (false, true, true) => &self.fxaa_chroma,
            (false, true, false) => &self.chroma,
            (false, false, true) if self.dof_enabled => &self.fxaa_dof,
            (false, false, false) if self.dof_enabled => &self.dof,
            (false, false, true) if self.color_blind_mode != 0 => &self.fxaa_color_blind,
            (false, false, false) if self.color_blind_mode != 0 => &self.color_blind,
            (false, false, true) => &self.fxaa,
            (false, false, false) => &self.copy,
        };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
        pollster::block_on(adapter.request_device(&Default::default())).ok()
    }

    #[test]
    fn makeup_post_rebuilds_water_and_bloom_targets_and_renders() {
        let Some((device, queue)) = device() else {
            eprintln!("no GPU adapter: skipped");
            return;
        };
        let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);

        let mut post = Post::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        post.antialias = true;
        let _ = post.prepare(&device, &queue, [64, 48], false);
        assert_eq!(post.target.as_ref().unwrap().bloom_mips.len(), 0);

        post.makeup_enabled = true;
        post.set_effect_settings(
            &queue, false, 16.0, 0.4, 0.7, false, false, true, true, false, 0.45, false, 0,
        );
        let _ = post.prepare(&device, &queue, [64, 48], false);
        assert!(post.target.as_ref().unwrap().bloom_mips.len() > 0);
        assert_eq!(post.target.as_ref().unwrap().taa_history.len(), 2);
        assert!(post.target.as_ref().unwrap().water_mask.is_some());
        post.set_environment(
            &queue,
            [0.5, 0.5],
            false,
            0.0,
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0],
            0.0,
            [0.0; 4],
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.5, 0.866, 0.0],
            [1.0, 1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.5, 0.866, 0.0],
            [0.0; 4],
            [0.0; 4],
        );

        let output = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("MakeUp bloom test output"),
            size: wgpu::Extent3d {
                width: 64,
                height: 48,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let target = post.target.as_ref().unwrap();
            let attachments = [
                Some(wgpu::RenderPassColorAttachment {
                    view: &target.color,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: target.water_mask.as_ref().unwrap(),
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                }),
            ];
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("MakeUp water reflection test input"),
                color_attachments: &attachments,
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.005),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
        }
        post.finish(&mut encoder, &output.create_view(&Default::default()));
        queue.submit([encoder.finish()]);
        post.mark_taa_history_valid(&queue);
        let mut encoder = device.create_command_encoder(&Default::default());
        post.finish(&mut encoder, &output.create_view(&Default::default()));
        queue.submit([encoder.finish()]);
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let error = pollster::block_on(error_scope.pop());
        assert!(
            error.is_none(),
            "wgpu rejected the bloom pyramid: {error:?}"
        );
    }
}
