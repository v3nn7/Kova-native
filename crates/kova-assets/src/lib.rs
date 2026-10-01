//! Image and vector assets for Kova.
//!
//! * [`ImageData`] — decoded raster images (PNG, JPEG, WebP), stored as
//!   premultiplied RGBA ready for the GPU atlas.
//! * [`SvgData`] — parsed SVG documents, rasterized on demand at the exact
//!   device pixel size they are drawn at (crisp at any scale factor).
//! * [`AssetCache`] — deduplicates loads by source so the same file is
//!   decoded once no matter how many elements display it.

use kova_core::{ImageId, KovaError, KovaResult, Size};
use parking_lot::Mutex;
use resvg::{tiny_skia, usvg};
use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// A decoded raster image (premultiplied RGBA8).
#[derive(Debug)]
pub struct ImageData {
    id: ImageId,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl ImageData {
    /// Decodes PNG/JPEG/WebP bytes.
    pub fn decode(bytes: &[u8]) -> KovaResult<Arc<ImageData>> {
        let img = image::load_from_memory(bytes)
            .map_err(|e| KovaError::Asset(format!("image decode failed: {e}")))?;
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        Ok(Self::from_rgba(width, height, rgba.into_raw()))
    }

    /// Wraps straight-alpha RGBA8 pixels (premultiplies them).
    pub fn from_rgba(width: u32, height: u32, mut pixels: Vec<u8>) -> Arc<ImageData> {
        assert_eq!(
            pixels.len(),
            (width * height * 4) as usize,
            "pixel buffer size mismatch"
        );
        premultiply(&mut pixels);
        Arc::new(ImageData {
            id: ImageId::next(),
            width,
            height,
            pixels,
        })
    }

    pub fn open(path: impl AsRef<Path>) -> KovaResult<Arc<ImageData>> {
        let bytes = std::fs::read(path.as_ref()).map_err(|e| {
            KovaError::Asset(format!("cannot read {}: {e}", path.as_ref().display()))
        })?;
        Self::decode(&bytes)
    }

    pub fn id(&self) -> ImageId {
        self.id
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn size(&self) -> Size {
        Size::new(self.width as f32, self.height as f32)
    }

    /// Premultiplied RGBA8 pixels.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

fn premultiply(pixels: &mut [u8]) {
    for px in pixels.chunks_exact_mut(4) {
        let a = px[3] as u16;
        if a < 255 {
            px[0] = ((px[0] as u16 * a + 127) / 255) as u8;
            px[1] = ((px[1] as u16 * a + 127) / 255) as u8;
            px[2] = ((px[2] as u16 * a + 127) / 255) as u8;
        }
    }
}

/// A parsed SVG document.
pub struct SvgData {
    id: ImageId,
    tree: usvg::Tree,
}

impl std::fmt::Debug for SvgData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SvgData")
            .field("id", &self.id)
            .field("size", &self.size())
            .finish()
    }
}

impl SvgData {
    pub fn parse(bytes: &[u8]) -> KovaResult<Arc<SvgData>> {
        let tree = usvg::Tree::from_data(bytes, &usvg::Options::default())
            .map_err(|e| KovaError::Asset(format!("svg parse failed: {e}")))?;
        Ok(Arc::new(SvgData {
            id: ImageId::next(),
            tree,
        }))
    }

    pub fn open(path: impl AsRef<Path>) -> KovaResult<Arc<SvgData>> {
        let bytes = std::fs::read(path.as_ref()).map_err(|e| {
            KovaError::Asset(format!("cannot read {}: {e}", path.as_ref().display()))
        })?;
        Self::parse(&bytes)
    }

    pub fn id(&self) -> ImageId {
        self.id
    }

    /// Intrinsic size from the document's width/height/viewBox.
    pub fn size(&self) -> Size {
        let s = self.tree.size();
        Size::new(s.width(), s.height())
    }

    /// Rasterizes into premultiplied RGBA8 at exactly `width` x `height`
    /// pixels, scaling the document to fit (aspect ratio preserved, centered).
    pub fn rasterize(&self, width: u32, height: u32) -> Option<Vec<u8>> {
        let mut pixmap = tiny_skia::Pixmap::new(width.max(1), height.max(1))?;
        let size = self.tree.size();
        let scale = (width as f32 / size.width()).min(height as f32 / size.height());
        let dx = (width as f32 - size.width() * scale) * 0.5;
        let dy = (height as f32 - size.height() * scale) * 0.5;
        let transform = tiny_skia::Transform::from_scale(scale, scale).post_translate(dx, dy);
        resvg::render(&self.tree, transform, &mut pixmap.as_mut());
        Some(pixmap.take())
    }

    /// Rasterizes to an 8-bit coverage mask (alpha channel only), for
    /// monochrome icons tinted at draw time.
    pub fn rasterize_mask(&self, width: u32, height: u32) -> Option<Vec<u8>> {
        let rgba = self.rasterize(width, height)?;
        Some(rgba.chunks_exact(4).map(|px| px[3]).collect())
    }
}

/// Where an asset comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AssetSource {
    Path(PathBuf),
    /// Embedded bytes, identified by their address and length (for
    /// `include_bytes!` data, which is `'static`).
    Static(usize, usize),
}

impl AssetSource {
    pub fn from_static(bytes: &'static [u8]) -> Self {
        AssetSource::Static(bytes.as_ptr() as usize, bytes.len())
    }
}

/// Caches decoded assets by source. Thread safe, so decoding can happen on
/// background threads.
#[derive(Default)]
pub struct AssetCache {
    images: Mutex<FxHashMap<AssetSource, Result<Arc<ImageData>, String>>>,
    svgs: Mutex<FxHashMap<AssetSource, Result<Arc<SvgData>, String>>>,
}

impl AssetCache {
    /// The process wide cache.
    pub fn global() -> &'static AssetCache {
        static CACHE: OnceLock<AssetCache> = OnceLock::new();
        CACHE.get_or_init(AssetCache::default)
    }

    pub fn image_from_path(&self, path: impl AsRef<Path>) -> KovaResult<Arc<ImageData>> {
        let key = AssetSource::Path(path.as_ref().to_path_buf());
        self.load_image(key, || ImageData::open(path.as_ref()))
    }

    pub fn image_from_static(&self, bytes: &'static [u8]) -> KovaResult<Arc<ImageData>> {
        self.load_image(AssetSource::from_static(bytes), || ImageData::decode(bytes))
    }

    pub fn svg_from_path(&self, path: impl AsRef<Path>) -> KovaResult<Arc<SvgData>> {
        let key = AssetSource::Path(path.as_ref().to_path_buf());
        self.load_svg(key, || SvgData::open(path.as_ref()))
    }

    pub fn svg_from_static(&self, bytes: &'static [u8]) -> KovaResult<Arc<SvgData>> {
        self.load_svg(AssetSource::from_static(bytes), || SvgData::parse(bytes))
    }

    fn load_image(
        &self,
        key: AssetSource,
        load: impl FnOnce() -> KovaResult<Arc<ImageData>>,
    ) -> KovaResult<Arc<ImageData>> {
        if let Some(entry) = self.images.lock().get(&key) {
            return entry.clone().map_err(KovaError::Asset);
        }
        let result = load().map_err(|e| e.to_string());
        self.images.lock().insert(key, result.clone());
        result.map_err(KovaError::Asset)
    }

    fn load_svg(
        &self,
        key: AssetSource,
        load: impl FnOnce() -> KovaResult<Arc<SvgData>>,
    ) -> KovaResult<Arc<SvgData>> {
        if let Some(entry) = self.svgs.lock().get(&key) {
            return entry.clone().map_err(KovaError::Asset);
        }
        let result = load().map_err(|e| e.to_string());
        self.svgs.lock().insert(key, result.clone());
        result.map_err(KovaError::Asset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SVG: &[u8] =
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24">
        <circle cx="12" cy="12" r="10" fill="#ff0000"/></svg>"##;

    #[test]
    fn svg_rasterizes_at_requested_size() {
        let svg = SvgData::parse(SVG).unwrap();
        assert_eq!(svg.size(), Size::new(24.0, 24.0));
        let px = svg.rasterize(48, 48).unwrap();
        assert_eq!(px.len(), 48 * 48 * 4);
        let center = &px[((24 * 48 + 24) * 4)..((24 * 48 + 24) * 4 + 4)];
        assert_eq!(center, &[255, 0, 0, 255]);
        let corner = &px[0..4];
        assert_eq!(corner[3], 0);
        let mask = svg.rasterize_mask(12, 12).unwrap();
        assert_eq!(mask.len(), 144);
    }

    #[test]
    fn png_roundtrip_and_cache() {
        // Encode a 2x1 PNG in memory.
        let mut png = Vec::new();
        {
            let img =
                image::RgbaImage::from_raw(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 128]).unwrap();
            img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .unwrap();
        }
        let data = ImageData::decode(&png).unwrap();
        assert_eq!((data.width(), data.height()), (2, 1));
        // Second pixel is premultiplied.
        assert_eq!(&data.pixels()[4..8], &[0, 0, 128, 128]);

        let bytes: &'static [u8] = Box::leak(png.into_boxed_slice());
        let cache = AssetCache::default();
        let a = cache.image_from_static(bytes).unwrap();
        let b = cache.image_from_static(bytes).unwrap();
        assert!(Arc::ptr_eq(&a, &b));
        assert!(cache.image_from_path("does/not/exist.png").is_err());
    }
}
