# Building Robigo

Robigo builds with Cargo on Linux, Windows and macOS. The only library it needs from outside
Cargo is SDL3, which the build compiles from source and links into the binary, so each platform
needs a C compiler and CMake as well as Rust.

Linux and Windows are built and tested on every change. macOS has the code it needs and has not
been built or run yet: [macOS](#macos) says what to report.

## Every platform

1. Install Rust 1.99 or newer with [rustup](https://rustup.rs).
2. Install the platform's tools from the section below.
3. Get the source and build it:

   ```sh
   git clone https://github.com/WelshProgrammer/RobigoEngine.git
   cd Robigo
   cargo build --release
   ```

   The first build compiles SDL3 and takes a few minutes.

4. Run it:

   ```sh
   cargo run --release
   ```

   The binary is `target/release/robigo`, or `robigo.exe` on Windows, and runs from anywhere.

The first start asks where your copy of Settlers 2 is and copies it into a `RETAIL` folder beside
the binary, or into the preferences folder when the binary's folder cannot be written. The copy
can be the CD or an image of it (an ISO, or BIN and CUE files), the Gold Edition's, or a GOG
installation. A copy older than v1.51 also needs the official v1.51 patch, which the game asks
for.

The game keeps its saved games, settings and baked data in the preferences folder:

| Platform | Folder |
|---|---|
| Linux | `~/.local/share/robigo/` |
| Windows | `%APPDATA%\robigo\` |
| macOS | `~/Library/Application Support/robigo/` |

`cargo test --workspace` runs the tests, which need no game data.

## Linux

A C compiler, CMake, and the development packages of the libraries SDL3 talks to. SDL3 leaves out
any backend whose package is missing, so a build without the Wayland packages makes a binary that
runs on X11 alone.

On Debian and Ubuntu:

```sh
sudo apt install build-essential cmake ninja-build pkg-config \
    libx11-dev libxext-dev libxrandr-dev libxcursor-dev libxi-dev libxss-dev libxtst-dev libxfixes-dev \
    libwayland-dev libxkbcommon-dev libdecor-0-dev libegl-dev libgles-dev libdrm-dev libgbm-dev \
    libasound2-dev libpulse-dev libpipewire-0.3-dev libudev-dev libdbus-1-dev
```

Other distributions need the same libraries under their own package names.

Running it needs a graphics driver with Vulkan.

`dist/linux/package.sh <version>` builds the release files: the tar.gz, the AppImage and the deb.
It needs `cargo-zigbuild` and `zig` as well, which link the binary against an older glibc.

## Windows

1. Install the [Build Tools for Visual Studio](https://visualstudio.microsoft.com/downloads/)
   with the "Desktop development with C++" workload. It brings the MSVC compiler, the Windows SDK
   and CMake.
2. Install Rust with rustup and keep its default toolchain, `x86_64-pc-windows-msvc`.
3. Build from a "Developer PowerShell for VS" window, which has CMake and the compiler on its
   path:

   ```powershell
   cargo build --release
   ```

The Windows SDK's resource compiler puts the icon into `robigo.exe`, so the build fails without
the SDK.

`dist\windows\package.ps1 <version>` stages the release folder and zips it, and
`dist/windows/robigo.nsi` is the NSIS script for the setup.

## macOS

1. Install the Xcode Command Line Tools:

   ```sh
   xcode-select --install
   ```

2. Install CMake, with [Homebrew](https://brew.sh) or from cmake.org:

   ```sh
   brew install cmake
   ```

3. Install Rust with rustup, then build and run as above. The build is for the machine's own
   processor, Intel or Apple silicon.

The game draws through Metal. There is no app bundle yet, so it starts from a terminal and shows
in the Dock under the binary's name.

Nobody has built Robigo on macOS so far. If it fails, the useful things to report are:

- the last 30 lines of `cargo build --release` when the build fails;
- everything the game prints when it starts and then fails, run as
  `RUST_LOG=debug cargo run --release`;
- the macOS version and whether the Mac is Intel or Apple silicon.
