#!/bin/sh
# Installs Robigo from this folder, or removes it again. As root it goes under /usr/local, and
# otherwise under ~/.local.
#
# usage: ./install.sh [--prefix DIR] [--uninstall]
set -eu

usage() {
    echo "usage: $0 [--prefix DIR] [--uninstall]"
}

here=$(cd "$(dirname "$0")" && pwd)
if [ "$(id -u)" -eq 0 ]; then
    prefix=/usr/local
else
    prefix=$HOME/.local
fi
uninstall=

while [ $# -gt 0 ]; do
    case $1 in
        --prefix)
            prefix=${2:?--prefix needs a folder}
            shift 2
            ;;
        --uninstall)
            uninstall=yes
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            usage >&2
            exit 2
            ;;
    esac
done

binary=$prefix/bin/robigo
desktop=$prefix/share/applications/robigo.desktop
icons=$prefix/share/icons/hicolor
licence=$prefix/share/licenses/robigo/LICENSE
data=${XDG_DATA_HOME:-$HOME/.local/share}/robigo

# The theme folder an icon file goes in: 48x48 for robigo-48.png.
size_of() {
    size=${1##*-}
    size=${size%.png}
    echo "${size}x$size"
}

refresh() {
    if command -v update-desktop-database > /dev/null 2>&1; then
        update-desktop-database "$prefix/share/applications" 2> /dev/null || true
    fi
}

if [ -n "$uninstall" ]; then
    rm -f "$binary" "$desktop" "$licence"
    for picture in "$here"/icons/robigo-*.png; do
        rm -f "$icons/$(size_of "$picture")/apps/robigo.png"
    done
    rmdir "$(dirname "$licence")" 2> /dev/null || true
    refresh
    echo "Robigo is removed from $prefix."
    echo "Your saved games, settings and copy of Settlers 2 stay in $data."
    exit 0
fi

install -D -m 755 "$here/robigo" "$binary"
for picture in "$here"/icons/robigo-*.png; do
    install -D -m 644 "$picture" "$icons/$(size_of "$picture")/apps/robigo.png"
done
install -D -m 644 "$here/LICENSE" "$licence"
mkdir -p "$(dirname "$desktop")"
sed "s|^Exec=.*|Exec=$binary|" "$here/robigo.desktop" > "$desktop"
chmod 644 "$desktop"
refresh

echo "Robigo is installed in $prefix."
case ":$PATH:" in
    *":$prefix/bin:"*) ;;
    *) echo "$prefix/bin is not on your PATH, so start it from the applications menu or as $binary." ;;
esac

# The game looks for the player's copy beside its binary first. An installed binary has none
# there, so the copy goes in the preferences folder.
if [ "$(id -u)" -ne 0 ]; then
    mkdir -p "$data/RETAIL/PATCH"
    [ -e "$data/RETAIL/RETAIL.txt" ] || cp "$here/RETAIL/RETAIL.txt" "$data/RETAIL/"
    [ -e "$data/RETAIL/PATCH/PATCH.txt" ] || cp "$here/RETAIL/PATCH/PATCH.txt" "$data/RETAIL/PATCH/"
    echo "Copy your Settlers 2 into $data/RETAIL, or start Robigo and it asks where your copy is."
else
    echo "Start Robigo and it asks where your copy of Settlers 2 is."
fi
