use wgpu::{Device, Queue, StorageTextureAccess, Texture as WgpuTexture, TextureFormat, TextureUsages, TextureView};

use crate::ecs::material::GeneralTexture;

#[derive(Debug, Clone)]
pub struct Texture {
    format: TextureFormat,
    texture: WgpuTexture,
    view: TextureView,
    sampled: bool,
}

#[derive(Debug, Clone)]
pub struct StorageTexture {
    format: TextureFormat,
    texture: WgpuTexture,
    view: TextureView,
    access: wgpu::StorageTextureAccess,
}

impl Texture {
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    pub fn texture_ref(&self) -> &WgpuTexture {
        &self.texture
    }

    pub fn texture(self) -> WgpuTexture {
        self.texture
    }

    pub fn view_ref(&self) -> &TextureView {
        &self.view
    }

    pub fn view(self) -> TextureView {
        self.view
    }

    pub fn is_sampled(&self) -> bool {
        self.sampled
    }

    pub async fn from_path(
        path: &str,
        device: &Device,
        queue: &Queue,
        format: TextureFormat,
        storage_texture_access: Option<StorageTextureAccess>,
        sampled: bool,
        extra_usages: TextureUsages,
    ) -> tokio::io::Result<GeneralTexture> {
        let raw_image = tokio::fs::read(path).await?;

        // TODO: Implement actual error handling
        let raw = image::load_from_memory(&raw_image).expect("Failed to create image");
        // let image = raw.into_rgba8();
        let bytes = raw.as_bytes();

        Ok(Self::from_bytes(
            bytes,
            (raw.width(), raw.height()),
            device,
            queue,
            format,
            storage_texture_access,
            sampled,
            extra_usages,
        ))
    }

    pub fn from_bytes(
        bytes: &[u8],
        dimensions: (u32, u32),
        device: &Device,
        queue: &Queue,
        format: TextureFormat,
        storage_texture_access: Option<StorageTextureAccess>,
        sampled: bool,
        extra_usages: TextureUsages,
    ) -> GeneralTexture {
        let texture = Self::create_texture(
            device,
            dimensions.0,
            dimensions.1,
            format,
            storage_texture_access,
            sampled,
            extra_usages,
        );

        queue.write_texture(
            texture.texture().as_image_copy(),
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(format.components() as u32 * dimensions.0),
                rows_per_image: Some(dimensions.1),
            },
            wgpu::Extent3d {
                width: dimensions.0,
                height: dimensions.1,
                depth_or_array_layers: 1,
            },
        );

        texture
    }

    pub fn create_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: TextureFormat,
        storage_texture_access: Option<StorageTextureAccess>,
        sampled: bool,
        extra_usages: TextureUsages,
    ) -> GeneralTexture {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("new created texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: extra_usages | TextureUsages::COPY_DST
                | if storage_texture_access.is_some() {
                    TextureUsages::STORAGE_BINDING
                } else {
                    TextureUsages::TEXTURE_BINDING
                },
            view_formats: &[],
        });

        let view = texture.create_view(&Default::default());

        if let Some(access) = storage_texture_access {
            GeneralTexture::Storage(StorageTexture { format, texture, view, access })
        } else {
            GeneralTexture::Regular( Texture {
                format,
                texture,
                view,
                sampled,
            })
        }
    }

    pub fn create_depth_texture(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> Self {
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

        Self {
            format: texture.format(),
            texture,
            view,
            sampled: true,
        }
    }

    pub fn view_mut(&mut self) -> &mut TextureView {
        &mut self.view
    }

    pub fn format(&self) -> TextureFormat {
        self.format
    }
}

impl StorageTexture {
    pub fn texture_ref(&self) -> &WgpuTexture {
        &self.texture
    }

    pub fn texture(self) -> WgpuTexture {
        self.texture
    }

    pub fn view_ref(&self) -> &TextureView {
        &self.view
    }

    pub fn view(self) -> TextureView {
        self.view
    }

    pub fn access(&self) -> wgpu::StorageTextureAccess {
        self.access
    }

    pub fn format(&self) -> TextureFormat {
        self.format
    }

    pub fn view_mut(&mut self) -> &mut TextureView {
        &mut self.view
    }
}
