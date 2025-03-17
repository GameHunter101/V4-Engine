use wgpu::{Device, Queue, Texture as WgpuTexture, TextureFormat, TextureView};

#[derive(Debug)]
pub struct Texture {
    format: TextureFormat,
    texture: WgpuTexture,
    view: TextureView,
    sampled: bool,
}

#[derive(Debug)]
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
        is_storage: bool,
        sampled: bool,
    ) -> tokio::io::Result<Self> {
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
            is_storage,
            sampled,
        ))
    }

    pub fn from_bytes(
        bytes: &[u8],
        dimensions: (u32, u32),
        device: &Device,
        queue: &Queue,
        format: TextureFormat,
        is_storage: bool,
        sampled: bool,
    ) -> Self {
        let texture = Self::create_texture(
            device,
            dimensions.0,
            dimensions.1,
            format,
            is_storage,
            sampled,
        );

        queue.write_texture(
            texture.texture_ref().as_image_copy(),
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
        is_storage: bool,
        sampled: bool,
    ) -> Self {
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
            usage: wgpu::TextureUsages::COPY_DST
                | if is_storage {
                    wgpu::TextureUsages::STORAGE_BINDING
                } else {
                    wgpu::TextureUsages::TEXTURE_BINDING
                },
            view_formats: &[],
        });

        let view = texture.create_view(&Default::default());

        Self {
            format,
            texture,
            view,
            sampled,
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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
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
