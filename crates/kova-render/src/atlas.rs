//! Texture atlases for glyphs, images and rasterized vectors.
//!
//! Two atlases exist: a single channel coverage atlas (`Mono`, glyph masks,
//! tintable icons) and a premultiplied RGBA atlas (`Color`, images, emoji).
//! Each is a GPU texture array; when a layer fills up a new layer is added,
//! so the whole UI can still be drawn with one bind group and one draw call.

use etagere::{AllocId, BucketedAtlasAllocator, size2};
use kova_core::ImageId;
use kova_text::GlyphKey;
use rustc_hash::FxHashMap;
use std::borrow::Cow;

/// Edge length of every atlas layer, in texels.
pub const ATLAS_SIZE: u32 = 2048;
/// Empty texels around every tile so bilinear filtering never bleeds.
const PADDING: u32 = 1;

/// What is stored in the atlas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AtlasKey {
    Glyph(GlyphKey),
    Image(ImageId),
    /// A vector image rasterized at a specific device pixel size.
    Vector {
        id: ImageId,
        width: u32,
        height: u32,
        mono: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AtlasKind {
    /// 8-bit coverage, tinted by the primitive color.
    Mono,
    /// Premultiplied RGBA.
    Color,
}

impl AtlasKind {
    fn bytes_per_pixel(self) -> u32 {
        match self {
            AtlasKind::Mono => 1,
            AtlasKind::Color => 4,
        }
    }
}

/// Location of an image inside the atlas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtlasTile {
    pub kind: AtlasKind,
    pub layer: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// Offset of the bitmap from its anchor point (e.g. glyph origin), copied
    /// from [`AtlasImage::origin`].
    pub origin: (i32, i32),
}

impl AtlasTile {
    /// Normalized texture coordinates `[u0, v0, u1, v1]`.
    pub fn uv(&self) -> [f32; 4] {
        let s = ATLAS_SIZE as f32;
        [
            self.x as f32 / s,
            self.y as f32 / s,
            (self.x + self.width) as f32 / s,
            (self.y + self.height) as f32 / s,
        ]
    }
}

/// Pixel data to insert into the atlas.
pub struct AtlasImage<'a> {
    pub kind: AtlasKind,
    pub width: u32,
    pub height: u32,
    /// Tightly packed rows (`width * bytes_per_pixel` bytes each).
    pub data: Cow<'a, [u8]>,
    /// Offset of the bitmap's top-left corner from its anchor point (glyphs:
    /// `(left, -top)` relative to the pen position on the baseline).
    pub origin: (i32, i32),
}

pub(crate) struct Upload {
    pub kind: AtlasKind,
    pub layer: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

#[derive(Default)]
struct Layers {
    allocators: Vec<BucketedAtlasAllocator>,
}

impl Layers {
    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, AllocId, u32, u32)> {
        let size = size2((width + 2 * PADDING) as i32, (height + 2 * PADDING) as i32);
        for (layer, alloc) in self.allocators.iter_mut().enumerate() {
            if let Some(a) = alloc.allocate(size) {
                return Some((
                    layer as u32,
                    a.id,
                    a.rectangle.min.x as u32,
                    a.rectangle.min.y as u32,
                ));
            }
        }
        let mut alloc = BucketedAtlasAllocator::new(size2(ATLAS_SIZE as i32, ATLAS_SIZE as i32));
        let a = alloc.allocate(size)?;
        self.allocators.push(alloc);
        Some((
            (self.allocators.len() - 1) as u32,
            a.id,
            a.rectangle.min.x as u32,
            a.rectangle.min.y as u32,
        ))
    }
}

/// CPU side of the atlases: allocation, key lookup and pending uploads.
#[derive(Default)]
pub struct Atlas {
    tiles: FxHashMap<AtlasKey, Option<(AtlasTile, AllocId)>>,
    mono: Layers,
    color: Layers,
    pending: Vec<Upload>,
}

impl Atlas {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the tile for `key`, rasterizing it with `build` on first use.
    /// `build` returning `None` (e.g. whitespace glyphs) is cached as well.
    pub fn get_or_insert_with<'a>(
        &mut self,
        key: AtlasKey,
        build: impl FnOnce() -> Option<AtlasImage<'a>>,
    ) -> Option<AtlasTile> {
        if let Some(entry) = self.tiles.get(&key) {
            return entry.map(|(tile, _)| tile);
        }
        let entry = build().and_then(|image| self.insert(image));
        self.tiles.insert(key, entry);
        entry.map(|(tile, _)| tile)
    }

    fn insert(&mut self, image: AtlasImage<'_>) -> Option<(AtlasTile, AllocId)> {
        if image.width == 0 || image.height == 0 {
            return None;
        }
        if image.width + 2 * PADDING > ATLAS_SIZE || image.height + 2 * PADDING > ATLAS_SIZE {
            log::warn!(
                "kova atlas: {}x{} image exceeds atlas size {ATLAS_SIZE}",
                image.width,
                image.height
            );
            return None;
        }
        let expected = (image.width * image.height * image.kind.bytes_per_pixel()) as usize;
        if image.data.len() < expected {
            log::error!(
                "kova atlas: image data too short ({} < {expected})",
                image.data.len()
            );
            return None;
        }
        let layers = match image.kind {
            AtlasKind::Mono => &mut self.mono,
            AtlasKind::Color => &mut self.color,
        };
        let (layer, id, x, y) = layers.allocate(image.width, image.height)?;
        let tile = AtlasTile {
            kind: image.kind,
            layer,
            x: x + PADDING,
            y: y + PADDING,
            width: image.width,
            height: image.height,
            origin: image.origin,
        };
        self.pending.push(Upload {
            kind: image.kind,
            layer,
            x: tile.x,
            y: tile.y,
            width: image.width,
            height: image.height,
            data: image.data[..expected].to_vec(),
        });
        Some((tile, id))
    }

    /// Frees the tile for `key` (e.g. when an image is dropped).
    pub fn remove(&mut self, key: &AtlasKey) {
        if let Some(Some((tile, id))) = self.tiles.remove(key) {
            let layers = match tile.kind {
                AtlasKind::Mono => &mut self.mono,
                AtlasKind::Color => &mut self.color,
            };
            if let Some(alloc) = layers.allocators.get_mut(tile.layer as usize) {
                alloc.deallocate(id);
            }
        }
    }

    pub fn contains(&self, key: &AtlasKey) -> bool {
        self.tiles.contains_key(key)
    }

    pub fn layer_count(&self, kind: AtlasKind) -> u32 {
        match kind {
            AtlasKind::Mono => self.mono.allocators.len() as u32,
            AtlasKind::Color => self.color.allocators.len() as u32,
        }
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    pub(crate) fn take_uploads(&mut self) -> Vec<Upload> {
        std::mem::take(&mut self.pending)
    }
}

/// GPU textures backing the atlas.
pub(crate) struct AtlasTextures {
    pub mono: GpuLayers,
    pub color: GpuLayers,
}

pub(crate) struct GpuLayers {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub layers: u32,
    format: wgpu::TextureFormat,
}

impl GpuLayers {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, layers: u32, label: &str) -> Self {
        let layers = layers.max(1);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: layers,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some(label),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        GpuLayers {
            texture,
            view,
            layers,
            format,
        }
    }

    /// Grows the texture array to `needed` layers, preserving contents.
    /// Returns `true` if the texture was recreated.
    fn ensure(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        needed: u32,
        label: &str,
    ) -> bool {
        if needed <= self.layers {
            return false;
        }
        let new_layers = needed.max(self.layers * 2);
        let new = GpuLayers::new(device, self.format, new_layers, label);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("kova atlas grow"),
        });
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &new.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: self.layers,
            },
        );
        // Submit right away: queued texture writes issued afterwards are
        // ordered after this copy and cannot be overwritten by it.
        queue.submit(Some(encoder.finish()));
        *self = new;
        true
    }
}

impl AtlasTextures {
    pub fn new(device: &wgpu::Device) -> Self {
        AtlasTextures {
            mono: GpuLayers::new(device, wgpu::TextureFormat::R8Unorm, 1, "kova mono atlas"),
            color: GpuLayers::new(
                device,
                wgpu::TextureFormat::Rgba8Unorm,
                1,
                "kova color atlas",
            ),
        }
    }

    /// Uploads pending tiles. Returns `true` if textures were recreated (the
    /// bind group must then be rebuilt).
    pub fn sync(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, atlas: &mut Atlas) -> bool {
        let mut recreated = self.mono.ensure(
            device,
            queue,
            atlas.layer_count(AtlasKind::Mono),
            "kova mono atlas",
        );
        recreated |= self.color.ensure(
            device,
            queue,
            atlas.layer_count(AtlasKind::Color),
            "kova color atlas",
        );
        for upload in atlas.take_uploads() {
            let target = match upload.kind {
                AtlasKind::Mono => &self.mono,
                AtlasKind::Color => &self.color,
            };
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &target.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: upload.x,
                        y: upload.y,
                        z: upload.layer,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &upload.data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(upload.width * upload.kind.bytes_per_pixel()),
                    rows_per_image: Some(upload.height),
                },
                wgpu::Extent3d {
                    width: upload.width,
                    height: upload.height,
                    depth_or_array_layers: 1,
                },
            );
        }
        recreated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocation_and_caching() {
        let mut atlas = Atlas::new();
        let key = AtlasKey::Image(ImageId(1));
        let mut builds = 0;
        for _ in 0..3 {
            let tile = atlas.get_or_insert_with(key, || {
                builds += 1;
                Some(AtlasImage {
                    kind: AtlasKind::Color,
                    width: 4,
                    height: 2,
                    data: Cow::Owned(vec![255; 32]),
                    origin: (0, 0),
                })
            });
            let tile = tile.unwrap();
            assert_eq!((tile.width, tile.height), (4, 2));
            assert!(tile.x >= PADDING && tile.y >= PADDING);
        }
        assert_eq!(builds, 1);
        assert_eq!(atlas.take_uploads().len(), 1);

        // Negative results are cached too.
        let empty = AtlasKey::Image(ImageId(2));
        assert!(atlas.get_or_insert_with(empty, || None).is_none());
        assert!(atlas.contains(&empty));
    }

    #[test]
    fn grows_new_layers_when_full() {
        let mut atlas = Atlas::new();
        let big = (ATLAS_SIZE - 2 * PADDING) as usize;
        for i in 0..3 {
            let tile = atlas
                .get_or_insert_with(AtlasKey::Image(ImageId(100 + i)), || {
                    Some(AtlasImage {
                        kind: AtlasKind::Mono,
                        width: big as u32,
                        height: big as u32,
                        data: Cow::Owned(vec![0; big * big]),
                        origin: (0, 0),
                    })
                })
                .unwrap();
            assert_eq!(tile.layer, i as u32);
        }
        assert_eq!(atlas.layer_count(AtlasKind::Mono), 3);
        atlas.remove(&AtlasKey::Image(ImageId(100)));
        assert!(!atlas.contains(&AtlasKey::Image(ImageId(100))));
    }
}
