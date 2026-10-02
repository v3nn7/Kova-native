//! Headless GPU tests: render real scenes and inspect pixels.
//!
//! Skipped (with a message) when no GPU adapter is available.

use super::*;
use kova_native_core::{Color, Corners, Point, bounds, linear_gradient, rgb};
use std::borrow::Cow;
use std::sync::Mutex;

static GPU_LOCK: Mutex<()> = Mutex::new(());

fn with_gpu(f: impl FnOnce(&GpuContext, &mut Renderer)) {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let gpu = match GpuContext::new_headless() {
        Ok(gpu) => gpu,
        Err(e) => {
            eprintln!("skipping GPU test: {e}");
            return;
        }
    };
    let mut renderer = Renderer::new(&gpu);
    f(&gpu, &mut renderer);
}

fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn close(a: [u8; 4], b: [u8; 4], tolerance: i32) -> bool {
    a.iter()
        .zip(b.iter())
        .all(|(x, y)| (*x as i32 - *y as i32).abs() <= tolerance)
}

#[test]
fn renders_solid_and_rounded_quads() {
    with_gpu(|gpu, renderer| {
        let mut scene = Scene::new();
        let state = DrawState::default();
        scene.push_quad(
            &Quad::filled(bounds(0.0, 0.0, 32.0, 32.0), rgb(0xff0000)),
            &state,
        );
        scene.push_quad(
            &Quad::filled(bounds(32.0, 0.0, 32.0, 32.0), rgb(0x00ff00)).radii(16.0),
            &state,
        );
        let px = renderer.render_to_rgba(gpu, &scene, 64, 32, Color::BLACK);
        assert!(
            close(pixel(&px, 64, 16, 16), [255, 0, 0, 255], 1),
            "{:?}",
            pixel(&px, 64, 16, 16)
        );
        // Center of the circle is green, its bounding box corner is background.
        assert!(close(pixel(&px, 64, 48, 16), [0, 255, 0, 255], 1));
        assert!(
            close(pixel(&px, 64, 33, 1), [0, 0, 0, 255], 2),
            "{:?}",
            pixel(&px, 64, 33, 1)
        );
        // Edge pixels are antialiased (partially covered).
        let edge = pixel(&px, 64, 48, 0);
        assert!(edge[1] > 20 && edge[1] < 255, "{edge:?}");
    });
}

#[test]
fn renders_borders_gradients_and_clipping() {
    with_gpu(|gpu, renderer| {
        let mut scene = Scene::new();
        let state = DrawState::default();
        scene.push_quad(
            &Quad::filled(bounds(0.0, 0.0, 40.0, 40.0), rgb(0x0000ff)).border(4.0, rgb(0xffffff)),
            &state,
        );
        scene.push_quad(
            &Quad::filled(
                bounds(40.0, 0.0, 40.0, 40.0),
                linear_gradient(90.0, rgb(0x000000), rgb(0xffffff)),
            ),
            &state,
        );
        let clipped = DrawState {
            clip: ContentMask::new(bounds(80.0, 0.0, 10.0, 40.0)),
            ..Default::default()
        };
        scene.push_quad(
            &Quad::filled(bounds(80.0, 0.0, 40.0, 40.0), rgb(0xff0000)),
            &clipped,
        );
        let px = renderer.render_to_rgba(gpu, &scene, 120, 40, Color::BLACK);
        assert!(
            close(pixel(&px, 120, 1, 20), [255, 255, 255, 255], 1),
            "border"
        );
        assert!(close(pixel(&px, 120, 20, 20), [0, 0, 255, 255], 1), "fill");
        let left = pixel(&px, 120, 42, 20)[0];
        let right = pixel(&px, 120, 78, 20)[0];
        assert!(left < 30 && right > 225, "gradient {left} -> {right}");
        assert!(
            close(pixel(&px, 120, 85, 20), [255, 0, 0, 255], 1),
            "inside clip"
        );
        assert!(
            close(pixel(&px, 120, 100, 20), [0, 0, 0, 255], 1),
            "outside clip"
        );
    });
}

#[test]
fn renders_shadows_and_transforms() {
    with_gpu(|gpu, renderer| {
        let mut scene = Scene::new();
        let state = DrawState::default();
        scene.push_shadow(
            &Shadow {
                bounds: bounds(20.0, 20.0, 40.0, 40.0),
                radii: Corners::all(8.0),
                color: Color::BLACK.with_alpha(1.0),
                offset: Point::new(0.0, 0.0),
                blur: 16.0,
                spread: 0.0,
                inset: false,
            },
            &state,
        );
        // Rotated 45° square around its center.
        let rotated = DrawState {
            transform: kova_native_core::Transform2D::rotate(std::f32::consts::FRAC_PI_4)
                .around(Point::new(100.0, 40.0)),
            ..Default::default()
        };
        scene.push_quad(
            &Quad::filled(bounds(90.0, 30.0, 20.0, 20.0), rgb(0xffffff)),
            &rotated,
        );
        let px = renderer.render_to_rgba(gpu, &scene, 140, 80, rgb(0x808080));
        // The shadow darkens the background around and under the box...
        let under = pixel(&px, 140, 40, 40)[0];
        let near = pixel(&px, 140, 64, 40)[0];
        let far = pixel(&px, 140, 2, 78)[0];
        assert!(
            under < near && near < far,
            "shadow falloff {under} {near} {far}"
        );
        // ...and the rotated square covers its center and the point at its
        // former corner direction rotated by 45° (top middle).
        assert!(close(pixel(&px, 140, 100, 40), [255, 255, 255, 255], 1));
        assert!(
            pixel(&px, 140, 100, 27)[0] > 200,
            "rotation extends the square upwards"
        );
    });
}

#[test]
fn renders_glyphs_from_atlas() {
    with_gpu(|gpu, renderer| {
        let mut ts = kova_native_text::TextSystem::new();
        let mut layout = kova_native_text::TextLayout::new();
        layout.set(
            &mut ts,
            "Rust",
            &kova_native_text::TextStyle {
                size: 32.0,
                ..Default::default()
            },
        );
        layout.layout(&mut ts, None);
        let mut scene = Scene::new();
        let state = DrawState::default();
        let mut drawn = 0;
        layout.for_each_glyph(Point::new(4.0, 4.0), 1.0, |g| {
            let tile = renderer
                .atlas()
                .get_or_insert_with(AtlasKey::Glyph(g.key), || {
                    ts.rasterize(g.key).map(|r| AtlasImage {
                        kind: if r.is_color {
                            AtlasKind::Color
                        } else {
                            AtlasKind::Mono
                        },
                        width: r.width,
                        height: r.height,
                        data: Cow::Owned(r.data),
                        origin: (r.left, -r.top),
                    })
                });
            if let Some(tile) = tile {
                let b = bounds(
                    (g.x + tile.origin.0) as f32,
                    (g.y + tile.origin.1) as f32,
                    tile.width as f32,
                    tile.height as f32,
                );
                scene.push_mono_sprite(b, &tile, Color::WHITE, &state);
                drawn += 1;
            }
        });
        assert_eq!(drawn, 4);
        let px = renderer.render_to_rgba(gpu, &scene, 120, 50, Color::BLACK);
        let lit = px.as_chunks::<4>().0.iter().filter(|p| p[0] > 128).count();
        assert!(lit > 100, "text pixels: {lit}");
    });
}

#[test]
fn backdrop_blur_path_renders() {
    with_gpu(|gpu, renderer| {
        let mut scene = Scene::new();
        let state = DrawState::default();
        // Hard black/white edge, then a frosted panel across it.
        scene.push_quad(
            &Quad::filled(bounds(0.0, 0.0, 50.0, 40.0), rgb(0xffffff)),
            &state,
        );
        scene.push_backdrop_blur(
            bounds(25.0, 0.0, 50.0, 40.0),
            Corners::all(0.0),
            12.0,
            Color::TRANSPARENT,
            &state,
        );
        assert_eq!(scene.stats().draw_calls, 2);
        let px = renderer.render_to_rgba(gpu, &scene, 100, 40, Color::BLACK);
        // Unblurred left side stays white; inside the panel the edge is smeared.
        assert!(pixel(&px, 100, 10, 20)[0] > 250);
        let at_edge = pixel(&px, 100, 52, 20)[0];
        assert!(
            at_edge > 40 && at_edge < 215,
            "blurred edge value {at_edge}"
        );
    });
}

#[test]
fn backdrop_targets_follow_post_buffer_growth() {
    with_gpu(|gpu, renderer| {
        let state = DrawState::default();
        // Establish the original effect textures and scene bind group.
        let mut first = Scene::new();
        first.push_quad(
            &Quad::filled(bounds(0.0, 0.0, 80.0, 40.0), rgb(0xff0000)),
            &state,
        );
        first.push_backdrop_blur(
            bounds(0.0, 0.0, 10.0, 40.0),
            Corners::ZERO,
            4.0,
            Color::TRANSPARENT,
            &state,
        );
        renderer.render_to_rgba(gpu, &first, 80, 40, Color::BLACK);

        // Six captures exceed the initial 16 uniform slots (6 * 3 + blit).
        let mut second = Scene::new();
        second.push_quad(
            &Quad::filled(bounds(0.0, 0.0, 80.0, 40.0), rgb(0x00ff00)),
            &state,
        );
        for i in 0..6 {
            second.push_backdrop_blur(
                bounds(i as f32 * 10.0, 0.0, 10.0, 40.0),
                Corners::ZERO,
                4.0,
                Color::TRANSPARENT,
                &state,
            );
        }
        let px = renderer.render_to_rgba(gpu, &second, 80, 40, Color::BLACK);
        for x in [5, 15, 25, 35, 45, 55] {
            assert!(
                close(pixel(&px, 80, x, 20), [0, 255, 0, 255], 2),
                "stale backdrop at {x}: {:?}",
                pixel(&px, 80, x, 20)
            );
        }
    });
}

#[test]
fn backdrop_respects_group_opacity() {
    with_gpu(|gpu, renderer| {
        let render = |renderer: &mut Renderer, opacity: f32| {
            let mut scene = Scene::new();
            scene.push_quad(
                &Quad::filled(bounds(0.0, 0.0, 50.0, 40.0), Color::WHITE),
                &DrawState::default(),
            );
            scene.push_backdrop_blur(
                bounds(25.0, 0.0, 50.0, 40.0),
                Corners::ZERO,
                12.0,
                Color::TRANSPARENT,
                &DrawState {
                    opacity,
                    ..Default::default()
                },
            );
            renderer.render_to_rgba(gpu, &scene, 100, 40, Color::BLACK)
        };
        let full = render(renderer, 1.0);
        let half = render(renderer, 0.5);
        let a = pixel(&full, 100, 52, 20)[0] as f32;
        let b = pixel(&half, 100, 52, 20)[0] as f32;
        assert!(a > 40.0);
        assert!(
            (b - a * 0.5).abs() < 3.0,
            "backdrop opacity: full={a}, half={b}"
        );
    });
}

#[test]
fn layers_composite_through_gradient_masks() {
    with_gpu(|gpu, renderer| {
        let mut scene = Scene::new();
        let state = DrawState::default();
        scene.push_layer();
        scene.push_quad(
            &Quad::filled(bounds(0.0, 0.0, 100.0, 20.0), rgb(0xffffff)),
            &state,
        );
        // Opaque on the left, transparent on the right.
        scene.pop_layer(
            bounds(0.0, 0.0, 100.0, 20.0),
            LayerMask::LinearGradient {
                angle: 90.0,
                from: 1.0,
                to: 0.0,
                start: 0.0,
                end: 1.0,
            },
            &state,
        );
        let px = renderer.render_to_rgba(gpu, &scene, 100, 20, Color::BLACK);
        let left = pixel(&px, 100, 2, 10)[0];
        let middle = pixel(&px, 100, 50, 10)[0];
        let right = pixel(&px, 100, 97, 10)[0];
        assert!(left > 240, "left {left}");
        assert!((100..160).contains(&middle), "middle {middle}");
        assert!(right < 15, "right {right}");
    });
}

#[test]
fn layers_composite_through_atlas_tile_masks_and_nest() {
    with_gpu(|gpu, renderer| {
        // A 16x16 mono mask: a filled disc.
        let size = 16u32;
        let disc: Vec<u8> = (0..size * size)
            .map(|i| {
                let (x, y) = ((i % size) as f32 + 0.5 - 8.0, (i / size) as f32 + 0.5 - 8.0);
                if x * x + y * y <= 49.0 { 255 } else { 0 }
            })
            .collect();
        let tile = renderer
            .atlas()
            .get_or_insert_with(AtlasKey::Custom(7), || {
                Some(AtlasImage {
                    kind: AtlasKind::Mono,
                    width: size,
                    height: size,
                    data: Cow::Owned(disc),
                    origin: (0, 0),
                })
            })
            .expect("tile");
        let mut scene = Scene::new();
        let state = DrawState::default();
        scene.push_layer();
        scene.push_layer();
        scene.push_quad(
            &Quad::filled(bounds(0.0, 0.0, 64.0, 64.0), rgb(0x00ff00)),
            &state,
        );
        scene.pop_layer(bounds(0.0, 0.0, 64.0, 64.0), LayerMask::None, &state);
        // An empty layer composites nothing.
        scene.push_layer();
        scene.pop_layer(bounds(0.0, 0.0, 64.0, 64.0), LayerMask::None, &state);
        scene.pop_layer(bounds(0.0, 0.0, 64.0, 64.0), LayerMask::Tile(tile), &state);
        assert_eq!(scene.layer_depth(), 0);
        let px = renderer.render_to_rgba(gpu, &scene, 64, 64, Color::BLACK);
        assert!(
            close(pixel(&px, 64, 32, 32), [0, 255, 0, 255], 2),
            "disc center {:?}",
            pixel(&px, 64, 32, 32)
        );
        assert!(
            close(pixel(&px, 64, 2, 2), [0, 0, 0, 255], 2),
            "outside the disc {:?}",
            pixel(&px, 64, 2, 2)
        );
    });
}

#[test]
fn custom_shaders_draw_with_parameters_and_batch() {
    with_gpu(|gpu, renderer| {
        let shader = register_shader(
            "split",
            r#"
fn shade(input: ShaderInput) -> vec4<f32> {
    if input.uv.x < 0.5 {
        return input.params0;
    }
    return vec4<f32>(input.params1.rgb, 1.0);
}
"#,
        )
        .expect("valid shader");
        let mut scene = Scene::new();
        let state = DrawState::default();
        for i in 0..2 {
            scene.push_custom(
                shader,
                bounds(i as f32 * 40.0, 0.0, 40.0, 20.0),
                Corners::ZERO,
                [1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0],
                0.0,
                Color::WHITE,
                1.0,
                &state,
            );
        }
        scene.push_quad(
            &Quad::filled(bounds(80.0, 0.0, 20.0, 20.0), rgb(0x00ff00)),
            &state,
        );
        assert_eq!(scene.stats().draw_calls, 2, "custom run + built-in run");
        let px = renderer.render_to_rgba(gpu, &scene, 100, 20, Color::BLACK);
        assert!(
            close(pixel(&px, 100, 5, 10), [255, 0, 0, 255], 1),
            "params0"
        );
        assert!(
            close(pixel(&px, 100, 35, 10), [0, 0, 255, 255], 1),
            "params1"
        );
        assert!(
            close(pixel(&px, 100, 45, 10), [255, 0, 0, 255], 1),
            "second instance"
        );
        assert!(
            close(pixel(&px, 100, 90, 10), [0, 255, 0, 255], 1),
            "built-in after"
        );
    });
}
