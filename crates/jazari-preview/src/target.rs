//! The render target of [`PreviewPlayer`](crate::PreviewPlayer): a texture
//! egui can sample directly, modelled on Ruffle desktop's `MovieView`.
//!
//! Unlike Ruffle's `TextureTarget`, submitting a frame does not copy it into
//! a mappable buffer. The texture also has `COPY_SRC`, so a frame can be read
//! back on demand ([`read_texture`]).

use std::sync::atomic::{AtomicU64, Ordering};

use ruffle_render_wgpu::descriptors::Descriptors;
use ruffle_render_wgpu::target::{RenderTarget, RenderTargetFrame};
use ruffle_render_wgpu::utils::{BufferDimensions, buffer_to_image};

/// Every texture gets a new generation, so hosts know when to re-register it
/// with egui (after a resize or a rebuild).
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

/// The texture Ruffle renders the preview into.
#[derive(Debug)]
pub(crate) struct PreviewTarget {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    generation: u64,
}

impl PreviewTarget {
    /// egui's native textures must be `Rgba8Unorm`; Ruffle writes gamma-space
    /// colours, which is what egui expects from a non-sRGB texture.
    pub(crate) const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

    /// A target of `width` x `height` physical pixels (at least 1 x 1).
    pub(crate) fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("jazari preview"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            texture,
            view,
            generation: NEXT_GENERATION.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// The texture.
    pub(crate) fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    /// A view of the whole texture.
    pub(crate) fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// The texture's generation (unique per texture).
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }
}

/// One frame of [`PreviewTarget`]: a view of its texture.
#[derive(Debug)]
pub(crate) struct PreviewFrame(wgpu::TextureView);

impl RenderTargetFrame for PreviewFrame {
    fn into_view(self) -> wgpu::TextureView {
        self.0
    }

    fn view(&self) -> &wgpu::TextureView {
        &self.0
    }
}

impl RenderTarget for PreviewTarget {
    type Frame = PreviewFrame;

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if (width.max(1), height.max(1)) != (self.width(), self.height()) {
            *self = Self::new(device, width, height);
        }
    }

    fn format(&self) -> wgpu::TextureFormat {
        Self::FORMAT
    }

    fn width(&self) -> u32 {
        self.texture.width()
    }

    fn height(&self) -> u32 {
        self.texture.height()
    }

    fn get_next_texture(&mut self) -> Option<Self::Frame> {
        Some(PreviewFrame(self.view.clone()))
    }

    fn submit<I: IntoIterator<Item = wgpu::CommandBuffer>>(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        command_buffers: I,
        _frame: Self::Frame,
    ) -> wgpu::SubmissionIndex {
        queue.submit(command_buffers)
    }
}

/// Copies `texture` (an `Rgba8Unorm` texture with `COPY_SRC`) into an image
/// with straight alpha. Blocks until the GPU (or WARP) has finished.
///
/// Panics if the device is lost; call it inside `catch_unwind`.
pub(crate) fn read_texture(descriptors: &Descriptors, texture: &wgpu::Texture) -> image::RgbaImage {
    let size = texture.size();
    let dimensions =
        BufferDimensions::new(size.width as usize, size.height as usize, texture.format());
    let buffer = descriptors.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("jazari preview readback"),
        size: dimensions.size(),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = descriptors
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("jazari preview readback"),
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
                bytes_per_row: Some(dimensions.padded_bytes_per_row),
                rows_per_image: None,
            },
        },
        size,
    );
    let index = descriptors.queue.submit(Some(encoder.finish()));
    buffer_to_image(&descriptors.device, &buffer, &dimensions, Some(index), size)
}
