# The user interface

How Robigo builds its windows. What the original does is in
[windows.md](../re/windows.md); this page covers the structure underneath, which is Robigo's own.

The hard rule "Match the output, not the code" applies here more than anywhere else in the
project. Every pixel and every click result belongs to `S2.EXE`. The machinery that produces them
belongs to Robigo.

## Why the original's structure stays behind

`S2.EXE` keeps a retained widget tree with a class and a vtable per control. A click travels from
the widget's `+0x58`, through the parent's vtable `+0x24`, into a sparse switch that matches a
command byte with `repnz scasb` against a table of 23 entries. The set of windows is a dense table
of `0x2f` addresses, and fourteen of its slots hold a creator that returns nothing. Extending that
table was never possible from outside the build, which is why the fourteen debug screens Blue Byte
wrote for themselves cannot be reached in any shipped copy.

Robigo needs the opposite property. Multiplayer, settings, key bindings and its own diagnostic
screens all arrive after the original's window set is complete, and the original's layouts will be
rearranged once the game runs. A structure that makes adding a window cheap is worth more than one
that mirrors the 1996 build.

## Shape

An immediate-mode core, with a retained store for the state that outlives a frame.

Almost every original window reads sim state and draws it. The stock, the statistics, the ship
register and the military settings hold nothing of their own between frames. Building those as
functions that read the world and emit draw calls removes any chance of a window disagreeing with
the sim, and turns rearranging a window into editing one function.

## What the retained store holds

Keyed by a stable widget id, because an immediate-mode pass has nowhere else to put it:

- which windows are open, their position and their order front to back
- the rolled-up state. Command id 5 collapses a window to height `0x20` and keeps the old height,
  and Robigo reproduces both the height and the restore
- scroll offsets, text carets and the node a drag started from
- the press state `S2.EXE` keeps at a widget's `+0x12`: idle, held inside, and held after the
  pointer left the control. Release fires from the second of those alone, and a player can cancel
  a click by sliding off a button before letting go. That behaviour is visible, and Robigo copies it

## Placing controls

Two modes share one widget set.

A window rebuilt from the original places its children at explicit coordinates, taken from
`S2.EXE`'s own tables. Pixel-for-pixel agreement with DOSBox is the test, and a layout engine would
fight it.

A window with no original places its children by row, column or grid. The network screens, the
settings and any diagnostic window take this route.

The widget set does not change between them. A button is the same button either way.

## Drawing

A painter emits indexed-colour operations into the same list as the terrain, which leaves the
palette resolve and the scaling to run once over everything:

- blit an item from `IO.DAT` at a position
- fill a rectangle with a palette index
- draw a run of text in one of the original's fonts
- push and pop a clip rectangle

Coordinates stay on the original's pixel grid and keep layout arithmetic in integers. `s2-sim`
forbids floats; the interface has no need of them either.

## Scaling

The interface scales by a whole number of its own, apart from the world view's zoom.

The original offers four screen modes up to 1280 × 1024 and draws its windows at the same pixel
size in all of them ([startup.md](../re/startup.md#the-screen-modes-confirmed)), so its interface
covers a quarter as much of the screen at its largest mode as at its smallest. Robigo keeps the
placement behaviour and adds the scaling the original has no answer for.

**The painter knows nothing about the scale.** A window is painted at one pixel per original
pixel, so a window that is 244 by 434 is 244 by 434 and every number on these pages is the number
in the code. One pass scales the finished layer, nearest neighbour only: the world view may take
the sharp filter it uses between whole zooms, and interface art and bitmap glyphs may not.

**The desktop is the screen divided by the scale.** At 2560 × 1440 and a scale of 3 the desktop is
853 by 480 of those pixels. Windows keep their own sizes and spread over the whole screen, which
is what the original already does across its four modes, so a larger screen gives more room for
windows rather than larger ones. A screen that does not divide evenly rounds the desktop up and
lets the layer overhang, clipped by the screen, rather than leaving a border.

**The pointer divides by the scale once**, at the edge, so the store and the windows only ever see
desktop pixels.

**The layer reaches the screen in the rectangles the windows cover.** Every window fills its own
rectangle, so the renderer takes the areas of the open windows, scales each one and draws the layer
inside them. The rest of the view reaches the screen untouched, and no index has to stand for
"nothing drawn here".

The scale is a setting, 2 by default and at most 8, and resizing the window leaves it alone: a
larger window gives a larger desktop. The window is never smaller than 320 by 400. A desktop that
shrinks, from a smaller window or a larger scale, moves each open window back inside it. The scale
is kept with the window's size, fullscreen and the view's zoom in `settings.json` in the user's
preferences folder (`src/settings.rs`). Until a settings window exists, Ctrl with + or - steps it
while the game runs, Ctrl+0 goes back to the opening scale, and the change is written back when the
window closes. `--ui-scale` overrides it for one run and is never written back. Whether 2 is the
right default is a question for play testing, not for reading the original.

Art drawn for a larger scale is a separate matter and composes with this: a theme declares the
scale its pictures are drawn for, and the scaling pass covers whatever is left over. Original art
works at every scale from the start, and better art slots in without a window changing.

A run is measured the way the original measures it ([text.md](../re/text.md)): each glyph advances
by its own width plus the pen's spacing, a character the font lacks draws nothing, and a fixed cell
width centres every glyph and advances by the cell. The measure the original uses for centring
counts codes `0x20` to `0xbf` and skips the rest, which Robigo keeps so a centred title lands on the
same pixel.

## The skin

A window never names a file. It asks for a picture by what the picture is, and a theme says which
item draws it.

A control draws in two parts. Its face comes from the style it carries: the background tile at
`+0x00` of the style record, the alternate tile at `+0x0a` for its other state, and the frame at
`+0x14` ([windows.md](../re/windows.md#window-styles-confirmed)). Its picture is a separate item
drawn over that, and the items are smaller than the rectangles they sit in and carry transparency:
the tick that accepts a prompt is 12 by 10 and a little under half of it is drawn.

So the chrome is shared and belongs to the style, and the picture is the part that says what the
control does. The 23 window modules name 64 distinct items between them, which is a small enough
vocabulary to give each one an identity.

- A `Picture` is one of those identities, a newtype over a number with a named constant, the same
  shape as the data tables in `s2-sim`.
- A theme maps each `Picture` to an item, and maps each control type and state to the chrome that
  draws it. `Theme::ORIGINAL` holds the 64 names out of `IO.DAT` and resolves them once at
  start-up through the loader in `s2-ui::items`.
- A window with no counterpart in the original names new `Picture`s. A theme that has no entry for
  one falls back, the way a missing state already falls back to the idle one, so the original theme
  is allowed to be incomplete: it describes a game from 1996.
- Replacement art is a new theme rather than an edit to any window, and because a `Picture` says
  what it is, whoever draws the replacement is told "the territory overlay button" instead of
  matching `io067` out of an archive.

A `Picture` resolves by array index, so nothing looks up a string while drawing.

## Commands

The boundary already decided for the sim holds. A control emits a Command carrying absolute values,
never a delta, and never writes to the world. Validation lives in helpers that the window and the
command handler both call, which keeps a greyed-out button and a rejected command agreeing on the
reason.

Windows read the world through read methods rather than reaching into its fields.

## Pausing

Pause is one flag in the retained store. Nothing is layered over the view to produce it, so
nothing can stack the way the original's overlay does
([windows.md](../re/windows.md#pausing-a-view-confirmed)). The view draws through the table the
original's overlay applies, the shadow table twice over, before the interface draws.

A paused game leaves the way back to the game menu and nothing else:

- the hub stays bright, and of its buttons only the one that opens the game menu acts
- the game menu and every window it opens stay bright and usable: saving and loading, the CD
  player and the MIDI player, the readme, the keyboard layout, the options and the prompt that
  asks before leaving
- every other window open when the pause starts stays open, drawn through the same table as the
  view, and ignores the pointer until the pause comes off
- no other window can open, from a key, the toolbar or the hub
- the keys that open a window the pause leaves usable work; every other key the interface binds
  does nothing, and the pause key takes the pause off
- clicks on the map do nothing, as the original's overlay swallows them
- every sound of the world stops at once and none starts: the ambient sounds, the rolls the view
  makes as it draws a mill, a ship, a fire or a boat, and the animations' own sounds. The
  interface's clicks, roll-ups and refusals still play, and music, once it plays, stops as well

`s2_ui::windows::live_while_paused` names the windows that stay usable, and every check reads it.

A button that cannot act still draws. The original's disabled controls draw in their own face,
swallow the click and are then greyed over (`windows.md`, the nonzero `+0x1d` and `0x4d8c0`).
Robigo greys the controls the original disables and the hub's buttons while paused; a window the
pause leaves dim takes the view's dimming instead, with its controls in their usual look.

Loading clears the pause flag through the same call that any other unpause goes through, so a
loaded game is never left half paused.

## The crate

`s2-ui` depends on `s2-render` for the painter, `s2-systems` for the shared types, and `s2-sim` for
its read-only views and the Command type. Nothing in `s2-sim` depends on `s2-ui`.

## Where the interface does not copy the original

The sim copies the original to the step. The interface copies its look, and departs from it where
the original withholds something the player needs. Each departure is listed here, so a comparison
against a screenshot of the original can tell a departure from a fault.

| Screen | The original | Robigo |
|---|---|---|
| The post office | Nothing marks the chosen category, so the five filters look the same whichever is on | The chosen filter draws in the face a control shows under the pointer |
| A panel of text | `0x5996d` shows one line fewer than the panel's height divides into | The panel shows every line that fits |
| The game menu | Ends with End game under the volume sliders | An Options button sits between the volume sliders and End game, which moves down 24 pixels and grows the window to match |
| A notice | No such window | A few lines of Robigo's own text over S2's confirm button, which closes it; the options open one when a pick needs the player to know more, such as a sound font that downloads at the next start |
| The credits | The crowd of 400 settlers steps once for each pass of the screen's idle loop, so it walks at the machine's pace, and its generator is seeded from the tick count | The pages, their text and timing as S2 shows them, and the portraits in black and white as the Gold Edition shows them: six of the base game's are drawn for another palette and show in blotches of sepia there, and the bake draws them in this palette's greys. The crowd steps at the ambient pace the options hold, the same passes a second the ambient sounds take, and its generator is seeded from the clock. The mode the original holds in which the crowd stands still, and never reaches, is left out. Two pages of Robigo's own come before the original's last, which never gives way: a heading and a page for WelshProgrammer with a portrait in the palette's colours nearest its greys. The portrait is a grey picture in `assets/credits`, which the game carries itself |
| The migration list | No such window | A panel of text with the readme's scroll buttons, titled "Save migrated". It opens after loading a save made under other mod rules, when the repair took something from the player's own buildings, and lists each building with what it lost |
| The options | No such window | The settings S2 lacks, a row each in S2's faces and letters: fullscreen, window size, interface scale, who draws the pointer, which way a drag moves the map, the ambient sounds' pace, the songs' rendering, the sound font, whether the intro plays, whether the loading pictures show and a button that opens the sound test. Each takes effect at once and goes to the settings file when the game ends, apart from the sound font, which renders the songs at the next start, and the intro, which the next start plays or skips |
| The sound test | No such window | Any effect by its id, with or without the random volume and rate the game gives it, and any song, each played on a press. An effect shows its name where its use is known and its `SOUND.LST` slot otherwise; an id with no sample greys its Play button |
| The main menu | Every button works; Options opens a screen of its own with the screen resolution and the mouse driver. The base game has eight buttons and the Gold Edition nine, with World Campaign second | The Gold Edition's nine buttons, with any data. Without the Gold Edition's mod, World Campaign reads in Robigo's own words and opens a notice saying the mod is needed. Credits opens the credits. Resume last game and Load game read Robigo's own saved games. Campaign opens the chapter screen. Unlimited play opens the settings screen. Options opens Robigo's options window over the menu. Intro plays the film and then shows the title |
| The music on the way back to the main menu | The song or the CD track that played in the game carries on over the menu | Song 1, the menu's own, takes over from any other song and from a CD track. Song 1 carries on where it is, and music switched off stays off |
| Losing a World Campaign continent | The game plays on with nobody to win it once the person's last building has gone | The game ends and the world screen shows, with a notice saying the player was defeated |
| Saved games | Eleven slots in the game's `SAVE` folder, named by `DIR.DAT` | Eleven slots in the preferences folder's `saves` folder, named by `directory.json`, with the same numbers and names; S2's own saves are not read |
| The loading picture | Shows for as long as the map takes to load | Shows for five seconds, or until a click or key, as a map now loads at once; the `loading_pictures` setting leaves it out |
| Unlimited play's settings | Three lines at the foot give the free memory, the memory the map and the graphics use, and what is left | The memory lines are left out, since they no longer mean anything. A map whose header counts more players than it holds start positions gets a row only for each position, so every player put in the game has one; every map in `DATA/MAPS3` holds a position for each player it counts, and shows the same rows as S2 |
| The intro film | The launcher plays `INTRO.SMK` through `SMACKPLY`, in a 640 by 400 mode, each pixel doubled, and only Escape ends it | The CD's `INTRO2.SMK`, the same film and sound compressed less, plays when the data has it, and `INTRO.SMK` otherwise. The film shows at 4:3, as a monitor of the time stretched it, as large as the window holds, sharp bilinear, with black around it. Any key or mouse button ends it, and the `skip_intro` setting leaves it out |
| The title and the main menu at other window sizes | 640 by 480, the screen mode they run in | 640 by 480 at the largest whole scale the window holds, centred with black around them; the interface scale applies to the game, not to these screens |
| Dragging the map | The view moves the way the pointer goes | The map follows the pointer, as most games now do; the `drag_moves_view` setting gives the original's way |
| Pausing | Every window stays open, bright and usable, and keys still open windows; only clicks on the map are swallowed | Only the hub's game menu button, the game menu and the windows it opens stay usable; every other open window dims with the view and ignores the pointer, and no other window opens |
| The prompt that asks before leaving | Every pixel of the window draws one lightening pass above the colour its item holds | The window draws in the colours its items hold |
| A tab in the action menu | The picture centres in the whole tab, so the strip the row below runs on cuts off the foot of an unchosen tab's picture | The picture centres in the band every tab shares, which keeps it whole |

## Testing

A window's read methods return its contents without a renderer, which lets a test assert what a
screen shows from a fixed world. Screens with an original are also compared against DOSBox
captures.

## Open

- Keyboard focus and tab order, which the original has no concept of.
- Whether the retained store belongs in the save, as `S2.EXE` keeps open windows in its own.
