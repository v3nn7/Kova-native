//! Design tokens.

use kova_native_core::{Color, Owner, Signal, rgb, rgba};
use std::cell::OnceCell;
use std::rc::Rc;

/// Colors, radii and type scale used by the built-in widgets.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub name: &'static str,
    pub dark: bool,
    /// Window background.
    pub background: Color,
    /// Cards, panels.
    pub surface: Color,
    /// Hovered / elevated surfaces.
    pub surface_raised: Color,
    /// Inputs, wells, tracks.
    pub surface_sunken: Color,
    pub border: Color,
    pub border_strong: Color,
    pub text: Color,
    pub text_muted: Color,
    pub text_subtle: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub accent_active: Color,
    /// Text drawn on top of `accent`.
    pub accent_text: Color,
    /// Translucent accent for selections, badges, focus rings.
    pub accent_soft: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub radius_small: f32,
    pub radius: f32,
    pub radius_large: f32,
    pub font_size: f32,
}

impl Theme {
    /// Kova Native's default dark theme.
    pub fn dark() -> Theme {
        Theme {
            name: "Kova Native Dark",
            dark: true,
            background: rgb(0x0c0d11),
            surface: rgb(0x15171d),
            surface_raised: rgb(0x1c1f27),
            surface_sunken: rgb(0x101116),
            border: rgb(0x252932),
            border_strong: rgb(0x343a47),
            text: rgb(0xeceef3),
            text_muted: rgb(0x9da3b2),
            text_subtle: rgb(0x6a7080),
            accent: rgb(0x7c5cff),
            accent_hover: rgb(0x8d72ff),
            accent_active: rgb(0x6b4af0),
            accent_text: rgb(0xffffff),
            accent_soft: rgba(0x7c5cff2e),
            success: rgb(0x34d399),
            warning: rgb(0xfbbf24),
            danger: rgb(0xf4506a),
            radius_small: 6.0,
            radius: 10.0,
            radius_large: 16.0,
            font_size: 14.0,
        }
    }

    /// A clean light theme.
    pub fn light() -> Theme {
        Theme {
            name: "Kova Native Light",
            dark: false,
            background: rgb(0xf4f5f8),
            surface: rgb(0xffffff),
            surface_raised: rgb(0xf8f9fb),
            surface_sunken: rgb(0xeceef2),
            border: rgb(0xe2e5eb),
            border_strong: rgb(0xccd1da),
            text: rgb(0x14161c),
            text_muted: rgb(0x5d6473),
            text_subtle: rgb(0x8b92a1),
            accent: rgb(0x6d4aff),
            accent_hover: rgb(0x7d5fff),
            accent_active: rgb(0x5b37f0),
            accent_text: rgb(0xffffff),
            accent_soft: rgba(0x6d4aff24),
            success: rgb(0x10b981),
            warning: rgb(0xd97706),
            danger: rgb(0xe11d48),
            radius_small: 6.0,
            radius: 10.0,
            radius_large: 16.0,
            font_size: 14.0,
        }
    }
}

impl Theme {
    /// Returns a copy of this theme using `accent` as its accent color.
    /// Hover, pressed and soft variants are derived from it.
    ///
    /// ```ignore
    /// set_theme(Theme::dark().with_accent(rgb(0x14b8a6)));
    /// ```
    pub fn with_accent(mut self, accent: impl Into<Color>) -> Theme {
        let accent = accent.into();
        self.accent = accent;
        self.accent_hover = accent.lighten(0.06);
        self.accent_active = accent.darken(0.06);
        self.accent_soft = accent.with_alpha(if self.dark { 0.18 } else { 0.14 });
        self.accent_text = if accent.luminance() > 0.4 {
            rgb(0x111318)
        } else {
            rgb(0xffffff)
        };
        self
    }

    /// Named accent colors that pair well with both built-in themes.
    pub const ACCENTS: [(&'static str, u32); 6] = [
        ("Violet", 0x7c5cff),
        ("Blue", 0x3b82f6),
        ("Teal", 0x14b8a6),
        ("Emerald", 0x22c55e),
        ("Amber", 0xf59e0b),
        ("Rose", 0xf43f5e),
    ];
}

impl Default for Theme {
    fn default() -> Self {
        Theme::dark()
    }
}

thread_local! {
    static THEME: OnceCell<Signal<Rc<Theme>>> = const { OnceCell::new() };
}

fn theme_signal() -> Signal<Rc<Theme>> {
    THEME.with(|cell| {
        *cell.get_or_init(|| {
            // Owned by a dedicated root so no UI rebuild can dispose it.
            Owner::new_root().with(|| kova_native_core::signal(Rc::new(Theme::dark())))
        })
    })
}

/// The current theme. Reading it inside a reactive scope (a region, a
/// `bind` closure, the window builder) subscribes to theme changes.
pub fn theme() -> Rc<Theme> {
    theme_signal().get()
}

/// Replaces the current theme; dependent UI updates automatically.
pub fn set_theme(theme: Theme) {
    theme_signal().set(Rc::new(theme));
}
