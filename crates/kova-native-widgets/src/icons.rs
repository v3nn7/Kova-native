//! A small built-in icon set.
//!
//! Each constant is an SVG drawn on a 24×24 grid with 2 px round strokes.
//! Use them with [`icon`](crate::elements::icon), which tints the shape with
//! the current text color unless a color is given:
//!
//! ```ignore
//! icon(icons::SEARCH).size(16.0)
//! button("Delete").icon(icon(icons::TRASH)).danger()
//! ```

macro_rules! icons {
    ($($(#[$meta:meta])* $name:ident => $body:literal;)*) => {
        $(
            $(#[$meta])*
            pub const $name: &[u8] = concat!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">"##,
                $body,
                "</svg>"
            ).as_bytes();
        )*

        /// Every icon with its name, for galleries and pickers.
        pub const ALL: &[(&str, &[u8])] = &[$((stringify!($name), $name)),*];
    };
}

icons! {
    ACTIVITY => r#"<path d="M3 12h4l3-8 4 16 3-8h4"/>"#;
    ALERT_CIRCLE => r#"<circle cx="12" cy="12" r="9"/><path d="M12 7.5v5.5M12 16.5v.01"/>"#;
    ALERT_TRIANGLE => r#"<path d="M12 3.5 2.5 20h19z"/><path d="M12 10v4.5M12 17.2v.01"/>"#;
    ARROW_DOWN => r#"<path d="M12 5v14M6 13l6 6 6-6"/>"#;
    ARROW_LEFT => r#"<path d="M19 12H5M11 6l-6 6 6 6"/>"#;
    ARROW_RIGHT => r#"<path d="M5 12h14M13 6l6 6-6 6"/>"#;
    ARROW_UP => r#"<path d="M12 19V5M6 11l6-6 6 6"/>"#;
    BELL => r#"<path d="M6 16V11a6 6 0 0 1 12 0v5l1.5 2h-15z"/><path d="M10 21h4"/>"#;
    CALENDAR => r#"<rect x="3.5" y="5" width="17" height="15.5" rx="2"/><path d="M3.5 10h17M8 3v4M16 3v4"/>"#;
    CHECK => r#"<path d="M5 12.5 9.5 17 19 7.5"/>"#;
    CHECK_CIRCLE => r#"<circle cx="12" cy="12" r="9"/><path d="m8 12.5 2.8 2.8L16.5 9.5"/>"#;
    CHEVRON_DOWN => r#"<path d="m6 9 6 6 6-6"/>"#;
    CHEVRON_LEFT => r#"<path d="m15 6-6 6 6 6"/>"#;
    CHEVRON_RIGHT => r#"<path d="m9 6 6 6-6 6"/>"#;
    CHEVRON_UP => r#"<path d="m6 15 6-6 6 6"/>"#;
    CHEVRONS_UP_DOWN => r#"<path d="m7 9 5-5 5 5M7 15l5 5 5-5"/>"#;
    CLOCK => r#"<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3.5 2"/>"#;
    CLOSE => r#"<path d="M6 6l12 12M18 6 6 18"/>"#;
    CODE => r#"<path d="m8 7-5 5 5 5M16 7l5 5-5 5"/>"#;
    COMMAND => r#"<path d="M9 9V6.5A2.5 2.5 0 1 0 6.5 9H17.5A2.5 2.5 0 1 0 15 6.5v11a2.5 2.5 0 1 0 2.5-2.5h-11A2.5 2.5 0 1 0 9 17.5z"/>"#;
    COPY => r#"<rect x="8.5" y="8.5" width="12" height="12" rx="2"/><path d="M15.5 8.5V5.5a2 2 0 0 0-2-2h-8a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h3"/>"#;
    DATABASE => r#"<ellipse cx="12" cy="5.5" rx="7.5" ry="2.5"/><path d="M4.5 5.5v13c0 1.4 3.4 2.5 7.5 2.5s7.5-1.1 7.5-2.5v-13M4.5 12c0 1.4 3.4 2.5 7.5 2.5s7.5-1.1 7.5-2.5"/>"#;
    DOWNLOAD => r#"<path d="M12 4v11M7 10.5l5 5 5-5M4.5 20h15"/>"#;
    EDIT => r#"<path d="M4 20h4L19 9a2.8 2.8 0 0 0-4-4L4 16z"/><path d="m13.5 6.5 4 4"/>"#;
    EXTERNAL_LINK => r#"<path d="M14 4h6v6M20 4l-9 9"/><path d="M18 14v4.5a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 4 18.5v-11A1.5 1.5 0 0 1 5.5 6H10"/>"#;
    EYE => r#"<path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12z"/><circle cx="12" cy="12" r="3"/>"#;
    EYE_OFF => r#"<path d="M4 4l16 16M10.6 6A9.6 9.6 0 0 1 12 5.5c6 0 9.5 6.5 9.5 6.5a16 16 0 0 1-2.6 3.4M6.6 6.9C4 8.6 2.5 12 2.5 12S6 18.5 12 18.5a9 9 0 0 0 4.6-1.3"/><path d="M9.9 9.9a3 3 0 0 0 4.2 4.2"/>"#;
    FILE => r#"<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/>"#;
    FILTER => r#"<path d="M3.5 5h17l-6.5 8v5.5l-4 2V13z"/>"#;
    FOLDER => r#"<path d="M3.5 7a2 2 0 0 1 2-2h4l2 2.5h7a2 2 0 0 1 2 2V17a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z"/>"#;
    GLOBE => r#"<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.5 2.7 3.8 5.7 3.8 9s-1.3 6.3-3.8 9c-2.5-2.7-3.8-5.7-3.8-9S9.5 5.7 12 3z"/>"#;
    GRID => r#"<rect x="4" y="4" width="6.5" height="6.5" rx="1.2"/><rect x="13.5" y="4" width="6.5" height="6.5" rx="1.2"/><rect x="4" y="13.5" width="6.5" height="6.5" rx="1.2"/><rect x="13.5" y="13.5" width="6.5" height="6.5" rx="1.2"/>"#;
    HEART => r#"<path d="M12 20s-8-4.6-8-10.2A4.5 4.5 0 0 1 12 7a4.5 4.5 0 0 1 8 2.8C20 15.4 12 20 12 20z"/>"#;
    HOME => r#"<path d="M3.5 11 12 4l8.5 7"/><path d="M5.5 9.5V20h4.5v-6h4v6h4.5V9.5"/>"#;
    INBOX => r#"<path d="M3.5 13.5 6 5h12l2.5 8.5V19a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 19z"/><path d="M3.5 13.5H8l1.5 2.5h5l1.5-2.5h4.5"/>"#;
    INFO => r#"<circle cx="12" cy="12" r="9"/><path d="M12 11v5.5M12 7.5v.01"/>"#;
    KEY => r#"<circle cx="7.5" cy="15.5" r="4"/><path d="m10.5 12.5 9-9M16 7l2.5 2.5M14 9l2 2"/>"#;
    LINK => r#"<path d="M10 14a4.5 4.5 0 0 0 6.4 0l3-3a4.5 4.5 0 0 0-6.4-6.4l-1 1"/><path d="M14 10a4.5 4.5 0 0 0-6.4 0l-3 3a4.5 4.5 0 0 0 6.4 6.4l1-1"/>"#;
    LIST => r#"<path d="M9 6h11M9 12h11M9 18h11M4.5 6h.01M4.5 12h.01M4.5 18h.01"/>"#;
    LOCK => r#"<rect x="4.5" y="10.5" width="15" height="10" rx="2"/><path d="M8 10.5V7.5a4 4 0 0 1 8 0v3"/>"#;
    LOG_OUT => r#"<path d="M9.5 20H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h3.5M15.5 16.5 20 12l-4.5-4.5M20 12H9"/>"#;
    MENU => r#"<path d="M4 7h16M4 12h16M4 17h16"/>"#;
    MINUS => r#"<path d="M5 12h14"/>"#;
    MOON => r#"<path d="M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z"/>"#;
    MORE_HORIZONTAL => r#"<path d="M5.5 12h.01M12 12h.01M18.5 12h.01" stroke-width="3"/>"#;
    MORE_VERTICAL => r#"<path d="M12 5.5v.01M12 12v.01M12 18.5v.01" stroke-width="3"/>"#;
    PAUSE => r#"<path d="M8.5 5v14M15.5 5v14"/>"#;
    PLAY => r#"<path d="M7 4.5v15l12-7.5z"/>"#;
    PLUS => r#"<path d="M12 5v14M5 12h14"/>"#;
    REFRESH => r#"<path d="M19.5 12a7.5 7.5 0 1 1-2.2-5.3L20 9.5"/><path d="M20 4v5.5h-5.5"/>"#;
    SEARCH => r#"<circle cx="10.5" cy="10.5" r="6.5"/><path d="m15.5 15.5 5 5"/>"#;
    SERVER => r#"<rect x="3.5" y="4" width="17" height="7" rx="1.5"/><rect x="3.5" y="13" width="17" height="7" rx="1.5"/><path d="M7.5 7.5h.01M7.5 16.5h.01"/>"#;
    SETTINGS => r#"<circle cx="12" cy="12" r="3"/><path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M5.3 18.7l2.1-2.1M16.6 7.4l2.1-2.1"/>"#;
    SLIDERS => r#"<path d="M4 7h10M18 7h2M4 17h4M12 17h8"/><circle cx="16" cy="7" r="2"/><circle cx="10" cy="17" r="2"/>"#;
    SORT => r#"<path d="m7 9 5-5 5 5M7 15l5 5 5-5"/>"#;
    SORT_ASC => r#"<path d="m7 14 5-5 5 5"/>"#;
    SORT_DESC => r#"<path d="m7 10 5 5 5-5"/>"#;
    STAR => r#"<path d="m12 3 2.8 5.8 6.2.9-4.5 4.4 1.1 6.3L12 17.4l-5.6 3 1.1-6.3L3 9.7l6.2-.9z"/>"#;
    SUN => r#"<circle cx="12" cy="12" r="4"/><path d="M12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5.3 5.3l1.4 1.4M17.3 17.3l1.4 1.4M5.3 18.7l1.4-1.4M17.3 6.7l1.4-1.4"/>"#;
    TERMINAL => r#"<rect x="3" y="4.5" width="18" height="15" rx="2"/><path d="m7 9.5 3 2.5-3 2.5M12.5 15H17"/>"#;
    TRASH => r#"<path d="M4.5 6.5h15M9.5 6.5V4.5h5v2M6.5 6.5l1 13h9l1-13M10 10.5v6M14 10.5v6"/>"#;
    UPLOAD => r#"<path d="M12 16V5M7 9.5l5-5 5 5M4.5 20h15"/>"#;
    USER => r#"<circle cx="12" cy="8" r="4"/><path d="M4.5 20.5a7.5 7.5 0 0 1 15 0"/>"#;
    USERS => r#"<circle cx="9" cy="8" r="3.5"/><path d="M2.5 20a6.5 6.5 0 0 1 13 0M15.5 4.8a3.5 3.5 0 0 1 0 6.4M18 14a6.5 6.5 0 0 1 3.5 6"/>"#;
    ZAP => r#"<path d="M13 2.5 4.5 13.5H12l-1 8 8.5-11H12z"/>"#;
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_icon_parses_and_rasterizes() {
        for (name, svg) in super::ALL {
            let data =
                kova_native_assets::SvgData::parse(svg).unwrap_or_else(|e| panic!("{name}: {e}"));
            let mask = data
                .rasterize_mask(24, 24)
                .unwrap_or_else(|| panic!("{name}: rasterize"));
            assert!(mask.iter().any(|a| *a > 0), "{name} draws nothing");
        }
    }
}
