use std::io::Cursor;

use image::{EncodableLayout, GenericImageView, ImageDecoder, codecs::hdr::HdrDecoder};
use wgpu::{
    Device, Queue, StorageTextureAccess, Texture as WgpuTexture, TextureFormat, TextureUsages,
    TextureView,
};

#[derive(Debug, Clone, Copy)]
pub struct TextureProperties {
    pub format: TextureFormat,
    pub storage_texture: Option<StorageTextureAccess>,
    pub is_cubemap: bool,
    pub is_filtered: bool,
    pub is_sampled: bool,
    pub is_hdr: bool,
    pub extra_usages: TextureUsages,
}

impl Default for TextureProperties {
    fn default() -> Self {
        Self {
            format: TextureFormat::Rgba8UnormSrgb,
            storage_texture: None,
            is_cubemap: false,
            is_filtered: true,
            is_sampled: true,
            is_hdr: false,
            extra_usages: TextureUsages::TEXTURE_BINDING,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TextureBundle {
    view: TextureView,
    properties: TextureProperties,
}

pub type CompleteTexture = (WgpuTexture, TextureBundle);

impl TextureBundle {
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    pub fn new(view: TextureView, properties: TextureProperties) -> TextureBundle {
        TextureBundle { view, properties }
    }

    pub async fn from_path(
        path: &str,
        device: &Device,
        queue: &Queue,
        props: TextureProperties,
    ) -> tokio::io::Result<CompleteTexture> {
        let raw_image = tokio::fs::read(path).await?;
        let (bytes, props, dimensions) = if props.is_hdr {
            (
                raw_image,
                TextureProperties {
                    is_sampled: false,
                    is_cubemap: false,
                    is_filtered: false,
                    extra_usages: props.extra_usages | TextureUsages::COPY_DST,
                    ..props
                },
                (0, 0),
            )
        } else {
            // TODO: Implement actual error handling
            let img = image::load_from_memory(&raw_image).expect("Failed to create image");
            let dims = img.dimensions();
            let bytes = if props.format.components() == 4 {
                let rgba8 = img.into_rgba8();
                rgba8.as_bytes().to_vec()
            } else {
                img.as_bytes().to_vec()
            };

            (bytes , props, dims)
        };

        Ok(Self::from_bytes(&bytes, dimensions, device, queue, props))
    }

    pub fn from_bytes(
        bytes: &[u8],
        dimensions: (u32, u32),
        device: &Device,
        queue: &Queue,
        props: TextureProperties,
    ) -> CompleteTexture {
        let texture_bundle = if props.is_hdr {
            let hdr_decoder = HdrDecoder::new(Cursor::new(bytes)).unwrap();
            let meta = hdr_decoder.metadata();
            let complete_texture = Self::create_texture(device, meta.width, meta.height, props);

            let mut bytes = vec![0_u8; hdr_decoder.total_bytes() as usize];
            hdr_decoder.read_image(&mut bytes).unwrap();
            bytes = (0..meta.width * meta.height)
                .flat_map(|pix_idx| {
                    let pix_idx = pix_idx as usize;
                    let rgb_pix_size = std::mem::size_of::<[f32; 3]>();
                    bytes[(pix_idx * rgb_pix_size)..((pix_idx + 1) * rgb_pix_size)]
                        .iter()
                        .copied()
                        .chain(bytemuck::cast_slice(&[1.0_f32]).to_vec())
                })
                .collect();
            queue.write_texture(
                complete_texture.0.as_image_copy(),
                &bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(meta.width * std::mem::size_of::<[f32; 4]>() as u32),
                    rows_per_image: Some(meta.height),
                },
                complete_texture.0.size(),
            );
            complete_texture
        } else {
            let texture_bundle = Self::create_texture(device, dimensions.0, dimensions.1, props);
            queue.write_texture(
                texture_bundle.0.as_image_copy(),
                bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(props.format.components() as u32 * dimensions.0),
                    rows_per_image: Some(dimensions.1),
                },
                wgpu::Extent3d {
                    width: dimensions.0,
                    height: dimensions.1,
                    depth_or_array_layers: 1,
                },
            );
            texture_bundle
        };

        texture_bundle
    }

    pub fn create_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        properties: TextureProperties,
    ) -> CompleteTexture {
        let TextureProperties {
            format,
            storage_texture,
            is_cubemap,
            extra_usages,
            ..
        } = properties;

        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: if is_cubemap { 6 } else { 1 },
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("New created texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: extra_usages
                | TextureUsages::COPY_DST
                | if storage_texture.is_some() {
                    TextureUsages::STORAGE_BINDING
                } else {
                    TextureUsages::TEXTURE_BINDING
                },
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(if is_cubemap {
                if storage_texture.is_some() {
                    wgpu::TextureViewDimension::D2Array
                } else {
                    wgpu::TextureViewDimension::Cube
                }
            } else {
                wgpu::TextureViewDimension::D2
            }),
            array_layer_count: if is_cubemap && storage_texture.is_none() {
                Some(size.depth_or_array_layers)
            } else {
                None
            },
            ..Default::default()
        });

        (texture, TextureBundle { view, properties })
    }

    pub fn create_depth_texture(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> CompleteTexture {
        let size = wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        };
        let desc = wgpu::TextureDescriptor {
            label: Some("Depth texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        let texture = device.create_texture(&desc);

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        (
            texture,
            TextureBundle {
                view,
                properties: TextureProperties {
                    format: Self::DEPTH_FORMAT,
                    storage_texture: None,
                    is_cubemap: false,
                    is_filtered: false,
                    is_sampled: true,
                    is_hdr: false,
                    extra_usages: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                },
            },
        )
    }

    pub fn view(&self) -> &TextureView {
        &self.view
    }

    pub fn view_mut(&mut self) -> &mut TextureView {
        &mut self.view
    }

    pub fn properties(&self) -> TextureProperties {
        self.properties
    }
}
