# Robigo

A modern engine for The Settlers II, written in Rust.

Robigo plays The Settlers II (Blue Byte, 1996) from your own copy of the game. It draws the same pixels,
plays the same sounds at the same times, and keeps its simulation in step with the original's.

The version Robigo replicates is v1.51, the base game after its official patch. Every behaviour it
reproduces comes from that version, and the code and docs call it S2.

Robigo is written from scratch. The original's behaviour was worked out by playing the game under DOSBox,
reading its data files, and disassembling and decompiling S2.EXE to study it. What that showed was written
up as notes on behaviour, and the code was written from those notes. No code from S2.EXE, disassembled or
decompiled, is in this repository in any form.

Fun fact: Robigo was the Roman goddess of wheat rust (ironically, the prevention of).

## What you need

- 64-bit Linux or Windows. macOS support is written but nobody has built it yet.
- A graphics driver with Vulkan (Metal on macOS).
- A copy of Settlers 2, any one of:
  - the CD, or an image of it as an ISO or as BIN and CUE files
  - the Gold Edition's CD or an image of it
  - a GOG installation of the Gold Edition
- For a copy older than v1.51, such as the first CD's v1.03, the official v1.51 patch as well. The Gold
  Edition and the GOG installation are v1.51 already.

On the first start Robigo asks where your copy is, keeps a copy of its own and spends a minute or two
extracting what it needs from it. It never changes your files.

## What is implemented so far

- The Roman campaign, the Gold Edition's World Campaign, and Unlimited Play on the game's maps.
- The computer players, rewritten from S2.EXE to make the same decisions.
- Saving and loading.
- The FM music through an emulated OPL3, or the General MIDI music through a SoundFont, and the CD's
  music tracks from a CD image or a GOG installation.
- Mods that replace the game's files by path or change its data tables
  ([docs/design/mods.md](docs/design/mods.md)).

Robigo fixes five outright bugs in S2.EXE by default. "Disable bug fixes" in the options window puts
them back for the next new game, and [docs/design/bug-fixes.md](docs/design/bug-fixes.md) lists them.
Multiplayer comes after 1.0.

## Getting it

Downloads for Linux (tar.gz, AppImage and deb) and Windows (zip and setup) are on the
[releases page](https://github.com/WelshProgrammer/RobigoEngine/releases). The READMEs for each OS
say how to install and where your saves and settings will go.

To build it yourself from source, [docs/building.md](docs/building.md) covers Linux, Windows and macOS.

## Contributing

Issues and pull requests are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) has the rules, the layout of the
code and the checks to run first. The main rule: no file from the original game goes in the repository,
in any form.

## Licence

Robigo is free software under the GNU General Public License, version 3 or later. See [LICENSE](LICENSE).

Robigo is not affiliated with or endorsed by Ubisoft or Blue Byte. The Settlers is a trademark of Ubisoft
Entertainment. You need your own copy of the game to play.

## Thanks

- [Return to the Roots](https://www.rttr.info), for inspiration and documentation.
- S. Christian Collins for GeneralUser GS, the default SoundFont.
- MuseScore General, the alternative SoundFont, after Frank Wen's FluidR3.
- Nuked-OPL3 by Nuke.YKT, which emulates the sound card's FM chip.
- SDL and wgpu, for the fundamentals.
