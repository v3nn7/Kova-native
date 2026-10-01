//! Images, SVG icons and custom drawing.

use crate::context::{MeasureCx, PaintCx};
use crate::element::{Element, ElementBase};
use kova_assets::{AssetCache, ImageData, SvgData};
use kova_core::{Bounds, Color, Corners, Point, Size};
use kova_layout::MeasureInput;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

/// How an image fills its box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ObjectFit {
    /// Stretch to the box.
    #[default]
    Fill,
    /// Scale to fit inside, preserving aspect ratio.
    Contain,
    /// Scale to cover the box, preserving aspect ratio (cropped).
    Cover,
}

/// Where an image comes from.
pub enum ImageSource {
    Data(Arc<ImageData>),
    Path(PathBuf),
    Static(&'static [u8]),
}

impl From<Arc<ImageData>> for ImageSource {
    fn from(d: Arc<ImageData>) -> Self {
        ImageSource::Data(d)
    }
}

impl From<PathBuf> for ImageSource {
    fn from(p: PathBuf) -> Self {
        ImageSource::Path(p)
    }
}

impl From<&'static [u8]> for ImageSource {
    fn from(b: &'static [u8]) -> Self {
        ImageSource::Static(b)
    }
}

impl<const N: usize> From<&'static [u8; N]> for ImageSource {
    fn from(b: &'static [u8; N]) -> Self {
        ImageSource::Static(b)
    }
}

/// A raster image.
pub struct Img {
    base: ElementBase,
    data: Option<Arc<ImageData>>,
    fit: ObjectFit,
    grayscale: bool,
}

/// Creates an image element. Decoding failures are logged and render nothing.
pub fn img(source: impl Into<ImageSource>) -> Img {
    let data = match source.into() {
        ImageSource::Data(d) => Ok(d),
        ImageSource::Path(p) => AssetCache::global().image_from_path(p),
        ImageSource::Static(b) => AssetCache::global().image_from_static(b),
    };
    let data = data.map_err(|e| log::warn!("kova img: {e}")).ok();
    Img {
        base: ElementBase::new(),
        data,
        fit: ObjectFit::Fill,
        grayscale: false,
    }
}

impl Img {
    pub fn object_fit(mut self, fit: ObjectFit) -> Self {
        self.fit = fit;
        self
    }

    pub fn grayscale(mut self, grayscale: bool) -> Self {
        self.grayscale = grayscale;
        self
    }
}

fn fit_rect(content: Bounds, intrinsic: Size, fit: ObjectFit) -> Bounds {
    if intrinsic.is_empty() || fit == ObjectFit::Fill {
        return content;
    }
    let sx = content.width() / intrinsic.width;
    let sy = content.height() / intrinsic.height;
    let s = if fit == ObjectFit::Contain {
        sx.min(sy)
    } else {
        sx.max(sy)
    };
    let size = intrinsic.scale(s);
    Bounds::new(
        Point::new(
            content.origin.x + (content.width() - size.width) * 0.5,
            content.origin.y + (content.height() - size.height) * 0.5,
        ),
        size,
    )
}

fn intrinsic_measure(intrinsic: Size, input: MeasureInput) -> Size {
    match (input.known_width, input.known_height) {
        (Some(w), Some(h)) => Size::new(w, h),
        (Some(w), None) if intrinsic.width > 0.0 => {
            Size::new(w, w * intrinsic.height / intrinsic.width)
        }
        (None, Some(h)) if intrinsic.height > 0.0 => {
            Size::new(h * intrinsic.width / intrinsic.height, h)
        }
        _ => intrinsic,
    }
}

impl Element for Img {
    fn base(&self) -> &ElementBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut ElementBase {
        &mut self.base
    }

    fn name(&self) -> &'static str {
        "img"
    }

    fn is_measured(&self) -> bool {
        true
    }

    fn measure(&mut self, _cx: &mut MeasureCx, input: MeasureInput) -> Size {
        intrinsic_measure(self.data.as_ref().map_or(Size::ZERO, |d| d.size()), input)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let Some(data) = self.data.clone() else {
            return;
        };
        let content = cx.content_bounds();
        let radii = self.base.style.corner_radii;
        let target = fit_rect(content, data.size(), self.fit);
        if self.fit == ObjectFit::Cover {
            cx.with_clip(content, radii, |cx| {
                cx.paint_image(&data, target, Corners::ZERO, self.grayscale)
            });
        } else {
            cx.paint_image(&data, target, radii, self.grayscale);
        }
    }
}

crate::impl_element_builder!(Img);

/// Where an SVG comes from.
pub enum SvgSource {
    Data(Arc<SvgData>),
    Path(PathBuf),
    Static(&'static [u8]),
}

impl From<Arc<SvgData>> for SvgSource {
    fn from(d: Arc<SvgData>) -> Self {
        SvgSource::Data(d)
    }
}

impl From<PathBuf> for SvgSource {
    fn from(p: PathBuf) -> Self {
        SvgSource::Path(p)
    }
}

impl From<&'static [u8]> for SvgSource {
    fn from(b: &'static [u8]) -> Self {
        SvgSource::Static(b)
    }
}

impl<const N: usize> From<&'static [u8; N]> for SvgSource {
    fn from(b: &'static [u8; N]) -> Self {
        SvgSource::Static(b)
    }
}

impl From<&'static str> for SvgSource {
    fn from(s: &'static str) -> Self {
        SvgSource::Static(s.as_bytes())
    }
}

/// A vector image, rasterized at the exact device resolution it is drawn at.
pub struct Svg {
    base: ElementBase,
    data: Option<Arc<SvgData>>,
    tint: Option<Color>,
    inherit_tint: bool,
}

/// Creates an SVG element (full color).
pub fn svg(source: impl Into<SvgSource>) -> Svg {
    let data = match source.into() {
        SvgSource::Data(d) => Ok(d),
        SvgSource::Path(p) => AssetCache::global().svg_from_path(p),
        SvgSource::Static(b) => AssetCache::global().svg_from_static(b),
    };
    let data = data.map_err(|e| log::warn!("kova svg: {e}")).ok();
    Svg {
        base: ElementBase::new(),
        data,
        tint: None,
        inherit_tint: false,
    }
}

/// Creates a monochrome icon tinted with the inherited text color.
pub fn icon(source: impl Into<SvgSource>) -> Svg {
    let mut s = svg(source);
    s.inherit_tint = true;
    s
}

impl Svg {
    /// Uses the SVG as a mask filled with `color`.
    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.tint = Some(color.into());
        self
    }
}

impl Element for Svg {
    fn base(&self) -> &ElementBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut ElementBase {
        &mut self.base
    }

    fn name(&self) -> &'static str {
        "svg"
    }

    fn is_measured(&self) -> bool {
        true
    }

    fn measure(&mut self, _cx: &mut MeasureCx, input: MeasureInput) -> Size {
        intrinsic_measure(self.data.as_ref().map_or(Size::ZERO, |d| d.size()), input)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let Some(data) = self.data.clone() else {
            return;
        };
        let tint = self.tint.or(if self.inherit_tint {
            Some(cx.text_color())
        } else {
            None
        });
        let target = fit_rect(cx.content_bounds(), data.size(), ObjectFit::Contain);
        cx.paint_svg(&data, target, tint);
    }
}

crate::impl_element_builder!(Svg);

/// Custom drawing. The closure runs every time the element is painted; any
/// signal it reads schedules a repaint when it changes.
pub struct Canvas {
    base: ElementBase,
    paint: Rc<dyn Fn(&mut PaintCx, Bounds)>,
}

/// Creates a canvas with a paint callback receiving the content bounds.
pub fn canvas(paint: impl Fn(&mut PaintCx, Bounds) + 'static) -> Canvas {
    Canvas {
        base: ElementBase::new(),
        paint: Rc::new(paint),
    }
}

impl Element for Canvas {
    fn base(&self) -> &ElementBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut ElementBase {
        &mut self.base
    }

    fn name(&self) -> &'static str {
        "canvas"
    }

    fn tracks_paint(&self) -> bool {
        true
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.content_bounds();
        (self.paint)(cx, bounds);
    }
}

crate::impl_element_builder!(Canvas);
