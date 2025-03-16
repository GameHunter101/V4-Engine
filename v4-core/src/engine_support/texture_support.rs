use std::io::Write;

use wgpu::{Device, Queue, Sampler, Texture as WgpuTexture, TextureFormat, TextureView};

#[derive(Debug)]
pub struct Texture {
    format: TextureFormat,
    texture: WgpuTexture,
    view: TextureView,
    sampler: Sampler,
}

#[derive(Debug)]
pub struct StorageTexture {
    format: TextureFormat,
    texture: WgpuTexture,
    view: TextureView,
    access: wgpu::StorageTextureAccess,
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

    pub fn sampler_ref(&self) -> &Sampler {
        &self.sampler
    }

    pub fn sampler(self) -> Sampler {
        self.sampler
    }

    pub async fn from_path(
        path: &str,
        device: &Device,
        queue: &Queue,
        format: TextureFormat,
        is_storage: bool,
    ) -> tokio::io::Result<Self> {
        let raw_image = tokio::fs::read(path).await?;

        // TODO: Implement actual error handling
        let raw = image::load_from_memory(&raw_image).expect("Failed to create image");
        // let image = raw.into_rgba8();
        let bytes = raw.as_bytes();

        Ok(Self::from_bytes(
            bytes,
            raw.width(),
            raw.height(),
            device,
            queue,
            format,
            is_storage,
        ))
    }

    pub fn from_bytes(
        bytes: &[u8],
        width: u32,
        height: u32,
        device: &Device,
        queue: &Queue,
        format: TextureFormat,
        is_storage: bool,
    ) -> Self {
        let texture = Self::create_texture(device, width, height, format, is_storage);

        queue.write_texture(
            texture.texture_ref().as_image_copy(),
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(format.components() as u32 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        texture
    }

    pub fn output_image_native(image_data: Vec<u8>, texture_dims: (usize, usize), path: String) {
        let mut png_data = Vec::<u8>::with_capacity(image_data.len());
        let mut encoder = png::Encoder::new(
            std::io::Cursor::new(&mut png_data),
            texture_dims.0 as u32,
            texture_dims.1 as u32,
        );
        encoder.set_color(png::ColorType::Rgba);
        let mut png_writer = encoder.write_header().unwrap();
        png_writer.write_image_data(&image_data[..]).unwrap();
        png_writer.finish().unwrap();
        log::info!("PNG file encoded in memory.");

        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(&png_data[..]).unwrap();
        log::info!("PNG file written to disc as \"{}\".", path);
    }

    pub fn create_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: TextureFormat,
        is_storage: bool,
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

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            format,
            texture,
            view,
            sampler,
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 100.0,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        Self {
            format: texture.format(),
            texture,
            view,
            sampler,
        }
    }

    pub fn view_mut(&mut self) -> &mut TextureView {
        &mut self.view
    }
}
