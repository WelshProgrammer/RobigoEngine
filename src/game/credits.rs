//! The credits as the front end shows them: the picture in the greenland palette, a page of
//! text, its portrait, and the crowd walking along the foot.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use s2_assets::{BobSprites, Landscape, Nation, Palette, SpriteSheet};
use s2_formats::bob::{BodyType, Direction};
use s2_mods::{Backdrop, Sheets};
use s2_render::terrain::Point;
use s2_render::{Frame, Paint, Walking, blit, draw_walking, job_item};
use s2_sim::Job;
use s2_ui::geometry::Rect;
use s2_ui::screens::credits::{self, Crowd, Look, Portrait, Settler, Words};

use super::desktop::lines;
use super::front::framed;

/// The picture of Robigo's author the credits show: a grey photograph the size of a portrait's
/// area, which the screen draws in the nearest colours of its palette.
const AUTHOR: &[u8] = include_bytes!("../../assets/credits/welshprogrammer.png");

/// The most steps the crowd takes in one frame, which holds a slow frame to a short catch-up.
const MOST_STEPS: u32 = 64;

/// The credits while they show.
pub(crate) struct Credits {
    /// The picture, with the palette the screen shows in.
    pub(crate) picture: Backdrop,
    /// The picture as a frame.
    pub(crate) frame: Frame,
    /// Every page's text.
    texts: Vec<Vec<u8>>,
    page: u16,
    turned: Instant,
    portraits: SpriteSheet,
    /// The author's picture, a palette index a pixel.
    author: Option<Frame>,
    carrier: BobSprites,
    jobs: BobSprites,
    crowd: Crowd,
    /// How many steps a second the crowd takes, and when it last took any.
    pace: u16,
    stepped: Instant,
    /// The part of a step the last frame left over.
    owed: f64,
}

impl Credits {
    /// Reads the credits out of `sheets`, with the crowd taking `pace` steps a second. A page
    /// whose text is missing shows none.
    ///
    /// # Errors
    ///
    /// What could not be read, for the picture, the palette, the portraits or the settlers.
    pub(crate) fn open(sheets: &Sheets<'_>, pace: u16) -> Result<Self, String> {
        let failed = |error: s2_mods::Error| error.to_string();
        let mut picture = sheets.backdrop(credits::PICTURE).map_err(failed)?;
        picture.palette = sheets
            .landscape(Landscape::Greenland)
            .map_err(failed)?
            .palette()
            .clone();
        let texts = (0..credits::PAGES)
            .map(|number| match credits::page(number).map(|page| page.words) {
                Some(Words::File(file)) => {
                    let path = credits::text_file(file, false);
                    sheets.text(&path).map_or_else(
                        |error| {
                            log::warn!("{path}: {error}");
                            Vec::new()
                        },
                        lines,
                    )
                }
                Some(Words::Own(text)) => text.to_vec(),
                None => Vec::new(),
            })
            .collect();
        let author = in_greys(AUTHOR, &picture.palette);
        if author.is_none() {
            log::warn!("the author's picture does not decode");
        }
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.subsec_nanos());
        let now = Instant::now();
        Ok(Self {
            frame: framed(&picture),
            picture,
            texts,
            page: 0,
            turned: now,
            portraits: sheets.sheet(credits::PORTRAITS).map_err(failed)?,
            author,
            carrier: sheets.bob(s2_assets::CARRIER_BOB).map_err(failed)?,
            jobs: sheets.bob(s2_assets::JOBS_BOB).map_err(failed)?,
            crowd: Crowd::new(seed),
            pace,
            stepped: now,
            owed: 0.0,
        })
    }

    /// Moves the credits on to now: the page turns once its time is up, the last staying, and
    /// the crowd takes the steps its pace owes.
    pub(crate) fn pass(&mut self) {
        let now = Instant::now();
        let page_time = Duration::from_secs(u64::from(credits::PAGE_TICKS)) / credits::TICKS_A_SECOND;
        if self.page + 1 < credits::PAGES && now.duration_since(self.turned) > page_time {
            self.page += 1;
            self.turned = now;
        }
        let owed = self.owed + now.duration_since(self.stepped).as_secs_f64() * f64::from(self.pace);
        self.stepped = now;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a count of steps, held to the most a frame takes"
        )]
        let steps = (owed.floor().min(f64::from(MOST_STEPS))) as u32;
        self.owed = (owed - f64::from(steps)).min(1.0);
        for _ in 0..steps {
            self.crowd.step();
        }
    }

    /// The text of the page that shows.
    pub(crate) fn text(&self) -> &[u8] {
        self.texts.get(usize::from(self.page)).map_or(&[], Vec::as_slice)
    }

    /// Draws the crowd and the page's portrait into `layer`, whose screen has its corner at
    /// `corner`.
    pub(crate) fn draw(&self, layer: &mut Frame, corner: Point) {
        within(layer, credits::CROWD_AREA, corner, |frame| {
            let mut surface = frame.surface();
            for settler in self.crowd.shown() {
                let at = Point::new(settler.x(), i32::from(settler.y));
                if let Some(walking) = self.walking(settler) {
                    draw_walking(&mut surface, walking, at);
                }
            }
        });
        match credits::page(self.page).and_then(|page| page.portrait) {
            Some(Portrait::Item(item)) => {
                let Some(portrait) = self.portraits.get(usize::from(item)) else {
                    return;
                };
                within(layer, credits::PORTRAIT_AREA, corner, |frame| {
                    let middle = |side: u32| i32::try_from(side / 2).unwrap_or_default();
                    let at = Point::new(middle(frame.width()), middle(frame.height()));
                    blit(&mut frame.surface(), 0, portrait, at, Paint::PLAIN);
                });
            }
            Some(Portrait::Author) => {
                if let Some(author) = &self.author {
                    within(layer, credits::PORTRAIT_AREA, corner, |frame| {
                        frame.paste(author, 0, 0);
                    });
                }
            }
            None => {}
        }
    }

    /// What `settler` is drawn from.
    fn walking(&self, settler: &Settler) -> Option<Walking<'_>> {
        let (bob, item, body) = match settler.look {
            Look::Carrier { ware, fat } => {
                let body = if fat { BodyType::Fat } else { BodyType::Slim };
                (&self.carrier, usize::from(ware), body)
            }
            Look::Worker { job, nation } => {
                let nation = Nation::ALL.get(usize::from(nation)).copied()?;
                let (item, body) = job_item(nation, Job(job))?;
                (&self.jobs, item, body)
            }
        };
        Some(Walking {
            bob,
            item,
            body,
            direction: if settler.leftwards {
                Direction::West
            } else {
                Direction::East
            },
            frame: settler.frame,
            base: settler.colour_base(),
        })
    }
}

/// The grey PNG `bytes` as a frame in `palette`: each pixel takes the colour nearest its grey,
/// as the other portraits do. `None` for bytes that are no 8-bit grey PNG.
fn in_greys(bytes: &[u8], palette: &Palette) -> Option<Frame> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder.read_info().ok()?;
    let info = reader.info();
    if info.color_type != png::ColorType::Grayscale || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let (width, height) = (info.width, info.height);
    let mut greys = vec![0; reader.output_buffer_size()?];
    reader.next_frame(&mut greys).ok()?;
    let mut shades = [0; 256];
    for (grey, shade) in (0..=u8::MAX).zip(&mut shades) {
        *shade = palette.nearest([grey; 3]);
    }
    let pixels: Vec<u8> = greys.iter().map(|&grey| shades[usize::from(grey)]).collect();
    let mut frame = Frame::new(width, height, 0);
    frame.fill_from(&pixels).then_some(frame)
}

/// Runs `draw` on the part of `layer` that `area` of the screen at `corner` covers, so that
/// nothing it draws lands outside the area.
fn within(layer: &mut Frame, area: Rect, corner: Point, draw: impl FnOnce(&mut Frame)) {
    let (Ok(left), Ok(top)) = (u32::try_from(corner.x + area.x), u32::try_from(corner.y + area.y)) else {
        return;
    };
    if left + area.width > layer.width() || top + area.height > layer.height() {
        return;
    }
    let (across, width) = (layer.width() as usize, area.width as usize);
    let mut cut = Vec::with_capacity(width * area.height as usize);
    for row in top..top + area.height {
        let start = row as usize * across + left as usize;
        cut.extend_from_slice(&layer.pixels()[start..start + width]);
    }
    let mut part = Frame::new(area.width, area.height, 0);
    if part.fill_from(&cut) {
        draw(&mut part);
        layer.paste(&part, left, top);
    }
}
