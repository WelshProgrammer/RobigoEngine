//! The renderer: CPU-drawn indexed frames, resolved and scaled on the GPU.
//!
//! A frame stays as palette indices until the last step, as in the original:
//!
//! 1. A [`MapView`] draws the map view into a [`Frame`] of palette indices on the CPU, matching
//!    `S2.EXE` pixel for pixel. The [`terrain`] pass draws landscape, roads and border stones,
//!    directly or from cached tiles. The object pass draws trees, stones, buildings, flags and
//!    animals over it every frame. The map state comes from `s2-sim`.
//! 2. The resolve pass looks each index up in the palette, so palette cycling never redraws a
//!    frame.
//! 3. The scale pass draws that to the output: nearest at whole zooms, sharp bilinear between,
//!    area-weighted when zoomed out.
//! 4. An [`Overlay`] goes over it the same way at its own whole scale, nearest only, so the
//!    interface keeps its pixel sizes however far the view is zoomed.
//!
//! A [`Renderer`] presents to a window, or renders to an [`Image`] when made with
//! [`Renderer::headless`]. [`Frame::to_image`] resolves a frame without a GPU.

pub mod capture;
mod frame;
mod heard;
mod layers;
mod map_view;
mod objects;
pub mod overlay;
mod resolve;
mod scale;
mod sprite;
pub mod terrain;
mod view;

#[cfg(test)]
mod tests;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use s2_assets::Palette;

pub use frame::{Frame, Image};
pub use heard::{HEARING, Heard, heard};
pub use layers::{Layers, Provenance, UnknownLayer};
pub use map_view::MapView;
pub use objects::{
    Clocks, Door, Doors, DoorsError, Extra, Extras, ExtrasError, FlagSlots, MAX_DOOR_TYPES, Mark,
    ObjectGraphics, ObjectTables, Offset, Place, ShipFlags, Walking, Yard, draw_walking, figure_position,
    job_item,
};
pub use overlay::{Overlay, Patch};
pub use sprite::{Paint, blit, colour_base};
pub use view::{BadZoom, ScaleFilter, Zoom};

use resolve::{RESOLVED_FORMAT, ResolvePass, WORLD_FORMAT};
use scale::{Blit, ScalePass, Scissor};

/// Format of [`Renderer::capture`] output before readback.
const CAPTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Errors raised while setting up or driving the GPU.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The window's native handles couldn't be read.
    #[error("window handle: {0}")]
    Handle(#[from] raw_window_handle::HandleError),
    /// wgpu couldn't create a surface for the window.
    #[error("creating surface: {0}")]
    Surface(#[from] wgpu::CreateSurfaceError),
    /// No GPU adapter is available, or none can present to the window.
    #[error("no suitable GPU adapter: {0}")]
    Adapter(#[from] wgpu::RequestAdapterError),
    /// The adapter refused to create a device.
    #[error("creating device: {0}")]
    Device(#[from] wgpu::RequestDeviceError),
    /// The frame is larger than the GPU supports.
    #[error("a {width}x{height} frame exceeds the GPU's {max}-pixel limit")]
    ViewTooLarge {
        /// Width of the frame.
        width: u32,
        /// Height of the frame.
        height: u32,
        /// The largest texture side the GPU accepts.
        max: u32,
    },
    /// An output or frame size of zero.
    #[error("cannot render a {width}x{height} image")]
    EmptyOutput {
        /// Width asked for.
        width: u32,
        /// Height asked for.
        height: u32,
    },
    /// The window's drawing surface could not be configured, which happens when the window is
    /// gone from under the renderer.
    #[error("the window's drawing surface is gone: {0}")]
    SurfaceLost(String),
    /// [`Renderer::render`] on a renderer without a window.
    #[error("this renderer has no window to present to")]
    Headless,
    /// Waiting for the GPU failed.
    #[error("waiting for the GPU: {0}")]
    Poll(#[from] wgpu::PollError),
    /// The captured frame could not be read back.
    #[error("reading back the frame: {0}")]
    Readback(String),
}

/// Result type for renderer calls.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
struct Presenter {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}

impl Presenter {
    /// Configures the surface for `config`, catching the validation error a surface whose
    /// window has gone raises, which wgpu would otherwise treat as fatal.
    fn configure(&self, device: &wgpu::Device) -> Result<()> {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        self.surface.configure(device, &self.config);
        match pollster::block_on(scope.pop()) {
            Some(error) => Err(Error::SurfaceLost(error.to_string())),
            None => Ok(()),
        }
    }
}

/// Intermediate targets for one frame size.
#[derive(Debug)]
struct Targets {
    size: (u32, u32),
    world: wgpu::Texture,
    resolved: wgpu::TextureView,
    resolve_group: wgpu::BindGroup,
    scale_group: scale::Binding,
}

/// Where a frame goes on the output when it is stretched over part of it, in output pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    /// The left edge.
    pub x: u32,
    /// The top edge.
    pub y: u32,
    /// How wide it is drawn.
    pub width: u32,
    /// How tall it is drawn.
    pub height: u32,
}

impl Placement {
    /// The largest placement of `aspect`, across to down, that fits `output`, centred.
    #[must_use]
    pub fn fitted(output: (u32, u32), (across, down): (u32, u32)) -> Self {
        let (out_w, out_h) = (u64::from(output.0), u64::from(output.1));
        let (across, down) = (u64::from(across.max(1)), u64::from(down.max(1)));
        let (width, height) = if out_w * down <= out_h * across {
            (out_w, out_w * down / across)
        } else {
            (out_h * across / down, out_h)
        };
        let narrow = |value: u64| u32::try_from(value).unwrap_or(u32::MAX);
        Self {
            x: narrow((out_w - width) / 2),
            y: narrow((out_h - height) / 2),
            width: narrow(width),
            height: narrow(height),
        }
    }

    /// The output pixel the frame's top-left corner lands on.
    #[expect(clippy::cast_precision_loss, reason = "output sizes are far below 2^24")]
    fn origin(self) -> (f32, f32) {
        (self.x as f32, self.y as f32)
    }

    /// Output pixels a pixel of a frame of `size` covers, across and down.
    #[expect(clippy::cast_precision_loss, reason = "sizes are far below 2^24")]
    fn scale(self, size: (u32, u32)) -> (f32, f32) {
        (
            self.width as f32 / size.0.max(1) as f32,
            self.height as f32 / size.1.max(1) as f32,
        )
    }
}

/// Resolves and scales frames, to a window or to images.
#[derive(Debug)]
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter_info: wgpu::AdapterInfo,
    presenter: Option<Presenter>,
    resolve: ResolvePass,
    scale: ScalePass,
    targets: Option<Targets>,
    interface: Option<Targets>,
}

impl Renderer {
    /// Creates a renderer presenting to `window`, whose drawable size is `width` × `height`
    /// pixels.
    ///
    /// # Safety
    ///
    /// The window must outlive the renderer: drop the renderer first.
    ///
    /// # Errors
    ///
    /// Fails if the window handles are unavailable or no GPU can present to the window.
    pub unsafe fn new(
        window: &(impl HasWindowHandle + HasDisplayHandle),
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let target = wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(window.display_handle()?.as_raw()),
            raw_window_handle: window.window_handle()?.as_raw(),
        };
        // SAFETY: the caller guarantees the window outlives the renderer and so the surface.
        let surface = unsafe { instance.create_surface_unsafe(target)? };
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))?;
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .or_else(|| caps.formats.first().copied())
            .unwrap_or(CAPTURE_FORMAT);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps
                .alpha_modes
                .first()
                .copied()
                .unwrap_or(wgpu::CompositeAlphaMode::Auto),
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        let mut renderer = Self::with_adapter(&adapter)?;
        surface.configure(&renderer.device, &config);
        renderer.scale.prepare(&renderer.device, format);
        renderer.presenter = Some(Presenter { surface, config });
        Ok(renderer)
    }

    /// Creates a renderer without a window, for [`Renderer::capture`].
    ///
    /// # Errors
    ///
    /// [`Error::Adapter`] when the machine has no usable GPU or software rasteriser.
    pub fn headless() -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
        Self::with_adapter(&adapter)
    }

    fn with_adapter(adapter: &wgpu::Adapter) -> Result<Self> {
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("robigo"),
            required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))?;
        let resolve = ResolvePass::new(&device);
        let mut scale = ScalePass::new(&device);
        scale.prepare(&device, CAPTURE_FORMAT);
        Ok(Self {
            device,
            queue,
            adapter_info: adapter.get_info(),
            presenter: None,
            resolve,
            scale,
            targets: None,
            interface: None,
        })
    }

    /// The GPU and backend in use, for logs and bug reports.
    #[must_use]
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.adapter_info
    }

    /// The largest frame side the GPU accepts. A view at zoom `z` on an output of side `n` needs
    /// a frame of side `n / z`.
    #[must_use]
    pub fn max_world_size(&self) -> u32 {
        self.device.limits().max_texture_dimension_2d
    }

    /// Call when the window's drawable size changes.
    ///
    /// # Errors
    ///
    /// [`Error::SurfaceLost`] when the window's surface can no longer be configured.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        let Some(presenter) = &mut self.presenter else {
            return Ok(());
        };
        if width == 0 || height == 0 {
            return Ok(());
        }
        presenter.config.width = width;
        presenter.config.height = height;
        presenter.configure(&self.device)
    }

    /// The window's drawable size, or `None` without a window.
    #[must_use]
    pub fn output_size(&self) -> Option<(u32, u32)> {
        self.presenter.as_ref().map(|p| (p.config.width, p.config.height))
    }

    /// Replaces the palette every index resolves through.
    pub fn set_palette(&mut self, palette: &Palette) {
        self.resolve.set_palette(&self.queue, palette);
    }

    /// Shows `frame` in the window at `zoom`, its top-left corner at the window's, with an
    /// `interface` layer over it, and presents it. A frame of [`Zoom::world_size`] of the
    /// window's size covers the window.
    ///
    /// # Errors
    ///
    /// [`Error::Headless`] without a window, [`Error::EmptyOutput`] for an empty frame,
    /// [`Error::ViewTooLarge`] when the frame is past the GPU's limit, [`Error::SurfaceLost`]
    /// when the window's surface can no longer be configured.
    pub fn render(&mut self, frame: &Frame, zoom: Zoom, interface: Option<Overlay>) -> Result<()> {
        self.present(|renderer, encoder, target, format, size| {
            renderer.encode(encoder, frame, zoom, interface, target, format, size)
        })
    }

    /// Shows `frame` stretched over `placed`, black around it, and presents it. A frame scaled
    /// up is drawn sharp bilinear and one scaled down area-weighted.
    ///
    /// # Errors
    ///
    /// As [`Renderer::render`].
    pub fn render_placed(&mut self, frame: &Frame, placed: Placement) -> Result<()> {
        self.present(|renderer, encoder, target, format, _| {
            renderer.encode_placed(encoder, frame, placed, target, format)
        })
    }

    /// Draws `frame` at `zoom` to a `width` × `height` image, with an `interface` layer over
    /// it, and reads it back. Indices are resolved through the palette exactly: at a whole zoom
    /// each output pixel is a palette colour.
    ///
    /// # Errors
    ///
    /// [`Error::EmptyOutput`] for a zero size, [`Error::ViewTooLarge`] past the GPU's limit,
    /// [`Error::Poll`] or [`Error::Readback`] if the GPU fails to hand the frame back.
    pub fn capture(
        &mut self,
        frame: &Frame,
        zoom: Zoom,
        width: u32,
        height: u32,
        interface: Option<Overlay>,
    ) -> Result<Image> {
        self.check_world_size((frame.width(), frame.height()))?;
        self.read_back((width, height), |renderer, encoder, target, format, size| {
            renderer.encode(encoder, frame, zoom, interface, target, format, size)
        })
    }

    /// Draws `frame` stretched over `placed` on a `width` × `height` image, black around it, as
    /// [`Renderer::render_placed`] shows it, and reads it back.
    ///
    /// # Errors
    ///
    /// As [`Renderer::capture`].
    pub fn capture_placed(
        &mut self,
        frame: &Frame,
        placed: Placement,
        width: u32,
        height: u32,
    ) -> Result<Image> {
        self.check_world_size((frame.width(), frame.height()))?;
        self.read_back((width, height), |renderer, encoder, target, format, _| {
            renderer.encode_placed(encoder, frame, placed, target, format)
        })
    }

    /// Encodes `draw` into the window's next texture and presents it.
    fn present(
        &mut self,
        draw: impl FnOnce(
            &mut Self,
            &mut wgpu::CommandEncoder,
            &wgpu::TextureView,
            wgpu::TextureFormat,
            (u32, u32),
        ) -> Result<()>,
    ) -> Result<()> {
        let Some(presenter) = &self.presenter else {
            return Err(Error::Headless);
        };
        let format = presenter.config.format;
        let size = (presenter.config.width, presenter.config.height);
        let output = match presenter.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                return presenter.configure(&self.device);
            }
            _ => return Ok(()),
        };
        let target = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        draw(self, &mut encoder, &target, format, size)?;
        self.queue.submit([encoder.finish()]);
        self.queue.present(output);
        Ok(())
    }

    /// Encodes `draw` into a `width` × `height` image and reads it back.
    fn read_back(
        &mut self,
        (width, height): (u32, u32),
        draw: impl FnOnce(
            &mut Self,
            &mut wgpu::CommandEncoder,
            &wgpu::TextureView,
            wgpu::TextureFormat,
            (u32, u32),
        ) -> Result<()>,
    ) -> Result<Image> {
        if width == 0 || height == 0 {
            return Err(Error::EmptyOutput { width, height });
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("capture"),
            size: extent(width, height),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: CAPTURE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let padded_row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capture readback"),
            size: u64::from(padded_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("capture"),
            });
        draw(self, &mut encoder, &target, CAPTURE_FORMAT, (width, height))?;
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            extent(width, height),
        );
        self.queue.submit([encoder.finish()]);

        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely())?;
        receiver
            .recv()
            .map_err(|e| Error::Readback(e.to_string()))?
            .map_err(|e| Error::Readback(e.to_string()))?;
        let mapped = slice
            .get_mapped_range()
            .map_err(|e| Error::Readback(e.to_string()))?;
        let row = width as usize * 4;
        let rgba = mapped
            .chunks_exact(padded_row as usize)
            .flat_map(|line| &line[..row])
            .copied()
            .collect();
        drop(mapped);
        buffer.unmap();
        Ok(Image::new(width, height, rgba))
    }

    fn check_world_size(&self, (width, height): (u32, u32)) -> Result<()> {
        let max = self.max_world_size();
        if width > max || height > max {
            return Err(Error::ViewTooLarge { width, height, max });
        }
        Ok(())
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "one frame of two layers needs all of it"
    )]
    fn encode(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        frame: &Frame,
        zoom: Zoom,
        interface: Option<Overlay>,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        output: (u32, u32),
    ) -> Result<()> {
        let size = (frame.width(), frame.height());
        if size.0 == 0 || size.1 == 0 {
            return Err(Error::EmptyOutput {
                width: size.0,
                height: size.1,
            });
        }
        self.check_world_size(size)?;
        if self.targets.as_ref().is_none_or(|t| t.size != size) {
            self.targets = Some(self.create_targets(size));
        }
        let Some(targets) = &self.targets else {
            return Ok(());
        };
        self.scale.prepare(&self.device, format);
        upload(&self.queue, targets, frame);
        self.resolve
            .draw(encoder, &targets.resolve_group, &targets.resolved);
        self.scale.draw(
            &self.queue,
            encoder,
            &targets.scale_group,
            &Blit {
                target,
                format,
                scale: (zoom.factor(), zoom.factor()),
                origin: (0.0, 0.0),
                filter: zoom.filter(),
                clear: true,
                patches: &[],
            },
        );
        self.encode_interface(encoder, interface, target, format, output)
    }

    /// Draws the interface layer over what the view left in `target`, in the parts of the output
    /// its patches cover.
    fn encode_interface(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        interface: Option<Overlay>,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        output: (u32, u32),
    ) -> Result<()> {
        let Some(layer) = interface else { return Ok(()) };
        let size = (layer.frame.width(), layer.frame.height());
        if size.0 == 0 || size.1 == 0 {
            return Ok(());
        }
        self.check_world_size(size)?;
        let patches: Vec<Scissor> = layer
            .patches
            .iter()
            .filter_map(|patch| patch.on_output(layer.scale, output))
            .collect();
        if patches.is_empty() {
            return Ok(());
        }
        if self.interface.as_ref().is_none_or(|t| t.size != size) {
            self.interface = Some(self.create_targets(size));
        }
        let Some(targets) = &self.interface else {
            return Ok(());
        };
        upload(&self.queue, targets, layer.frame);
        self.resolve
            .draw(encoder, &targets.resolve_group, &targets.resolved);
        self.scale.draw(
            &self.queue,
            encoder,
            &targets.scale_group,
            &Blit {
                target,
                format,
                scale: (f32::from(layer.scale.max(1)), f32::from(layer.scale.max(1))),
                origin: (0.0, 0.0),
                filter: ScaleFilter::Nearest,
                clear: false,
                patches: &patches,
            },
        );
        Ok(())
    }

    /// Draws `frame` stretched over `placed` into `target`, clearing the rest to black.
    fn encode_placed(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        frame: &Frame,
        placed: Placement,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) -> Result<()> {
        let size = (frame.width(), frame.height());
        if size.0 == 0 || size.1 == 0 || placed.width == 0 || placed.height == 0 {
            return Err(Error::EmptyOutput {
                width: placed.width,
                height: placed.height,
            });
        }
        if self.targets.as_ref().is_none_or(|t| t.size != size) {
            self.targets = Some(self.create_targets(size));
        }
        let Some(targets) = &self.targets else {
            return Ok(());
        };
        self.scale.prepare(&self.device, format);
        upload(&self.queue, targets, frame);
        self.resolve
            .draw(encoder, &targets.resolve_group, &targets.resolved);
        let scale = placed.scale(size);
        let filter = if scale.0 < 1.0 || scale.1 < 1.0 {
            ScaleFilter::Area
        } else {
            ScaleFilter::SharpBilinear
        };
        self.scale.draw(
            &self.queue,
            encoder,
            &targets.scale_group,
            &Blit {
                target,
                format,
                scale,
                origin: placed.origin(),
                filter,
                clear: true,
                patches: &[Scissor {
                    x: placed.x,
                    y: placed.y,
                    width: placed.width,
                    height: placed.height,
                }],
            },
        );
        Ok(())
    }

    fn create_targets(&self, size: (u32, u32)) -> Targets {
        let texture = |label, format, usage| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: extent(size.0, size.1),
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let world = texture(
            "world",
            WORLD_FORMAT,
            wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let resolved = texture(
            "resolved",
            RESOLVED_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        )
        .create_view(&wgpu::TextureViewDescriptor::default());
        let world_view = world.create_view(&wgpu::TextureViewDescriptor::default());
        Targets {
            size,
            resolve_group: self.resolve.bind(&self.device, &world_view),
            scale_group: self.scale.bind(&self.device, &resolved, size),
            world,
            resolved,
        }
    }
}

fn upload(queue: &wgpu::Queue, targets: &Targets, frame: &Frame) {
    queue.write_texture(
        targets.world.as_image_copy(),
        frame.pixels(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(targets.size.0),
            rows_per_image: None,
        },
        extent(targets.size.0, targets.size.1),
    );
}

fn extent(width: u32, height: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    }
}
