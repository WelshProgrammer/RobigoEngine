//! The rows Robigo's own windows are laid out in: a name on the left and a control on the right, a
//! button showing a value, a value between a down and an up arrow, or a value that opens a list
//! to pick from, drawn with S2's own faces, arrows and letters. A press hands back what the row
//! asks for.

use s2_render::terrain::Point;

use crate::geometry::Rect;
use crate::paint::{Align, Edges, Painter};
use crate::picture::Picture;
use crate::skin::{Look, Skin};
use crate::store::Ui;
use crate::style::Lettering;
use crate::text::{Label, Pen};
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::helpline;

/// How wide a window of rows is.
pub const WIDTH: u32 = 332;

/// Where the first row sits, how far apart rows are and how tall a control is.
const FIRST_ROW: i32 = 30;
const ROW: i32 = 26;
const CONTROL: u32 = 20;

/// Where a row's name sits and how far down its line starts.
const NAME_X: i32 = 12;
const NAME_DOWN: i32 = 6;

/// Where a row's control starts and how wide it is: a button, or arrows either side of a value.
const CONTROL_X: i32 = 140;
/// See [`CONTROL_X`].
pub const CONTROL_WIDTH: u32 = 180;
/// How wide each arrow of a stepper is.
pub const ARROW: u32 = 20;

/// How tall one choice of an open list is, and the edge round the list.
const CHOICE: u32 = 16;
const LIST_EDGE: u32 = 2;

/// How tall the strip under the last row is, which shows the help of the row under the pointer
/// in up to two lines, and how far it stands in from the window's sides.
const HELP_LINE: u32 = 14;
const HELP: u32 = 2 * HELP_LINE;
const HELP_INSET: u32 = 12;

/// How many characters of help one line of the strip takes before the rest goes to the second.
const HELP_CHARACTERS: usize = 40;

/// The room under the last row: the help strip and the window's edge.
const FOOT: u32 = HELP + 16;

/// The style the controls and letters draw in, as the game menu's do.
const PLAIN: u16 = 0;

/// The style an open list's faces draw in, the main menu's green, which sets the list apart
/// from the controls round it.
const LISTED: u16 = 3;

/// One row: its name, its line of help and what it holds. The help shows in the strip at the
/// window's foot while the pointer is on the row, which holds two lines of it.
#[derive(Debug)]
pub struct Row<'a, A> {
    /// What the row is called.
    pub name: &'static str,
    /// The help line under the pointer.
    pub help: &'static str,
    /// What sits on the right.
    pub control: Control<'a, A>,
}

/// A row's control, which asks for an `A`.
#[derive(Debug)]
pub enum Control<'a, A> {
    /// The value beside a down arrow. A press opens the list of `choices` under it, with the
    /// one at `chosen` marked, and a press on a choice asks for the `A` the function makes of
    /// its place. A press anywhere else closes the list.
    Choice {
        /// What the closed control shows.
        shown: Label,
        /// What the list offers, top to bottom.
        choices: &'a [Label],
        /// Which of them is in force, when one is.
        chosen: Option<usize>,
        /// What picking the choice at a place asks for.
        pick: fn(usize) -> A,
    },
    /// A button showing the value, which a press asks for the `A` of. Without the flag it draws
    /// greyed and swallows the press.
    Button(Label, A, bool),
    /// The value between a down and an up arrow, which ask for the `A` the function makes of
    /// -1 and 1.
    Stepper(Label, fn(i8) -> A),
    /// The value alone, with nothing to press.
    Text(Label),
}

/// The size of a window of `rows` rows.
#[must_use]
pub fn size(rows: usize) -> (u32, u32) {
    let rows = u32::try_from(rows).unwrap_or_default();
    let height =
        u32::try_from(FIRST_ROW).unwrap_or_default() + rows * u32::try_from(ROW).unwrap_or_default() + FOOT;
    (WIDTH, height)
}

/// Where row `row`'s control sits, in a window whose corner is `at`.
#[must_use]
pub fn control(row: usize, at: Point) -> Rect {
    let y = FIRST_ROW + ROW * i32::try_from(row).unwrap_or_default();
    Rect::new(at.x + CONTROL_X, at.y + y, CONTROL_WIDTH, CONTROL)
}

/// Where the help strip sits in a window of `rows` rows whose corner is `at`.
#[must_use]
pub fn help_strip(rows: usize, at: Point) -> Rect {
    let y = FIRST_ROW + ROW * i32::try_from(rows).unwrap_or_default();
    Rect::new(
        at.x + i32::try_from(HELP_INSET).unwrap_or_default(),
        at.y + y,
        WIDTH - 2 * HELP_INSET,
        HELP,
    )
}

/// `help` as the strip's lines: whole when it fits one, and otherwise broken at the last space
/// the first line has room for.
fn help_lines(help: &str) -> (&str, Option<&str>) {
    if help.len() <= HELP_CHARACTERS {
        return (help, None);
    }
    let room = help
        .char_indices()
        .map(|(at, _)| at)
        .take_while(|&at| at <= HELP_CHARACTERS)
        .last()
        .unwrap_or_default();
    match help[..room].rfind(' ') {
        Some(space) => (&help[..space], Some(&help[space + 1..])),
        None => (help, None),
    }
}

/// Where the list of `choices` choices opens for the control `control` in a window of `rows`
/// rows whose corner is `at`: under the control, or over it when the window ends too soon.
fn list_area(control: Rect, choices: usize, rows: usize, at: Point) -> Rect {
    let height = u32::try_from(choices).unwrap_or_default() * CHOICE + 2 * LIST_EDGE;
    let tall = i32::try_from(height).unwrap_or(i32::MAX);
    let under = control.y + i32::try_from(control.height).unwrap_or_default();
    let floor = help_strip(rows, at).y;
    let y = if under + tall <= floor {
        under
    } else {
        control.y - tall
    };
    Rect::new(control.x, y, control.width, height)
}

/// Where choice `number` of `choices` sits once row `row`'s list is open, in a window of `rows`
/// rows whose corner is `at`.
#[must_use]
pub fn choice(row: usize, (number, choices): (usize, usize), rows: usize, at: Point) -> Rect {
    let list = list_area(control(row, at), choices, rows, at);
    let edge = i32::try_from(LIST_EDGE).unwrap_or_default();
    let down = i32::try_from(CHOICE).unwrap_or_default() * i32::try_from(number).unwrap_or_default();
    Rect::new(
        list.x + edge,
        list.y + edge + down,
        list.width - 2 * LIST_EDGE,
        CHOICE,
    )
}

/// Takes `mouse`, draws `rows` into the window `id` whose corner is `at`, keeping its controls
/// under `id`, and gives back what a release asks for. `open` is the row whose list of choices
/// is open, which the caller keeps from one frame to the next.
pub fn run<A: Copy>(
    ui: &mut Ui,
    painter: &mut Painter,
    skin: &Skin,
    (id, at): (WindowId, Point),
    rows: &[Row<A>],
    open: &mut Option<usize>,
    mouse: Pointer,
) -> Option<A> {
    let style = painter.lettering(PLAIN);
    let mut draw = Draw {
        ui,
        painter,
        skin,
        style,
        window: id,
        widget: 0,
    };
    let mut asked = None;
    let was_open = *open;
    let mut help_shown = was_open.and_then(|row| rows.get(row)).map(|row| row.help);
    for (place, row) in rows.iter().enumerate() {
        let control = control(place, at);
        draw.name(Point::new(at.x + NAME_X, control.y + NAME_DOWN), row.name);
        let help = Label::Text(row.help);
        let whole = Rect::new(
            at.x + NAME_X,
            control.y,
            u32::try_from(control.x - at.x - NAME_X).unwrap_or_default() + CONTROL_WIDTH,
            CONTROL,
        );
        if was_open.is_none() && whole.contains(mouse.at.x, mouse.at.y) {
            help_shown = Some(row.help);
        }
        // While a list is open only the control it hangs from takes the pointer.
        let mouse = match was_open {
            Some(row) if row != place => Pointer::away(),
            _ => mouse,
        };
        let last = Rect::new(
            control.x + i32::try_from(CONTROL_WIDTH - ARROW).unwrap_or_default(),
            control.y,
            ARROW,
            CONTROL,
        );
        let pressed = match row.control {
            Control::Choice { shown, .. } => {
                if draw.button(control, (None, None), help, (true, mouse)) {
                    *open = (was_open != Some(place)).then_some(place);
                }
                let value = Rect::new(control.x, control.y, CONTROL_WIDTH - ARROW, CONTROL);
                draw.centred(value, shown);
                if let Some(item) = skin.picture(Picture::LINE_DOWN) {
                    draw.painter.item_centred(item, last);
                }
                None
            }
            Control::Button(value, ask, enabled) => draw
                .button(control, (None, Some(value)), help, (enabled, mouse))
                .then_some(ask),
            Control::Stepper(value, step) => {
                let first = Rect::new(control.x, control.y, ARROW, CONTROL);
                let lower = draw.button(first, (Some(Picture::STEP_DOWN), None), help, (true, mouse));
                let higher = draw.button(last, (Some(Picture::STEP_UP), None), help, (true, mouse));
                let middle = Rect::new(
                    first.x + i32::try_from(ARROW).unwrap_or_default(),
                    control.y,
                    CONTROL_WIDTH - 2 * ARROW,
                    CONTROL,
                );
                draw.centred(middle, value);
                if lower {
                    Some(step(-1))
                } else if higher {
                    Some(step(1))
                } else {
                    None
                }
            }
            Control::Text(value) => {
                draw.centred(control, value);
                None
            }
        };
        if asked.is_none() {
            asked = pressed;
        }
    }
    if let Some(help) = help_shown.filter(|help| !help.is_empty()) {
        let strip = help_strip(rows.len(), at);
        match help_lines(help) {
            (line, None) => helpline::strip(draw.painter, strip, line.as_bytes()),
            (first, Some(second)) => {
                let upper = Rect::new(strip.x, strip.y, strip.width, HELP_LINE);
                let lower = Rect::new(
                    strip.x,
                    strip.y + i32::try_from(HELP_LINE).unwrap_or_default(),
                    strip.width,
                    HELP_LINE,
                );
                helpline::strip(draw.painter, upper, first.as_bytes());
                helpline::strip(draw.painter, lower, second.as_bytes());
            }
        }
    }
    let picked = draw.list(at, rows, open, mouse);
    asked.or(picked)
}

/// What the rows of one window draw with: the window's controls are numbered as they draw.
struct Draw<'a> {
    ui: &'a mut Ui,
    painter: &'a mut Painter,
    skin: &'a Skin,
    style: Option<Lettering>,
    window: WindowId,
    widget: u16,
}

impl Draw<'_> {
    /// The next control of the window.
    fn next(&mut self) -> WidgetId {
        let id = WidgetId::new(self.window, self.widget);
        self.widget += 1;
        id
    }

    /// Draws a row's name from `at`.
    fn name(&mut self, at: Point, name: &'static str) {
        if let Some(style) = self.style {
            self.painter.text_aligned(
                at,
                style.font,
                style.colour,
                Label::Text(name),
                Pen::TIGHT,
                Align::Left,
            );
        }
    }

    /// Draws `value` in the middle of `rect`, cut off at its sides.
    fn centred(&mut self, rect: Rect, value: Label) {
        if let Some(style) = self.style {
            self.painter.push_clip(rect);
            self.painter.text_aligned(
                Point::new(rect.x, rect.y),
                style.font,
                style.colour,
                value,
                Pen::TIGHT,
                Align::CentredInBox(rect.width, rect.height),
            );
            self.painter.pop_clip();
        }
    }

    /// Takes `mouse` for a button covering `rect`, draws it with its picture or text, greyed
    /// unless enabled, and says whether it acts.
    fn button(
        &mut self,
        rect: Rect,
        (picture, text): (Option<Picture>, Option<Label>),
        help: Label,
        (enabled, mouse): (bool, Pointer),
    ) -> bool {
        let id = self.next();
        self.ui.note_help(rect, mouse, help);
        let acted = self.ui.press(id, rect, mouse, enabled);
        let look = self.ui.button(id).look(true, false);
        if let Some(item) = self.skin.face(PLAIN, look) {
            self.painter
                .frame_edges(item, self.skin.edge(PLAIN), rect, look.edges());
        }
        if let Some(item) = picture.and_then(|picture| self.skin.picture(picture)) {
            self.painter.item_centred(item, rect);
        }
        if let Some(text) = text {
            self.centred(rect, text);
        }
        if !enabled {
            self.painter.disable(rect);
        }
        acted
    }

    /// Draws the list of the row `open` names over the rows of the window whose corner is `at`
    /// and takes `mouse` for it. A press on a choice closes the list and gives back what the
    /// choice asks for, and a press off the list and its control only closes it.
    fn list<A: Copy>(
        &mut self,
        at: Point,
        rows: &[Row<A>],
        open: &mut Option<usize>,
        mouse: Pointer,
    ) -> Option<A> {
        let listed = open.and_then(|place| match rows.get(place)?.control {
            Control::Choice {
                choices,
                chosen,
                pick,
                ..
            } => Some((place, choices, chosen, pick)),
            _ => None,
        });
        let Some((place, choices, chosen, pick)) = listed else {
            *open = None;
            return None;
        };
        let head = control(place, at);
        let list = list_area(head, choices.len(), rows.len(), at);
        if let Some(face) = self.skin.face(LISTED, Look::Idle) {
            self.painter.frame(face, self.skin.edge(LISTED), list);
        }
        self.ui.note_help(list, mouse, Label::Text(rows[place].help));
        let mut asked = None;
        for (number, &label) in choices.iter().enumerate() {
            let rect = choice(place, (number, choices.len()), rows.len(), at);
            let id = self.next();
            let acted = self.ui.press(id, rect, mouse, true);
            let look = self.ui.button(id).look(true, chosen == Some(number));
            // A choice under the pointer takes the list's own face for that look, as a button of
            // the main menu does, and the one in force a control's face.
            let face = match look {
                Look::Hover | Look::Pressed => self.skin.face(LISTED, look),
                Look::On => self.skin.face(PLAIN, Look::Idle),
                Look::Idle | Look::Disabled => None,
            };
            if let Some(face) = face {
                self.painter.frame_edges(face, None, rect, Edges::Lit);
            }
            self.centred(rect, label);
            if acted {
                asked = Some(pick(number));
                *open = None;
            }
        }
        let outside = !list.contains(mouse.at.x, mouse.at.y) && !head.contains(mouse.at.x, mouse.at.y);
        if mouse.pressed && outside {
            *open = None;
        }
        asked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_past_one_line_breaks_between_words() {
        assert_eq!(
            help_lines("Fill the screen, or windowed"),
            ("Fill the screen, or windowed", None)
        );
        assert_eq!(
            help_lines("Hardware is best, use software if you have problems"),
            ("Hardware is best, use software if you", Some("have problems"))
        );
    }
}
