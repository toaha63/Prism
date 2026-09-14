
#!/bin/bash
# Prism build script — works in Termux (local) and GitHub CI (portable)
#
# Usage:
#   ./build.sh              # GUI build, dynamic
#   ./build.sh --no-gui     # terminal-only, dynamic

set -e

# ---------- Parse flags ----------
GUI_ENABLED=1
for arg in "$@"; do
    if [ "$arg" = "--no-gui" ]; then
        GUI_ENABLED=0
    fi
done

if [ "$GUI_ENABLED" = "1" ]; then
    echo "Building Prism WITH GUI support..."
else
    echo "Building Prism WITHOUT GUI support..."
fi

# ---------- Detect CI vs local ----------
# In CI: use portable ISA (never -march=native) so binary runs on any CPU
# Locally: use native CPU + aggressive LTO
if [ -n "$CI" ] || [ -n "$GITHUB_ACTIONS" ]; then
    ARCH_FLAGS="-mtune=generic"
    RUST_TUNING="-C lto=fat -C codegen-units=1 -C opt-level=3 -C prefer-dynamic=no"
    echo "CI detected — portable CPU flags, full optimization"
else
    ARCH_FLAGS="-march=native -mtune=native"
    RUST_TUNING="-C lto=fat -C target-cpu=native -C codegen-units=1 -C opt-level=3 -C prefer-dynamic=no"
    echo "Local build — native CPU optimizations"
fi

# ---------- Compiler ----------
if command -v clang-21 >/dev/null 2>&1; then
    CC=clang-21
elif command -v clang >/dev/null 2>&1; then
    CC=clang
else
    CC=gcc
fi
echo "Using compiler: $CC"

# ---------- Common C flags (maximum optimization) ----------
C_FLAGS="-O3 $ARCH_FLAGS -flto -funroll-loops -ffast-math -fomit-frame-pointer \
    -fstrict-aliasing -fno-signed-zeros -freciprocal-math -fno-trapping-math \
    -fassociative-math"

# ---------- GTK detection (GUI mode only) ----------
if [ "$GUI_ENABLED" = "1" ]; then
    if command -v pkg-config >/dev/null 2>&1; then
        GTK_CFLAGS=$(pkg-config --cflags gtk+-3.0 2>/dev/null || echo "")
    else
        GTK_CFLAGS=""
    fi
fi

# ---------- Compile C sources ----------
if [ "$GUI_ENABLED" = "1" ]; then
    echo "Compiling gui_gtk.c..."
    $CC $C_FLAGS -c -fPIC gui_gtk.c -o gui_gtk.o $GTK_CFLAGS
fi

echo "Compiling builtins.c..."
if [ "$GUI_ENABLED" = "1" ]; then
    $CC $C_FLAGS -c -fPIC -I. -DUSE_GUI=1 builtins.c -o builtins.o $GTK_CFLAGS
else
    $CC $C_FLAGS -c -fPIC -I. -DUSE_GUI=0 builtins.c -o builtins.o
fi

# ---------- Create static library ----------
echo "Creating static library..."
if [ "$GUI_ENABLED" = "1" ]; then
    ar rcs libbuiltins.a builtins.o gui_gtk.o
else
    ar rcs libbuiltins.a builtins.o
fi

# ---------- Compile Rust (maximum optimization) ----------
echo "Compiling Rust interpreter..."

RUST_COMMON="-C panic=unwind \
    -C lto=fat \
    -C codegen-units=1 \
    -C prefer-dynamic=no \
    -C relocation-model=pic \
    -C debuginfo=0 \
    -C strip=debuginfo \
    -C opt-level=3 \
    -C link-arg=-L. \
    -C link-arg=-lbuiltins \
    -A warnings"

RUST_LIBS="-C link-arg=-lcurl \
    -C link-arg=-lzip \
    -C link-arg=-lsqlite3 \
    -C link-arg=-lc \
    -C link-arg=-lm"

if [ "$GUI_ENABLED" = "1" ]; then
    rustc $RUST_COMMON $RUST_TUNING \
          --cfg gui \
          -C link-arg=-lgtk-3 \
          -C link-arg=-lgdk-3 \
          -C link-arg=-lgobject-2.0 \
          -C link-arg=-lglib-2.0 \
          $RUST_LIBS \
          main.rs \
          -o prism
else
    rustc $RUST_COMMON $RUST_TUNING \
          $RUST_LIBS \
          main.rs \
          -o prism
fi

strip --strip-all prism 2>/dev/null || true

# ---------- Smoke test ----------
if [ -f main.prism ]; then
    echo "Running smoke test: ./prism main.prism"
    ./prism main.prism
    echo "Smoke test passed"
fi

echo "Build complete: ./prism"
