//! Headless renders of synthetic frames. Each test passes with a note when the machine has no GPU
//! adapter, as CI runners often do not.

use s2_assets::Palette;

use super::*;

fn renderer() -> Option<Renderer> {
    match Renderer::headless() {
        Ok(renderer) => Some(renderer),
        Err(e) => {
            eprintln!("skipped: no GPU adapter ({e})");
            None
        }
    }
}

/// Entry `i` is (i, 255 − i, i / 2).
fn palette() -> Palette {
    let mut palette = Palette::default();
    for (i, colour) in palette.0.iter_mut().enumerate() {
        let i = u8::try_from(i).unwrap();
        *colour = [i, 255 - i, i / 2];
    }
    palette
}

/// Index `(x + 3·y) mod 256` at each pixel.
fn pattern(width: u32, height: u32) -> Frame {
    let mut frame = Frame::new(width, height, 0);
    let mut surface = frame.surface();
    for y in 0..height {
        if let Some(row) = surface.row_mut(y) {
            for (x, pixel) in row.iter_mut().enumerate() {
                *pixel = u8::try_from((x + 3 * y as usize) % 256).unwrap();
            }
        }
    }
    frame
}

fn zoom(quarters: u8) -> Zoom {
    Zoom::from_quarters(quarters).unwrap()
}

#[test]
fn whole_zoom_matches_the_cpu_resolve() {
    let Some(mut renderer) = renderer() else { return };
    renderer.set_palette(&palette());
    let frame = pattern(70, 45);
    let image = renderer.capture(&frame, Zoom::ONE, 70, 45, None).unwrap();
    assert_eq!(image, frame.to_image(&palette()));
}

#[test]
fn whole_zooms_repeat_pixels() {
    let Some(mut renderer) = renderer() else { return };
    renderer.set_palette(&palette());
    let frame = pattern(20, 10);
    let image = renderer.capture(&frame, zoom(12), 60, 30, None).unwrap();
    let expected = frame.to_image(&palette());
    for y in 0..30 {
        for x in 0..60 {
            assert_eq!(image.pixel(x, y), expected.pixel(x / 3, y / 3), "({x}, {y})");
        }
    }
}

#[test]
fn palette_changes_need_no_new_frame() {
    let Some(mut renderer) = renderer() else { return };
    let frame = Frame::new(8, 8, 240);
    renderer.set_palette(&palette());
    let before = renderer.capture(&frame, Zoom::ONE, 8, 8, None).unwrap();
    let mut cycled = palette();
    cycled.0[240] = [1, 2, 3];
    renderer.set_palette(&cycled);
    let after = renderer.capture(&frame, Zoom::ONE, 8, 8, None).unwrap();
    assert_eq!(before.pixel(4, 4), Some([240, 15, 120, 255]));
    assert_eq!(after.pixel(4, 4), Some([1, 2, 3, 255]));
}

#[test]
fn zooming_out_averages() {
    let Some(mut renderer) = renderer() else { return };
    let mut palette = Palette::default();
    palette.0[1] = [200, 100, 0];
    let mut frame = Frame::new(4, 4, 0);
    let mut surface = frame.surface();
    for y in 0..4 {
        if let Some(row) = surface.row_mut(y) {
            row.copy_from_slice(&[0, 1, 0, 1]);
        }
    }
    renderer.set_palette(&palette);
    let image = renderer.capture(&frame, zoom(2), 2, 2, None).unwrap();
    let [r, g, b, _] = image.pixel(0, 0).unwrap();
    assert!(
        (95..=105).contains(&r) && (45..=55).contains(&g) && b == 0,
        "{r} {g} {b}"
    );
}

#[test]
fn bad_sizes_are_errors() {
    let Some(mut renderer) = renderer() else { return };
    let frame = Frame::new(4, 4, 0);
    assert!(matches!(
        renderer.capture(&frame, Zoom::ONE, 0, 4, None),
        Err(Error::EmptyOutput { .. })
    ));
    assert!(matches!(
        renderer.capture(&Frame::new(0, 4, 0), Zoom::ONE, 4, 4, None),
        Err(Error::EmptyOutput { .. })
    ));
    let side = renderer.max_world_size() + 1;
    assert!(matches!(
        renderer.capture(&Frame::new(side, 1, 0), Zoom::ONE, 4, 4, None),
        Err(Error::ViewTooLarge { .. })
    ));
    assert!(matches!(
        renderer.render(&frame, Zoom::ONE, None),
        Err(Error::Headless)
    ));
}

#[test]
fn the_interface_layer_draws_at_its_own_scale_inside_its_patches() {
    let Some(mut renderer) = renderer() else { return };
    renderer.set_palette(&palette());
    let view = Frame::new(40, 40, 7);
    let mut layer = Frame::new(20, 20, 0);
    for y in 0..20 {
        for x in 0..20 {
            layer.set(x, y, 200);
        }
    }
    let patches = [Patch {
        x: 2,
        y: 3,
        width: 4,
        height: 5,
    }];
    let image = renderer
        .capture(
            &view,
            Zoom::ONE,
            40,
            40,
            Some(Overlay {
                frame: &layer,
                scale: 2,
                patches: &patches,
            }),
        )
        .unwrap();
    let colour = |index: u8| {
        let [r, g, b] = palette().0[index as usize];
        [r, g, b, 255]
    };
    assert_eq!(image.pixel(4, 6), Some(colour(200)));
    assert_eq!(image.pixel(11, 15), Some(colour(200)));
    assert_eq!(image.pixel(3, 6), Some(colour(7)));
    assert_eq!(image.pixel(12, 16), Some(colour(7)));
    assert_eq!(image.pixel(0, 0), Some(colour(7)));
}

#[test]
fn an_interface_layer_with_no_patches_leaves_the_view_alone() {
    let Some(mut renderer) = renderer() else { return };
    renderer.set_palette(&palette());
    let view = pattern(30, 20);
    let layer = Frame::new(30, 20, 200);
    let image = renderer
        .capture(
            &view,
            Zoom::ONE,
            30,
            20,
            Some(Overlay {
                frame: &layer,
                scale: 1,
                patches: &[],
            }),
        )
        .unwrap();
    assert_eq!(image, view.to_image(&palette()));
}

#[test]
fn a_fitted_placement_keeps_the_aspect_and_centres() {
    let wide = Placement::fitted((1920, 1080), (4, 3));
    assert_eq!(
        wide,
        Placement {
            x: 240,
            y: 0,
            width: 1440,
            height: 1080
        }
    );
    let tall = Placement::fitted((1280, 1024), (4, 3));
    assert_eq!(
        tall,
        Placement {
            x: 0,
            y: 32,
            width: 1280,
            height: 960
        }
    );
    let exact = Placement::fitted((1280, 960), (4, 3));
    assert_eq!(
        exact,
        Placement {
            x: 0,
            y: 0,
            width: 1280,
            height: 960
        }
    );
}

#[test]
fn a_placed_frame_stretches_over_its_place_with_black_around_it() {
    let Some(mut renderer) = renderer() else { return };
    renderer.set_palette(&palette());
    let frame = pattern(4, 2);
    let placed = Placement {
        x: 2,
        y: 3,
        width: 8,
        height: 6,
    };
    let image = renderer.capture_placed(&frame, placed, 12, 10).unwrap();
    let expected = frame.to_image(&palette());
    let mut wrong = Vec::new();
    for y in 0..10 {
        for x in 0..12 {
            let inside = (2..10).contains(&x) && (3..9).contains(&y);
            let want = if inside {
                expected.pixel((x - 2) / 2, (y - 3) / 3)
            } else {
                Some([0, 0, 0, u8::MAX])
            };
            let got = image.pixel(x, y);
            if got != want {
                wrong.push(format!("({x}, {y}): {got:?}, expected {want:?}"));
            }
        }
    }
    let info = renderer.adapter_info();
    assert!(
        wrong.is_empty(),
        "{} on {:?}:\n{}",
        info.name,
        info.backend,
        wrong.join("\n")
    );
}

/// A frame placed at odd and even pixels, at whole scales from two to five, comes out whole. The
/// failure names the rows and columns that are wrong and the adapter that drew them.
#[test]
fn placed_frames_are_whole_at_every_origin_and_scale() {
    let Some(mut renderer) = renderer() else { return };
    renderer.set_palette(&palette());
    let frame = pattern(4, 2);
    let expected = frame.to_image(&palette());
    let mut wrong = Vec::new();
    for (across, down) in [(2, 2), (2, 3), (3, 2), (3, 3), (2, 4), (2, 5), (5, 2)] {
        for corner in 0..6 {
            for (x, y) in [(2, corner), (corner, 3)] {
                let placed = Placement {
                    x,
                    y,
                    width: 4 * across,
                    height: 2 * down,
                };
                let (width, height) = (x + placed.width + 1, y + placed.height + 1);
                let image = renderer.capture_placed(&frame, placed, width, height).unwrap();
                let mut rows = std::collections::BTreeSet::new();
                let mut columns = std::collections::BTreeSet::new();
                let mut first = None;
                for row in 0..height {
                    for column in 0..width {
                        let inside =
                            (x..x + placed.width).contains(&column) && (y..y + placed.height).contains(&row);
                        let want = if inside {
                            expected.pixel((column - x) / across, (row - y) / down)
                        } else {
                            Some([0, 0, 0, u8::MAX])
                        };
                        let got = image.pixel(column, row);
                        if got != want {
                            rows.insert(row);
                            columns.insert(column);
                            first.get_or_insert((got, want));
                        }
                    }
                }
                if let Some((got, want)) = first {
                    wrong.push(format!(
                        "{across}x{down} at ({x}, {y}): rows {rows:?}, columns {columns:?}, first {got:?} for {want:?}"
                    ));
                }
            }
        }
    }
    let info = renderer.adapter_info();
    assert!(
        wrong.is_empty(),
        "{} on {:?}:\n{}",
        info.name,
        info.backend,
        wrong.join("\n")
    );
}

/// Draws one frame through each filter inside a scissor rectangle at odd and even pixels. Every
/// pixel a draw writes is opaque, so one that is not was read from outside the image, as Direct3D
/// 12's software renderer did along a rectangle's odd edges.
#[test]
fn every_filter_draws_inside_a_scissor_at_any_pixel() {
    let Some(mut renderer) = renderer() else { return };
    renderer.set_palette(&palette());
    let frame = pattern(4, 2);
    let size = (frame.width(), frame.height());
    let mut wrong = Vec::new();
    for filter in [
        ScaleFilter::Nearest,
        ScaleFilter::SharpBilinear,
        ScaleFilter::Area,
    ] {
        for scissor in [None, Some((2, 2)), Some((3, 3)), Some((3, 2)), Some((2, 3))] {
            let patches: Vec<Scissor> = scissor
                .map(|(x, y)| Scissor {
                    x,
                    y,
                    width: 8,
                    height: 4,
                })
                .into_iter()
                .collect();
            let image = renderer
                .read_back((16, 12), |renderer, encoder, target, format, _| {
                    if renderer.targets.as_ref().is_none_or(|t| t.size != size) {
                        renderer.targets = Some(renderer.create_targets(size));
                    }
                    let Some(targets) = &renderer.targets else {
                        return Ok(());
                    };
                    renderer.scale.prepare(&renderer.device, format);
                    upload(&renderer.queue, targets, &frame);
                    renderer
                        .resolve
                        .draw(encoder, &targets.resolve_group, &targets.resolved);
                    renderer.scale.draw(
                        &renderer.queue,
                        encoder,
                        &targets.scale_group,
                        &Blit {
                            target,
                            format,
                            scale: (2.0, 2.0),
                            origin: (0.0, 0.0),
                            filter,
                            clear: true,
                            patches: &patches,
                        },
                    );
                    Ok(())
                })
                .unwrap();
            let mut rows = std::collections::BTreeSet::new();
            let mut count = 0;
            for y in 0..12 {
                for x in 0..16 {
                    if image.pixel(x, y).is_some_and(|pixel| pixel[3] != u8::MAX) {
                        rows.insert(y);
                        count += 1;
                    }
                }
            }
            if count > 0 {
                wrong.push(format!(
                    "{filter:?} scissor {scissor:?}: {count} pixels in rows {rows:?}"
                ));
            }
        }
    }
    let info = renderer.adapter_info();
    assert!(
        wrong.is_empty(),
        "{} on {:?}:\n{}",
        info.name,
        info.backend,
        wrong.join("\n")
    );
}
