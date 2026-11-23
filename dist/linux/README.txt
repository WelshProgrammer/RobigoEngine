Robigo
======

A modern engine for The Settlers II, written in Rust.


What you need
-------------

- 64-bit Linux with glibc 2.28 or newer: Debian 10, Ubuntu 18.10, RHEL 8 or anything later.
- A graphics driver with Vulkan.
- A copy of Settlers 2, any one of:
    - the CD, or an image of it as an ISO or as BIN and CUE files
    - the Gold Edition's CD or an image of it
    - a GOG installation of the Gold Edition
- For a copy older than v1.51, such as the first CD's v1.03, the official v1.51 patch as well.
  The Gold Edition and the GOG installation are v1.51 already.


Playing without installing
--------------------------

Run ./robigo from this folder. The first time, it asks where your copy of Settlers 2 is and copies
it into the RETAIL folder here. You can also copy it in yourself before you start: RETAIL/RETAIL.txt
says what goes in, and RETAIL/PATCH/PATCH.txt where the patch goes.


Installing
----------

    ./install.sh

puts Robigo under ~/.local with an entry in the applications menu. To install it for every user,
run it as root, which puts it under /usr/local:

    sudo ./install.sh

--prefix DIR picks another place. Once installed, this folder is no longer needed, though the
script is what removes Robigo again, so keep that or the archive.

An installed Robigo keeps your copy of Settlers 2 in ~/.local/share/robigo/RETAIL. The first start
asks where your copy is and copies it there.


Removing
--------

    ./install.sh --uninstall

Run it the way you installed: with sudo, or with the same --prefix. It removes the program, its
menu entry and its icons. Your saved games, your settings and your copy of Settlers 2 stay in
~/.local/share/robigo. Delete that folder as well to remove everything.


The first start
---------------

Robigo reads your copy once and bakes what it needs into ~/.local/share/robigo/mods, which takes a
minute or two. It never changes the copy in RETAIL. It then offers to download a sound font for
the General MIDI music; without it the music still plays.

The CD's music tracks play only from a CD image that holds them, or from a GOG installation.
A folder of files copied off the CD has the game and no CD music.


Where everything is kept
------------------------

~/.local/share/robigo/
    RETAIL/          your copy of Settlers 2, when Robigo is installed
    mods/            what Robigo baked from it
    saves/           your saved games
    soundfonts/      sound fonts it downloaded
    settings.json    your settings

If XDG_DATA_HOME is set, the folder is $XDG_DATA_HOME/robigo.
