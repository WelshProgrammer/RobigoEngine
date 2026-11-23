//! The screens before a game: the title picture, which a left click passes, and the main menu
//! over its own picture. Both are 640 by 480 and fill the screen; the game draws the picture and this
//! module the controls over it.

use s2_render::terrain::Point;

pub mod chapters;
pub mod credits;
pub mod maps;
pub mod settings;
pub mod world;

use crate::geometry::Rect;
use crate::paint::{Align, ItemId, Painter};
use crate::skin::Skin;
use crate::store::Ui;
use crate::text::{Label, Pen};
use crate::widget::{Pointer, WidgetId, WindowId};

/// How big each screen is.
pub const SIZE: (u32, u32) = (640, 480);

/// The title picture, the one the English game shows (`0x60381`, kind 0).
pub const TITLE_PICTURE: &str = "GFX/PICS/SETUP996.LBM";

/// The main menu's picture (`0x60381`, kind 1).
pub const MENU_PICTURE: &str = "GFX/PICS/SETUP010.LBM";

/// The logo over the main menu, the English one (`0x5e094`).
pub const LOGO: &str = "iobob_02";

/// What a button of the main menu asks for, in the order the menu lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Choice {
    /// The Roman campaign's chapters.
    Campaign,
    /// The Gold Edition's World Campaign.
    WorldCampaign,
    /// The game saved last, loaded straight away.
    Resume,
    /// The load screen.
    Load,
    /// A game on a map of the player's choosing.
    Unlimited,
    /// The settings.
    Options,
    /// The intro video.
    Intro,
    /// The people who made the game.
    Credits,
    /// Back to the system.
    Quit,
}

/// One button of the main menu.
#[derive(Debug, Clone, Copy)]
struct Button {
    text: u16,
    style: u16,
    choice: Choice,
}

/// Where the buttons sit: the first at (60, 140) and each 30 pixels below the last, 200 by 20.
/// These are the Gold Edition's nine (`0x1fd628`); the base game has eight, without the World Campaign
/// (`0x1fd118`, the ys `0x5ee9c` writes in).
const LEFT: i32 = 60;
const TOP: i32 = 140;
const STEP: i32 = 30;
const BUTTON: (u32, u32) = (200, 20);

/// The green face most buttons take and the red one Quit program takes.
const GREEN: u16 = 3;
const RED: u16 = 1;

/// The buttons, top to bottom, each with the slot its text comes from.
const BUTTONS: [Button; 9] = [
    Button {
        text: 341,
        style: GREEN,
        choice: Choice::Campaign,
    },
    Button {
        text: WORLD_CAMPAIGN,
        style: GREEN,
        choice: Choice::WorldCampaign,
    },
    Button {
        text: 342,
        style: GREEN,
        choice: Choice::Resume,
    },
    Button {
        text: 343,
        style: GREEN,
        choice: Choice::Load,
    },
    Button {
        text: 344,
        style: GREEN,
        choice: Choice::Unlimited,
    },
    Button {
        text: 389,
        style: GREEN,
        choice: Choice::Options,
    },
    Button {
        text: 345,
        style: GREEN,
        choice: Choice::Intro,
    },
    Button {
        text: 346,
        style: GREEN,
        choice: Choice::Credits,
    },
    Button {
        text: 347,
        style: RED,
        choice: Choice::Quit,
    },
];

/// The World Campaign button's text, which the Gold Edition's text table holds and the base game's
/// gives another line.
const WORLD_CAMPAIGN: u16 = 573;

/// The line naming the publisher, year, language and version, a label of style 0 across the
/// foot (`0x5ee3d`).
const COPYRIGHT: u16 = 355;
const COPYRIGHT_AREA: (i32, i32, u32, u32) = (0, 460, 270, 20);

/// The logo's rectangle, which the item centres in (`0x5eef3`).
const LOGO_AREA: (i32, i32, u32, u32) = (45, 16, 230, 100);

/// The Gold Edition's title line, which takes the logo's place: slot 340 in style 5, centred in
/// its rectangle (`0x5f68f`).
pub const GOLD_TITLE: u16 = 340;
const TITLE_AREA: (i32, i32, u32, u32) = (0, 16, 320, 100);
const TITLE_STYLE: u16 = 5;

/// The style the copyright line draws in.
const PLAIN: u16 = 0;

/// The main menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MainMenu {
    /// The id its buttons are kept under.
    pub id: WindowId,
    /// What the World Campaign button says: its slot, or Robigo's own words where the text table
    /// lacks them.
    pub world: Label,
    /// The line the Gold Edition writes in the logo's place, which then is not drawn.
    pub title: Option<Label>,
}

impl Default for MainMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl MainMenu {
    /// The menu, its buttons kept under [`crate::windows::kind::MAIN_MENU`].
    #[must_use]
    pub const fn new() -> Self {
        Self {
            id: crate::windows::kind::MAIN_MENU,
            world: Label::Slot(WORLD_CAMPAIGN),
            title: None,
        }
    }

    /// Where the button for `choice` sits on a screen whose corner is `at`.
    #[must_use]
    pub fn button(choice: Choice, at: Point) -> Rect {
        let place = BUTTONS
            .iter()
            .position(|button| button.choice == choice)
            .and_then(|place| i32::try_from(place).ok())
            .unwrap_or_default();
        Rect::new(at.x + LEFT, at.y + TOP + STEP * place, BUTTON.0, BUTTON.1)
    }

    /// Takes `mouse`, draws the menu's controls onto a screen whose corner is `at` with the
    /// logo `logo`, and gives back the choice a release on a button makes. A choice `enabled`
    /// refuses draws greyed and swallows the click, as a disabled control does (`0x4d8c0`).
    #[allow(clippy::too_many_arguments, reason = "a screen needs all of it")]
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        logo: Option<ItemId>,
        at: Point,
        enabled: &dyn Fn(Choice) -> bool,
        mouse: Pointer,
    ) -> Option<Choice> {
        if let Some(style) = painter.lettering(PLAIN) {
            let (x, y, width, height) = COPYRIGHT_AREA;
            painter.text_aligned(
                Point::new(at.x + x, at.y + y),
                style.font,
                style.colour,
                Label::Slot(COPYRIGHT),
                Pen::TIGHT,
                Align::CentredInBox(width, height),
            );
        }
        let mut chosen = None;
        for (place, button) in BUTTONS.iter().enumerate() {
            let rect = Self::button(button.choice, at);
            let widget = WidgetId::new(self.id, u16::try_from(place).unwrap_or_default());
            let open = enabled(button.choice);
            let acted = ui.press(widget, rect, mouse, open);
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.face(button.style, look) {
                painter.frame_edges(item, skin.edge(button.style), rect, look.edges());
            }
            if let Some(style) = painter.lettering(button.style) {
                painter.text_aligned(
                    Point::new(rect.x, rect.y),
                    style.font,
                    style.colour,
                    if button.choice == Choice::WorldCampaign {
                        self.world
                    } else {
                        Label::Slot(button.text)
                    },
                    Pen::TIGHT,
                    Align::CentredInBox(rect.width, rect.height),
                );
            }
            if !open {
                painter.disable(rect);
            }
            if acted && chosen.is_none() {
                chosen = Some(button.choice);
            }
        }
        match (self.title, logo) {
            (Some(title), _) => {
                if let Some(style) = painter.lettering(TITLE_STYLE) {
                    let (x, y, width, height) = TITLE_AREA;
                    painter.text_aligned(
                        Point::new(at.x + x, at.y + y),
                        style.font,
                        style.colour,
                        title,
                        Pen::TIGHT,
                        Align::CentredInBox(width, height),
                    );
                }
            }
            (None, Some(item)) => {
                let (x, y, width, height) = LOGO_AREA;
                painter.item_centred(item, Rect::new(at.x + x, at.y + y, width, height));
            }
            (None, None) => {}
        }
        chosen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(
        menu: &mut MainMenu,
        ui: &mut Ui,
        mouse: Pointer,
        enabled: &dyn Fn(Choice) -> bool,
    ) -> Option<Choice> {
        let mut painter = Painter::new(Rect::new(0, 0, SIZE.0, SIZE.1));
        menu.run(
            ui,
            &mut painter,
            &Skin::new(),
            None,
            Point::new(0, 0),
            enabled,
            mouse,
        )
    }

    #[test]
    fn the_buttons_stack_thirty_pixels_apart() {
        let at = Point::new(0, 0);
        assert_eq!(
            MainMenu::button(Choice::Campaign, at),
            Rect::new(60, 140, 200, 20)
        );
        assert_eq!(
            MainMenu::button(Choice::WorldCampaign, at),
            Rect::new(60, 170, 200, 20)
        );
        assert_eq!(MainMenu::button(Choice::Options, at), Rect::new(60, 290, 200, 20));
        assert_eq!(MainMenu::button(Choice::Quit, at), Rect::new(60, 380, 200, 20));
    }

    #[test]
    fn a_release_chooses_and_a_disabled_button_swallows_it() {
        let (mut menu, mut ui) = (MainMenu::new(), Ui::new());
        let on = |choice| {
            let rect = MainMenu::button(choice, Point::new(0, 0));
            Pointer::at(Point::new(rect.x + 4, rect.y + 4))
        };
        let all = |_| true;
        run(&mut menu, &mut ui, on(Choice::Options).pressing(), &all);
        assert_eq!(
            run(&mut menu, &mut ui, on(Choice::Options).releasing(), &all),
            Some(Choice::Options)
        );
        let not_campaign = |choice| choice != Choice::Campaign;
        run(&mut menu, &mut ui, on(Choice::Campaign).pressing(), &not_campaign);
        assert_eq!(
            run(
                &mut menu,
                &mut ui,
                on(Choice::Campaign).releasing(),
                &not_campaign
            ),
            None
        );
    }
}
