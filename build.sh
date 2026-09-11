#!/bin/bash

set -e

if command -v clang-21 >/dev/null 2>&1; then
    CC=clang-21
elif command -v clang >/dev/null 2>&1; then
    CC=clang
else
    CC=gcc
fi

echo "Using compiler: $CC"

if command -v pkg-config >/dev/null 2>&1; then
    GTK_CFLAGS=$(pkg-config --cflags gtk+-3.0 2>/dev/null || echo "")
else
    GTK_CFLAGS=""
fi

echo "Compiling gui_gtk.c..."
$CC -O2 -c -fPIC gui_gtk.c -o gui_gtk.o $GTK_CFLAGS

echo "Compiling builtins.c..."
$CC -O3 -c -fPIC -I. builtins.c -o builtins.o $GTK_CFLAGS

echo "Creating static library..."
ar rcs libbuiltins.a builtins.o gui_gtk.o

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
      -C link-arg=-lm \
      -A warnings \
      main.rs \
      -o prism

echo "Build complete: ./prism"


