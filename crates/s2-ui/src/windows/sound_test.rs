//! Robigo's sound test, which the options open: any effect of the table by its id, with or without
//! its jitter, and any song, so each can be checked by ear. The window keeps which effect and song
//! are picked; the caller says which effects have a sample and plays what a press asks for.

use s2_render::terrain::Point;

use crate::geometry::Rect;
use crate::paint::Painter;
use crate::skin::Skin;
use crate::store::Ui;
use crate::text::Label;
use crate::widget::{Pointer, WindowId};
use crate::windows::music::{Play, SONGS};
use crate::windows::rows::{self, Control, Row};
use crate::windows::{Outcome, kind};

/// The highest effect id the table has a row for.
pub const LAST_EFFECT: u8 = 70;

/// The slot of `SOUND.LST` effect 0 would read: effect `n` reads slot `n + 50`.
const FIRST_SLOT: u16 = 50;

/// What a press on the sound test asks to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestEffect {
    /// The effect's id.
    pub id: u8,
    /// Whether its volume and rate take the jitter the game gives them, or the table's own.
    pub jitter: bool,
}

/// What a press on a row does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pressed {
    Effect(i8),
    PlayEffect,
    Jitter,
    Song(i8),
    PlaySong,
    StopSong,
}

/// The sound test window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundTestWindow {
    /// Which window kind it opens as.
    pub id: WindowId,
    effect: u8,
    jitter: bool,
    song: u16,
}

impl Default for SoundTestWindow {
    fn default() -> Self {
        Self::new()
    }
}

/// The names of the effects whose use is known: the interface's and those the game asks for
/// outside the animations.
const NAMES: [(u8, &str); 8] = [
    (27, "Mill or ship"),
    (34, "Boat rowing"),
    (44, "Sheep"),
    (46, "Fire"),
    (58, "Cheat word"),
    (59, "Refused"),
    (61, "Click"),
    (63, "Window rolls"),
];

impl SoundTestWindow {
    /// The window at its own kind, on effect 1 and song 1 with the jitter on.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            id: kind::SOUND_TEST,
            effect: 1,
            jitter: true,
            song: 1,
        }
    }

    /// How big the window is.
    #[must_use]
    pub fn size() -> (u32, u32) {
        rows::size(Self::new().rows(Label::Text(""), &|_| true).len())
    }

    /// Where row `row`'s control sits, in a window whose corner is `at`.
    #[must_use]
    pub fn control(row: usize, at: Point) -> Rect {
        rows::control(row, at)
    }

    /// The effect picked.
    #[must_use]
    pub const fn effect(&self) -> u8 {
        self.effect
    }

    /// The song picked, from 1.
    #[must_use]
    pub const fn song(&self) -> u16 {
        self.song
    }

    /// What the picked effect is known as, or its `SOUND.LST` slot, as a run of `painter`'s.
    fn effect_name(self, painter: &mut Painter) -> Label {
        NAMES.iter().find(|&&(id, _)| id == self.effect).map_or_else(
            || {
                let slot = u16::from(self.effect) + FIRST_SLOT;
                painter.run(format!("SOUND.LST slot {slot}").as_bytes())
            },
            |&(_, name)| Label::Text(name),
        )
    }

    fn rows(self, name: Label, sampled: &dyn Fn(u8) -> bool) -> [Row<'static, Pressed>; 7] {
        let has_sample = sampled(self.effect);
        [
            Row {
                name: "Effect",
                help: "The effect's id in the table",
                control: Control::Stepper(Label::Number(i32::from(self.effect)), Pressed::Effect),
            },
            Row {
                name: "",
                help: "",
                control: Control::Text(name),
            },
            Row {
                name: "Play effect",
                help: "Plays the effect as the game asks for it",
                control: Control::Button(
                    Label::Text(if has_sample { "Play" } else { "No sample" }),
                    Pressed::PlayEffect,
                    has_sample,
                ),
            },
            Row {
                name: "Jitter",
                help: "Random volume and rate, as in the game",
                control: Control::Button(
                    Label::Text(if self.jitter { "On" } else { "Off" }),
                    Pressed::Jitter,
                    true,
                ),
            },
            Row {
                name: "Song",
                help: "The song's number in the MIDI player",
                control: Control::Stepper(Label::Number(i32::from(self.song)), Pressed::Song),
            },
            Row {
                name: "Play song",
                help: "Plays the song in place of the music",
                control: Control::Button(Label::Text("Play"), Pressed::PlaySong, true),
            },
            Row {
                name: "Stop music",
                help: "Stops the song",
                control: Control::Button(Label::Text("Stop"), Pressed::StopSong, true),
            },
        ]
    }

    /// Takes `mouse`, draws the test into `painter`, with `sampled` saying which effects have a
    /// sample to play, and gives back what a release asks to play.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        sampled: &dyn Fn(u8) -> bool,
        mouse: Pointer,
    ) -> Option<Outcome> {
        let window = *ui.window(self.id)?;
        let area = window.area;
        if window.rolled() {
            return None;
        }
        painter.push_clip(area);
        let at = Point::new(area.x, area.y);
        let name = self.effect_name(painter);
        let rows = self.rows(name, sampled);
        let pressed = rows::run(ui, painter, skin, (self.id, at), &rows, &mut None, mouse);
        painter.pop_clip();
        match pressed? {
            Pressed::Effect(step) => {
                self.effect = stepped(self.effect, step, LAST_EFFECT);
                None
            }
            Pressed::Jitter => {
                self.jitter = !self.jitter;
                None
            }
            Pressed::Song(step) => {
                let last = u16::try_from(SONGS).unwrap_or(u16::MAX);
                self.song = u16::from(stepped(
                    u8::try_from(self.song).unwrap_or(1),
                    step,
                    u8::try_from(last).unwrap_or(u8::MAX),
                ));
                None
            }
            Pressed::PlayEffect => Some(Outcome::TestEffect(TestEffect {
                id: self.effect,
                jitter: self.jitter,
            })),
            Pressed::PlaySong => Some(Outcome::Play(Play::Song(self.song))),
            Pressed::StopSong => Some(Outcome::Play(Play::StopMusic)),
        }
    }
}

/// `value` moved by `step` within 1 to `last`, wrapping round at either end.
fn stepped(value: u8, step: i8, last: u8) -> u8 {
    let moved = i16::from(value) + i16::from(step);
    if moved < 1 {
        last
    } else if moved > i16::from(last) {
        1
    } else {
        u8::try_from(moved).unwrap_or(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;

    fn release(
        window: &mut SoundTestWindow,
        row: usize,
        dx: i32,
        sampled: &dyn Fn(u8) -> bool,
    ) -> Option<Outcome> {
        let (width, height) = SoundTestWindow::size();
        let mut ui = Ui::new();
        ui.open(kind::SOUND_TEST, Rect::new(0, 0, width, height));
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        let skin = Skin::new();
        let control = SoundTestWindow::control(row, Point::new(0, 0));
        let on = Pointer::at(Point::new(control.x + dx, control.y + 2));
        window.run(&mut ui, &mut painter, &skin, sampled, on.pressing());
        painter.reset();
        window.run(&mut ui, &mut painter, &skin, sampled, on.releasing())
    }

    #[test]
    fn the_steppers_wrap_round_the_effects_and_songs() {
        let mut window = SoundTestWindow::new();
        let all = |_| true;
        assert_eq!(release(&mut window, 0, 2, &all), None);
        assert_eq!(window.effect(), LAST_EFFECT);
        let up = i32::try_from(rows::CONTROL_WIDTH - rows::ARROW).unwrap() + 2;
        release(&mut window, 0, up, &all);
        assert_eq!(window.effect(), 1);
        release(&mut window, 4, 2, &all);
        assert_eq!(window.song(), u16::try_from(SONGS).unwrap());
    }

    #[test]
    fn play_asks_for_the_effect_with_the_jitter_switch() {
        let mut window = SoundTestWindow::new();
        let all = |_| true;
        assert_eq!(
            release(&mut window, 2, 2, &all),
            Some(Outcome::TestEffect(TestEffect { id: 1, jitter: true }))
        );
        release(&mut window, 3, 2, &all);
        assert_eq!(
            release(&mut window, 2, 2, &all),
            Some(Outcome::TestEffect(TestEffect { id: 1, jitter: false }))
        );
        assert_eq!(
            release(&mut window, 5, 2, &all),
            Some(Outcome::Play(Play::Song(1)))
        );
        assert_eq!(
            release(&mut window, 6, 2, &all),
            Some(Outcome::Play(Play::StopMusic))
        );
    }

    #[test]
    fn an_effect_with_no_sample_cannot_be_played() {
        let mut window = SoundTestWindow::new();
        assert_eq!(release(&mut window, 2, 2, &|_| false), None);
    }
}
