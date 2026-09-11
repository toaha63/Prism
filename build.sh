#!/bin/bash
# build.sh - Portable build script

set -e   # Exit on any error

# Detect compiler
if command -v clang-21 >/dev/null 2>&1; then
    CC=clang-21
elif command -v clang >/dev/null 2>&1; then
    CC=clang
else
    CC=gcc
fi

echo "Using compiler: $CC"

# Detect pkg-config availability
if command -v pkg-config >/dev/null 2>&1; then
    GTK_CFLAGS=$(pkg-config --cflags gtk+-3.0 2>/dev/null || echo "")
else
    GTK_CFLAGS=""
fi

# Compile GUI wrapper
echo "Compiling gui_gtk.c..."
$CC -O2 -c -fPIC gui_gtk.c -o gui_gtk.o $GTK_CFLAGS

# Compile builtins.c
echo "Compiling builtins.c..."
$CC -O3 -c -fPIC \
    -I. \
    builtins.c -o builtins.o \
    $GTK_CFLAGS

# Create static library
echo "Creating static library..."
ar rcs libbuiltins.a builtins.o gui_gtk.o

# Compile Rust
echo "Compiling Rust interpreter..."
rustc -C opt-level=2 \
      -C panic=unwind \
      -C lto=thin \
      -C codegen-units=1 \
      -C relocation-model=pic \
      -C link-arg=-L. \
      -C link-arg=-lbuiltins \
      -C link-arg=-lgtk-3 \
      -C link-arg=-lgdk-3 \
      -C link-arg=-lgobject-2.0 \
      -C link-arg=-lcurl \
      -C link-arg=-lzip \
      -C link-arg=-lsqlite3 \
      -C link-arg=-lglib-2.0 \
      -A warnings \
      main.rs \
      sha256sum.rs \
      sha512sum.rs \
      lexer.rs \
      parser.rs \
      interpreter.rs \
      pacman.rs \
      -o prism

echo "Build complete: ./prism"
