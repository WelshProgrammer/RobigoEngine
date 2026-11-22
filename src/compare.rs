//! `robigo compare`: a starting view checked against a screenshot of the original game.
//!
//! The view is drawn as `robigo view` would draw it at the capture's size, once per animation
//! phase, and compared palette index by palette index with the capture's map view. Pixels the
//! game's interface covers are left out: those every `--interface` capture shares with this one.
//! So are pixels Robigo drew an animal on, since animals walk off before a screenshot can be taken.

use std::path::{Path, PathBuf};

use s2_render::capture::{self, Capture, CaptureError, Comparison, PHASES};
use s2_render::terrain::Point;
use s2_render::{Clocks, Frame, Layers, Provenance};

use crate::view;

/// Arguments of `robigo compare`.
#[derive(Debug, clap::Args)]
pub struct Args {
    #[command(flatten)]
    view: view::Args,
    /// A raw `DOSBox` screenshot (8-bit indexed PNG) of the game's starting view of the map.
    #[arg(long, value_name = "PNG")]
    capture: PathBuf,
    /// Raw screenshots of other maps at the same resolution. Pixels that every one of them shares
    /// with --capture are the game's interface and are left out.
    #[arg(long = "interface", value_name = "PNG")]
    interface: Vec<PathBuf>,
    /// Write the capture's map view here, dimmed, with differing pixels in red and the pixels
    /// left out in blue.
    #[arg(long, value_name = "PNG")]
    diff: Option<PathBuf>,
}

/// Why a comparison failed or found differences.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The map or its graphics did not load.
    #[error(transparent)]
    View(#[from] crate::error::Error),
    /// A file could not be read or written.
    #[error("{0}: {1}")]
    Io(PathBuf, std::io::Error),
    /// A screenshot did not decode.
    #[error("{0}: {1}")]
    Capture(PathBuf, CaptureError),
    /// A screenshot has no map view, or not the capture's size.
    #[error("{0}: {1}")]
    Size(PathBuf, String),
    /// The picture differs from the capture.
    #[error("{0} pixels differ from the capture at the closest animation phase")]
    Differs(usize),
}

fn read(path: &Path) -> Result<Capture, Error> {
    let bytes = std::fs::read(path).map_err(|e| Error::Io(path.to_owned(), e))?;
    Capture::from_png(&bytes).map_err(|e| Error::Capture(path.to_owned(), e))
}

/// Runs `robigo compare`, printing what differs.
///
/// # Errors
///
/// [`Error::Differs`] when any compared pixel differs; otherwise loading or decoding failures.
pub fn run(args: &Args) -> Result<(), Error> {
    let capture = read(&args.capture)?;
    let rect = capture
        .view_rect()
        .ok_or_else(|| Error::Size(args.capture.clone(), "too small for a map view".into()))?;
    let theirs = capture.view();
    let others = interface_views(&args.interface, &capture)?;
    let mut views: Vec<&[u8]> = vec![&theirs];
    views.extend(others.iter().map(Vec::as_slice));
    let interface = capture::interface_mask(&views).unwrap_or_else(|| {
        log::warn!("no --interface captures; the game's interface will count as differences");
        vec![false; theirs.len()]
    });

    let scene = crate::scene::load(&args.view.start())?;
    let size = (rect.width, rect.height);
    let screen = scene.start_scroll(args.view.at, (capture.width(), capture.height()));
    let scroll = Point::new(screen.x + rect.x.cast_signed(), screen.y + rect.y.cast_signed());
    let mut best: Option<(u32, Comparison, Vec<bool>)> = None;
    let mut frames = Vec::new();
    for phase in 0..PHASES {
        let mut frame = Frame::new(size.0, size.1, 0);
        let mut provenance = Provenance::new(size.0, size.1);
        let world = scene.game.world();
        let step = scene.game.clock().step();
        let clocks = |anim| Clocks { anim, step };
        scene
            .view
            .draw(world, scroll, clocks(phase), &mut frame, Some(&mut provenance));
        let skip: Vec<bool> = interface
            .iter()
            .zip(provenance.mask(Layers::FIGURES))
            .map(|(&ui, figure)| ui || figure)
            .collect();
        let Some(result) = capture::compare(frame.pixels(), &theirs, size.0, &skip) else {
            continue;
        };
        if best
            .as_ref()
            .is_none_or(|(_, b, _)| result.differing < b.differing)
        {
            best = Some((phase, result, skip));
        }
        frames.push(frame);
    }
    let Some((phase, result, skip)) = best else {
        return Ok(());
    };
    let ui = interface.iter().filter(|&&s| s).count();
    let left_out = skip.iter().filter(|&&s| s).count();
    println!(
        "{} ({}) against {}: {}x{} screen, {}x{} view",
        scene.title,
        scene.origin.name(),
        args.capture.display(),
        capture.width(),
        capture.height(),
        size.0,
        size.1
    );
    println!(
        "left out: {ui} interface pixels ({} other captures), {} under animals",
        others.len(),
        left_out - ui
    );
    println!(
        "closest at animation phase {phase}: {} of {} compared pixels differ",
        result.differing, result.compared
    );
    for region in result.regions.iter().take(12) {
        let r = region.rect;
        println!(
            "  {} pixels within {}x{} at ({}, {})",
            region.pixels, r.width, r.height, r.x, r.y
        );
    }
    if let Some(note) = torn(&frames, phase, &theirs, &skip) {
        println!("{note}");
    }
    if let Some(path) = &args.diff {
        let frame = &frames[phase as usize];
        let rgba = diff_image(&theirs, frame.pixels(), &skip, &scene.palette.0);
        write_rgba(path, size, &rgba).map_err(|e| Error::Io(path.clone(), e))?;
    }
    if result.differing > 0 {
        return Err(Error::Differs(result.differing));
    }
    Ok(())
}

/// The map views of the `paths` captures, which must all be the size of `capture`.
fn interface_views(paths: &[PathBuf], capture: &Capture) -> Result<Vec<Vec<u8>>, Error> {
    let mut views = Vec::new();
    for path in paths {
        let other = read(path)?;
        if (other.width(), other.height()) != (capture.width(), capture.height()) {
            return Err(Error::Size(
                path.clone(),
                format!(
                    "{}x{}, the capture is {}x{}",
                    other.width(),
                    other.height(),
                    capture.width(),
                    capture.height()
                ),
            ));
        }
        views.push(other.view());
    }
    Ok(views)
}

/// A note when every pixel that differs at `best` matches one other phase. Such a capture holds
/// two animation frames: the game was copying a new one to the screen as it was taken.
fn torn(frames: &[Frame], best: u32, theirs: &[u8], skip: &[bool]) -> Option<String> {
    let ours = frames.get(best as usize)?.pixels();
    let differing: Vec<usize> = (0..theirs.len())
        .filter(|&i| !skip[i] && ours[i] != theirs[i])
        .collect();
    if differing.is_empty() {
        return None;
    }
    frames.iter().enumerate().find_map(|(phase, frame)| {
        let pixels = frame.pixels();
        (phase != best as usize && differing.iter().all(|&i| pixels[i] == theirs[i])).then(|| {
            format!(
                "every differing pixel matches phase {phase}: the capture holds two animation \
                 frames (the game was mid-copy when it was taken); take it again"
            )
        })
    })
}

fn diff_image(theirs: &[u8], ours: &[u8], skip: &[bool], palette: &[[u8; 3]; 256]) -> Vec<u8> {
    theirs
        .iter()
        .zip(ours)
        .zip(skip)
        .flat_map(|((&t, &o), &s)| {
            let [r, g, b] = palette[usize::from(t)];
            let grey = u8::try_from((u16::from(r) + u16::from(g) + u16::from(b)) / 6).unwrap_or(0);
            if s {
                [grey / 2, grey / 2, grey / 2 + 96, 255]
            } else if t == o {
                [grey, grey, grey, 255]
            } else {
                [255, 0, 0, 255]
            }
        })
        .collect()
}

fn write_rgba(path: &Path, (width, height): (u32, u32), rgba: &[u8]) -> std::io::Result<()> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(std::io::Error::other)?;
    writer.write_image_data(rgba).map_err(std::io::Error::other)?;
    writer.finish().map_err(std::io::Error::other)
}
