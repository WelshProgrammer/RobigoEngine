#!/usr/bin/env bash
# Builds and packages the Linux release into out/: a tar.gz with its install script, an AppImage
# and a deb. The binary is linked against glibc 2.28 with cargo-zigbuild, so it starts on
# distributions back to Debian 10, RHEL 8 and Ubuntu 18.10. Needs cargo-zigbuild and zig on the
# PATH, SDL's build dependencies and dpkg-deb.
#
# usage: dist/linux/package.sh <version>
set -euo pipefail

version=${1:?usage: package.sh <version>}

GLIBC=2.28
TARGET=x86_64-unknown-linux-gnu
APPIMAGETOOL_URL=https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage
APPIMAGETOOL_SHA256=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
RUNTIME_URL=https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64
RUNTIME_SHA256=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d
MAINTAINER="WelshProgrammer <welshprogrammer@thecodecache.net>"

# The icon sizes the hicolor theme has folders for.
ICONS="16 24 32 48 64 256"

# The libraries SDL loads at run time. Each name is in the binary only when SDL's build found
# that backend's headers and library.
BACKENDS="libX11.so.6 libwayland-client.so.0 libasound.so.2 libpulse.so.0 libpipewire-0.3.so.0 libudev.so.1"

root=$(cd "$(dirname "$0")/../.." && pwd)
binary=${CARGO_TARGET_DIR:-$root/target}/$TARGET/release/robigo
out=$root/out
name=robigo-$version-linux-x86_64

# zig's compiler looks in neither the system's include folders nor its multiarch library folder,
# and SDL's build drops every backend it cannot find there.
multiarch=$(gcc -print-multiarch)
(
    cd "$root"
    ROBIGO_VERSION=$version \
        CFLAGS="-idirafter /usr/include -idirafter /usr/include/$multiarch" \
        CMAKE_LIBRARY_PATH=/usr/lib/$multiarch \
        cargo zigbuild --release --locked -p robigo --target "$TARGET.$GLIBC"
)

built=$("$binary" --version)
if [ "$built" != "robigo $version" ]; then
    echo "the binary built is '$built', and the release is $version" >&2
    exit 1
fi
needs=$(objdump -T "$binary" | grep -o 'GLIBC_[0-9.]*' | sort -V | tail -1)
if [ "$needs" != "$(printf '%s\n' "$needs" "GLIBC_$GLIBC" | sort -V | head -1)" ]; then
    echo "the binary needs $needs, and the release promises glibc $GLIBC" >&2
    exit 1
fi
for backend in $BACKENDS; do
    if ! grep --quiet --text --fixed-strings "$backend" "$binary"; then
        echo "SDL was built without $backend" >&2
        exit 1
    fi
done

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$out"

fetch() {
    curl --fail --silent --show-error --location --output "$2" "$1"
    echo "$3  $2" | sha256sum --check --quiet
}

# The notes are copied by name, so a game a developer keeps in dist/RETAIL stays out.
stage=$work/$name
install -D -m 755 "$binary" "$stage/robigo"
install -D -m 644 "$root/LICENSE" "$stage/LICENSE"
install -D -m 644 "$root/dist/RETAIL/RETAIL.txt" "$stage/RETAIL/RETAIL.txt"
install -D -m 644 "$root/dist/RETAIL/PATCH/PATCH.txt" "$stage/RETAIL/PATCH/PATCH.txt"
install -D -m 755 "$root/dist/linux/install.sh" "$stage/install.sh"
install -D -m 644 "$root/dist/linux/README.txt" "$stage/README.txt"
install -D -m 644 "$root/dist/linux/robigo.desktop" "$stage/robigo.desktop"
for size in $ICONS; do
    install -D -m 644 "$root/assets/icons/robigo-$size.png" "$stage/icons/robigo-$size.png"
done
tar --create --gzip --owner=0 --group=0 --numeric-owner --file "$out/$name.tar.gz" --directory "$work" "$name"

# An AppImage is mounted read-only, so the game finds no RETAIL beside its binary there and
# uses the one in the preferences folder.
app=$work/AppDir
install -D -m 755 "$binary" "$app/usr/bin/robigo"
install -D -m 644 "$root/LICENSE" "$app/usr/share/licenses/robigo/LICENSE"
install -D -m 644 "$root/dist/linux/robigo.desktop" "$app/robigo.desktop"
install -D -m 644 "$root/assets/icons/robigo-256.png" "$app/robigo.png"
ln -s robigo.png "$app/.DirIcon"
ln -s usr/bin/robigo "$app/AppRun"

fetch "$APPIMAGETOOL_URL" "$work/appimagetool" "$APPIMAGETOOL_SHA256"
fetch "$RUNTIME_URL" "$work/runtime" "$RUNTIME_SHA256"
chmod +x "$work/appimagetool"
ARCH=x86_64 "$work/appimagetool" --appimage-extract-and-run --no-appstream \
    --runtime-file "$work/runtime" "$app" "$out/$name.AppImage"

# A Debian version sorts 1.0.0~rc1 before 1.0.0, and 1.0.0-rc1 after it.
deb=$work/deb
install -D -m 755 "$binary" "$deb/usr/bin/robigo"
install -D -m 644 "$root/dist/linux/robigo.desktop" "$deb/usr/share/applications/robigo.desktop"
for size in $ICONS; do
    install -D -m 644 "$root/assets/icons/robigo-$size.png" \
        "$deb/usr/share/icons/hicolor/${size}x$size/apps/robigo.png"
done
install -D -m 644 "$root/dist/linux/copyright" "$deb/usr/share/doc/robigo/copyright"
mkdir "$deb/DEBIAN"
cat > "$deb/DEBIAN/control" <<CONTROL
Package: robigo
Version: $(printf '%s' "$version" | tr - '~')
Architecture: amd64
Maintainer: $MAINTAINER
Installed-Size: $(du --summarize --block-size=1024 "$deb/usr" | cut -f1)
Depends: libc6 (>= $GLIBC)
Recommends: libvulkan1, libxkbcommon0, libdecor-0-0
Section: games
Priority: optional
Description: A modern engine for The Settlers II, written in Rust
 Robigo runs Settlers 2 from the graphics, sounds and maps of a copy of the game.
 The first start asks where that copy is.
CONTROL
dpkg-deb --root-owner-group --build "$deb" "$out/$name.deb" > /dev/null

ls -l "$out"
