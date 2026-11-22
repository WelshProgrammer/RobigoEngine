//! The scale pass: a resolved layer to the output at its own scale.

use crate::view::ScaleFilter;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    scale: [f32; 2],
    origin: [f32; 2],
    mode: u32,
    decode_srgb: u32,
    size: [u32; 2],
}

/// One layer's bind group and the parameters it draws with. Each layer keeps its own buffer
/// because the queue applies every write before the commands run, so a shared buffer would give
/// both layers of a frame the parameters of the one written last.
#[derive(Debug)]
pub(crate) struct Binding {
    group: wgpu::BindGroup,
    params: wgpu::Buffer,
    /// The image's size in pixels.
    size: (u32, u32),
}

/// A part of the output a draw is kept inside, in output pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Scissor {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// Where a layer goes and how it gets there.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Blit<'a> {
    pub(crate) target: &'a wgpu::TextureView,
    pub(crate) format: wgpu::TextureFormat,
    /// Output pixels a layer pixel covers, across and down.
    pub(crate) scale: (f32, f32),
    /// The output pixel the layer's top-left corner lands on.
    pub(crate) origin: (f32, f32),
    pub(crate) filter: ScaleFilter,
    /// Clears the target first. The layer that goes down second leaves what is under it.
    pub(crate) clear: bool,
    /// The parts of the output to draw. An empty list draws over all of it.
    pub(crate) patches: &'a [Scissor],
}

#[derive(Debug)]
pub(crate) struct ScalePass {
    shader: wgpu::ShaderModule,
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: Vec<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
}

impl ScalePass {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scale"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
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
            label: Some("scale"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        Self {
            shader: device.create_shader_module(wgpu::include_wgsl!("scale.wgsl")),
            layout,
            pipeline_layout,
            pipelines: Vec::new(),
        }
    }

    /// A binding reading the resolved image `resolved`, which is `size` pixels.
    pub(crate) fn bind(
        &self,
        device: &wgpu::Device,
        resolved: &wgpu::TextureView,
        size: (u32, u32),
    ) -> Binding {
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scale params"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scale"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(resolved),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: params.as_entire_binding(),
                },
            ],
        });
        Binding { group, params, size }
    }

    /// Makes sure a pipeline drawing into `format` exists.
    pub(crate) fn prepare(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        if !self.pipelines.iter().any(|(f, _)| *f == format) {
            let pipeline = pipeline(device, &self.pipeline_layout, &self.shader, format);
            self.pipelines.push((format, pipeline));
        }
    }

    /// Scales the image bound in `binding` into the target of `blit`, whose format must have
    /// been passed to [`Self::prepare`].
    pub(crate) fn draw(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        binding: &Binding,
        blit: &Blit,
    ) {
        let Some((_, pipeline)) = self.pipelines.iter().find(|(f, _)| *f == blit.format) else {
            return;
        };
        let params = Params {
            scale: [blit.scale.0, blit.scale.1],
            origin: [blit.origin.0, blit.origin.1],
            mode: match blit.filter {
                ScaleFilter::Nearest => 0,
                ScaleFilter::SharpBilinear => 1,
                ScaleFilter::Area => 2,
            },
            decode_srgb: u32::from(blit.format.is_srgb()),
            size: [binding.size.0, binding.size.1],
        };
        queue.write_buffer(&binding.params, 0, bytemuck::bytes_of(&params));
        let load = if blit.clear {
            wgpu::LoadOp::Clear(wgpu::Color::BLACK)
        } else {
            wgpu::LoadOp::Load
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scale"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: blit.target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &binding.group, &[]);
        if blit.patches.is_empty() {
            pass.draw(0..3, 0..1);
            return;
        }
        for patch in blit.patches {
            pass.set_scissor_rect(patch.x, patch.y, patch.width, patch.height);
            pass.draw(0..3, 0..1);
        }
    }
}

fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("scale"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(format.into())],
        }),
        multiview_mask: None,
        cache: None,
    })
}
