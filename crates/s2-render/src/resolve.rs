//! The resolve pass: palette index to colour.

use s2_assets::Palette;

/// Format of the indexed frame on the GPU.
pub(crate) const WORLD_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Uint;

/// Format of the resolved image: the palette's 8-bit values passed through unchanged.
pub(crate) const RESOLVED_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

const TABLE_SIZE: u32 = 256;

#[derive(Debug)]
pub(crate) struct ResolvePass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    palette: wgpu::Texture,
}

impl ResolvePass {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("resolve"),
            entries: &[
                texture_entry(0, wgpu::TextureSampleType::Uint),
                texture_entry(1, wgpu::TextureSampleType::Float { filterable: false }),
            ],
        });
        let palette = palette_texture(device);
        Self {
            pipeline: pipeline(device, &layout),
            layout,
            palette,
        }
    }

    pub(crate) fn set_palette(&self, queue: &wgpu::Queue, palette: &Palette) {
        let rgba: Vec<u8> = palette
            .0
            .iter()
            .flat_map(|&[r, g, b]| [r, g, b, u8::MAX])
            .collect();
        write(queue, &self.palette, &rgba, TABLE_SIZE * 4);
    }

    /// A bind group reading the indexed frame `world`.
    pub(crate) fn bind(&self, device: &wgpu::Device, world: &wgpu::TextureView) -> wgpu::BindGroup {
        let palette = self.palette.create_view(&wgpu::TextureViewDescriptor::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("resolve"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(world),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&palette),
                },
            ],
        })
    }

    pub(crate) fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        bind_group: &wgpu::BindGroup,
        target: &wgpu::TextureView,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("resolve"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn texture_entry(binding: u32, sample_type: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn palette_texture(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("palette"),
        size: wgpu::Extent3d {
            width: TABLE_SIZE,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn write(queue: &wgpu::Queue, texture: &wgpu::Texture, bytes: &[u8], bytes_per_row: u32) {
    queue.write_texture(
        texture.as_image_copy(),
        bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(bytes_per_row),
            rows_per_image: None,
        },
        texture.size(),
    );
}

fn pipeline(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::include_wgsl!("resolve.wgsl"));
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("resolve"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("resolve"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(RESOLVED_FORMAT.into())],
        }),
        multiview_mask: None,
        cache: None,
    })
}
