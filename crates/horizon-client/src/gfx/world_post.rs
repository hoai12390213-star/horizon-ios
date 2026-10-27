//! Resolution scale and FXAA (Horizon settings): the world renders into its
//! own target at the scaled size, then one full-screen pass copies or
//! anti-aliases it onto the screen, under the hand and HUD. With neither on,
//! the world renders straight to the screen.
use wgpu::util::DeviceExt;

struct WorldTarget {
    size: [u32; 2],
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    scratch: Option<wgpu::TextureView>,
    group: wgpu::BindGroup,
}

pub struct Post {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    settings: wgpu::Buffer,
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
    /// Enable MakeUp's optional chromatic aberration.
    pub chroma_enabled: bool,
    /// Enable MakeUp's depth-of-field effect.
    pub dof_enabled: bool,
    focus_distance: f32,
    color_blind_mode: u32,
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
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Horizon post"),
            bind_group_layouts: &[Some(&layout)],
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Horizon post"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let settings = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Horizon MakeUp post settings"),
            contents: bytemuck::cast_slice(&[0.0_f32, 16.0, 0.0, 0.0]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
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
            chroma: pipeline("fs_chroma"),
            fxaa_chroma: pipeline("fs_fxaa_chroma"),
            layout,
            sampler,
            settings,
            format,
            scale: 1.0,
            antialias: false,
            makeup_enabled: false,
            chroma_enabled: false,
            dof_enabled: false,
            focus_distance: 16.0,
            color_blind_mode: 0,
            target: None,
        }
    }

    /// The world's colour and depth targets this frame, or `None` to draw
    /// it straight to the screen.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        window: [u32; 2],
        always: bool,
    ) -> Option<(wgpu::TextureView, wgpu::TextureView)> {
        if self.scale == 1.0
            && !self.antialias
            && !self.makeup_enabled
            && !self.chroma_enabled
            && !self.dof_enabled
            && self.color_blind_mode == 0
            && !always
        {
            self.target = None;
            return None;
        }
        let size = window.map(|v| ((v as f32 * self.scale).round() as u32).clamp(1, 8192));
        // Only the shader-pack path needs an intermediate before its own
        // post-processing. Standalone MakeUp renders directly to the screen.
        let needs_scratch = always && self.makeup_effects_enabled();
        if self
            .target
            .as_ref()
            .is_none_or(|t| t.size != size || t.scratch.is_some() != needs_scratch)
        {
            let extent = wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            };
            let make_tex = |label, format, usage| {
                device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: extent,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
            };
            let color = make_tex(
                "Scaled world",
                self.format,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            )
            .create_view(&Default::default());
            // TEXTURE_BINDING: occlusion culling reads it.
            let depth = make_tex(
                "Scaled world depth",
                wgpu::TextureFormat::Depth32Float,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            )
            .create_view(&Default::default());
            let scratch = needs_scratch.then(|| {
                make_tex(
                    "MakeUp post intermediate",
                    self.format,
                    wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                )
                .create_view(&Default::default())
            });
            let group = self.bind(device, &color, &depth);
            self.target = Some(WorldTarget {
                size,
                color,
                depth,
                scratch,
                group,
            });
        }
        self.target
            .as_ref()
            .map(|t| (t.color.clone(), t.depth.clone()))
    }

    pub fn size(&self) -> Option<[u32; 2]> {
        self.target.as_ref().map(|t| t.size)
    }

    pub fn makeup_effects_enabled(&self) -> bool {
        self.makeup_enabled || self.chroma_enabled || self.dof_enabled || self.color_blind_mode != 0
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
            ],
        })
    }

    pub fn set_effect_settings(
        &mut self,
        queue: &wgpu::Queue,
        enabled: bool,
        focus: f32,
        color_blind_mode: u32,
    ) {
        self.dof_enabled = enabled;
        self.focus_distance = focus.clamp(2.0, 64.0);
        self.color_blind_mode = color_blind_mode.min(3);
        let settings = [
            f32::from(enabled),
            self.focus_distance,
            self.color_blind_mode as f32,
            0.0,
        ];
        queue.write_buffer(&self.settings, 0, bytemuck::cast_slice(&settings));
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
