#!/usr/bin/env bash
# Build a self-contained x86_64 AppImage for OpenRouter Image Studio.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="openrouter-image-studio"
BIN="$ROOT/target/release/$APP"
APPDIR="$ROOT/packaging/AppDir"
ARCH="x86_64"
TOOLS="$ROOT/packaging/.tools"
LIBDIR="/usr/lib/x86_64-linux-gnu"

echo "==> Building release binary"
cd "$ROOT"
cargo build --release

echo "==> Preparing AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/lib" "$APPDIR/usr/share/applications" \
         "$APPDIR/usr/share/icons/hicolor/256x256/apps" \
         "$APPDIR/usr/share/glib-2.0/schemas" \
         "$APPDIR/usr/lib/gdk-pixbuf-2.0/2.10.0/loaders" \
         "$APPDIR/usr/lib/gio/modules"

cp "$BIN" "$APPDIR/usr/bin/$APP"
cp "$ROOT/assets/icon.png" "$APPDIR/$APP.png"
cp "$ROOT/assets/icon.png" "$APPDIR/.DirIcon"
cp "$ROOT/assets/icon.png" "$APPDIR/usr/share/icons/hicolor/256x256/apps/$APP.png"
cp "$ROOT/packaging/$APP.desktop" "$APPDIR/$APP.desktop"
cp "$ROOT/packaging/$APP.desktop" "$APPDIR/usr/share/applications/$APP.desktop"
cp "$ROOT/packaging/AppRun" "$APPDIR/AppRun"
chmod +x "$APPDIR/AppRun" "$APPDIR/usr/bin/$APP"

is_excluded() {
  case "$1" in
    /lib*/ld-linux*|/lib*/libc.so*|/lib*/libm.so*|/lib*/libpthread*|/lib*/libdl.so*|/lib*/librt.so*|/lib*/libresolv.so*|/lib*/libgcc_s.so*|/lib*/libstdc++.so*|linux-vdso*|/lib*/libnss_*|/lib*/libutil.so*|/lib*/libanl.so*|/lib*/libBrokenLocale*|/lib*/libthread_db*)
      return 0 ;;
    *) return 1 ;;
  esac
}

copy_deps() {
  ldd "$1" 2>/dev/null | awk '{ for (i=1;i<=NF;i++) if ($i ~ /\//) print $i }' | while read -r lib; do
    [ -e "$lib" ] || continue
    if is_excluded "$lib"; then continue; fi
    dest="$APPDIR/usr/lib/$(basename "$lib")"
    if [ ! -e "$dest" ]; then
      cp -L "$lib" "$dest"
    fi
  done
}

echo "==> Bundling shared libraries"
copy_deps "$BIN"

echo "==> Bundling gdk-pixbuf loaders"
for so in "$LIBDIR"/gdk-pixbuf-2.0/2.10.0/loaders/*.so; do
  cp -L "$so" "$APPDIR/usr/lib/gdk-pixbuf-2.0/2.10.0/loaders/"
  copy_deps "$so"
done

echo "==> Bundling GIO modules"
if [ -d "$LIBDIR/gio/modules" ]; then
  for so in "$LIBDIR"/gio/modules/*.so; do
    [ -e "$so" ] || continue
    cp -L "$so" "$APPDIR/usr/lib/gio/modules/"
    copy_deps "$so"
  done
fi

echo "==> Bundling GLib schemas"
cp /usr/share/glib-2.0/schemas/*.xml "$APPDIR/usr/share/glib-2.0/schemas/" 2>/dev/null || true
cp /usr/share/glib-2.0/schemas/*.gschema.override "$APPDIR/usr/share/glib-2.0/schemas/" 2>/dev/null || true
glib-compile-schemas "$APPDIR/usr/share/glib-2.0/schemas"

echo "==> Bundling icon theme (Adwaita)"
mkdir -p "$APPDIR/usr/share/icons"
cp -r /usr/share/icons/Adwaita "$APPDIR/usr/share/icons/"
if [ -f /usr/share/icons/hicolor/index.theme ]; then
  mkdir -p "$APPDIR/usr/share/icons/hicolor"
  cp /usr/share/icons/hicolor/index.theme "$APPDIR/usr/share/icons/hicolor/"
fi

echo "==> Generating gdk-pixbuf loader cache"
QUERY="$LIBDIR/gdk-pixbuf-2.0/gdk-pixbuf-query-loaders"
if [ -x "$QUERY" ]; then
  "$QUERY" "$APPDIR"/usr/lib/gdk-pixbuf-2.0/2.10.0/loaders/*.so > "$APPDIR/usr/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache"
  sed -i "s|$APPDIR|@APPDIR@|g" "$APPDIR/usr/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache"
fi

echo "==> Generating GIO module cache"
gio-querymodules "$APPDIR/usr/lib/gio/modules" || true

echo "==> Fetching appimagetool"
mkdir -p "$TOOLS"
AT="$TOOLS/appimagetool-x86_64.AppImage"
if [ ! -x "$AT" ]; then
  curl -fsSL -o "$AT" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage" \
    || curl -fsSL -o "$AT" "https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage"
  chmod +x "$AT"
fi

echo "==> Building AppImage"
OUT="$ROOT/$APP-$ARCH.AppImage"
rm -f "$OUT"
ARCH="$ARCH" APPIMAGE_EXTRACT_AND_RUN=1 "$AT" "$APPDIR" "$OUT"
chmod +x "$OUT"

echo "==> Done: $OUT"
ls -la "$OUT"
