
use wgpu::{Device, Queue, Texture};
use crossfont::{self, Rasterize, Rasterizer, Size, FontKey, FontDesc, Metrics, GlyphKey, BitmapBuffer};
use std::collections::HashMap;

#[derive(Clone)]
pub struct Glyph {
    pub uv_top: f32,
    pub uv_left: f32,
    pub uv_width: f32,
    pub uv_height: f32,

    pub width: f32,
    pub height: f32,
    pub top: f32,
    pub left: f32,
}


pub struct Atlas {
    rasterizer: Rasterizer,
    glyphs: HashMap<GlyphKey, Glyph>,
    textures: Vec<Texture>,
    active_texture: usize,
    v_offset: u32,
    h_offset: u32,
    row_height: u32,
    h_size: u32,
    v_size: u32,
}

const DEFAULT_TEXTURE_SIZE: u32 = 4096;

impl Atlas {
    pub fn new(scale_factor: f32) -> Self {
        let rasterizer = Rasterizer::new(scale_factor, true).unwrap();

        Self {
            rasterizer,
            glyphs: HashMap::default(),
            textures: Vec::new(),
            active_texture: 0,
            v_offset: 0,
            h_offset: 0,
            row_height: 0,
            h_size: DEFAULT_TEXTURE_SIZE,
            v_size: DEFAULT_TEXTURE_SIZE,
        }
    }

    pub fn load_font(&mut self, font: &FontDesc, size: f32) -> (FontKey, Metrics) {
        let font_size = Size::new(size);
        let regular = self.rasterizer.load_font(font, font_size).unwrap();
        let gk = GlyphKey { font_key: regular, character: 'm', size: font_size };

        let metrics =  self.rasterizer.metrics(regular, font_size).unwrap();
        self.row_height = metrics.line_height as u32;
        return (regular, metrics);
    }

    pub fn texture_view(&mut self, device: &Device) -> wgpu::TextureView {
        let texture = self.get_or_create_texture(device).unwrap();

        texture.create_view(&wgpu::TextureViewDescriptor::default())
    }

    pub fn sampler(&self, device: &Device) -> wgpu::Sampler {
        device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        })
    }

    pub fn get_glyph(&mut self, device: &Device, queue: &Queue, key: GlyphKey) -> Option<Glyph> {
        if self.glyphs.contains_key(&key) {
            return match self.glyphs.get(&key) {
                Some(g) => Some(g.clone()),
                None => None,
            };
        }

        let rast_glyph = self.rasterizer.get_glyph(key).unwrap();

        let (target_x, target_y) = self.location_for(rast_glyph.width as u32, rast_glyph.height as u32);
        let metrics = self.rasterizer.metrics(key.font_key, key.size).unwrap();

        let texture = self.get_or_create_texture(device).unwrap();

        // Convert to rgba
        let buff = match rast_glyph.buffer {
            BitmapBuffer::Rgba(v) => {
                println!("Format: RGBA");
                v
            },
            BitmapBuffer::Rgb(v) => {
                println!("Format: RGB");
                let mut new_buff = Vec::with_capacity((v.len() / 3) * 4);
                for chunk in v.chunks(3) {
                    match chunk {
                        &[r,g,b] => {
                            new_buff.push(r);
                            new_buff.push(g);
                            new_buff.push(b);
                            new_buff.push(std::cmp::max(std::cmp::max(r,g),b));
                        }
                        _ => println!("Not chunk aligned"),
                    }
                }

                new_buff
            },
        };


        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d{
                    x: target_x, // TODO: Offset in the atlas
                    y: target_y, // TODO: Offset in the atlas
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &buff,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: std::num::NonZeroU32::new(4 * rast_glyph.width as u32),
                rows_per_image: std::num::NonZeroU32::new(rast_glyph.height as u32),
            },
            wgpu::Extent3d {
                width: rast_glyph.width as u32,
                height: rast_glyph.height as u32,
                depth_or_array_layers: 1,
            },
        );

        let g = Glyph{
            uv_top: target_y as f32 / self.v_size as f32,
            uv_left: target_x as f32 / self.h_size as f32,
            uv_height: (rast_glyph.height as f32) / self.v_size as f32,
            uv_width: (rast_glyph.width as f32) / self.h_size as f32,
            top: rast_glyph.top as f32 - metrics.descent, 
            left: rast_glyph.left as f32,
            width: rast_glyph.width as f32,
            height: rast_glyph.height as f32,
        };

        self.glyphs.insert(key, g);

        return match self.glyphs.get(&key) {
            Some(g) => Some(g.clone()),
            None => None,
        };
    }

    // location_for returns the next x/y in the atlas to store a texture of the given size
    fn location_for(&mut self, width: u32, height: u32) -> (u32, u32) {
        if self.row_height < height {
            // Can't store in this row...
            if (self.v_offset + self.row_height) > height {
                println!("We outta space!");
                panic!("Ran out of texture space");
            }
            self.v_offset += self.row_height;
            self.row_height = height;
        }
        if self.h_offset + width < self.h_size {
            // Have enough vertical space
            let x = self.h_offset;
            self.h_offset += width;
            return (x, self.v_offset);
        }
        return (self.h_offset, self.v_offset);
    }

    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        self.rasterizer.update_dpr(scale_factor);
    }

    fn get_or_create_texture(&mut self, device: &Device) -> Option<&wgpu::Texture> {
        if self.active_texture >= self.textures.len() {
            // Create first
            let texture = device.create_texture(
                &wgpu::TextureDescriptor {
                    size: wgpu::Extent3d {
                        width: DEFAULT_TEXTURE_SIZE,
                        height: DEFAULT_TEXTURE_SIZE,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    label: Some("Glyph Texture"),
                }
            );

            self.textures.push(texture);
            self.active_texture = self.textures.len() - 1;
        }
        Some(&self.textures[self.active_texture])
    }
}
