#!/bin/bash

clear
# Compile GUI wrapper
clang-21 -O3 -c -fPIC gui_gtk.c -o gui_gtk.o `pkg-config --cflags gtk+-3.0` 

# Compile builtins.c with warnings suppressed
clang-21 -O3 -march=native -mtune=native -flto -funroll-loops -ffast-math \
    -fvectorize -fslp-vectorize -fstrict-aliasing -fomit-frame-pointer \
    -fno-signed-zeros -freciprocal-math -fno-trapping-math -fassociative-math \
    -mllvm -unroll-threshold=150 -mllvm -unroll-max-iteration-count-to-analyze=64 \
    -mllvm -unroll-count=8 -mllvm -vectorize-memory-check-threshold=1024 \
    -mllvm -enable-interleaved-mem-accesses -c -fPIC \
    -lgtk-3 -lgdk-3 -lgobject-2.0 -lglib-2.0 builtins.c -o builtins.o \
    -Wno-unused-command-line-argument

# Create static library
ar rcs libbuiltins.a builtins.o gui_gtk.o

# Compile Rust with warnings suppressed
rustc -C panic=unwind \
      -C lto=fat \
      -C target-cpu=native \
      -C codegen-units=1 \
      -C prefer-dynamic=no \
      -C relocation-model=pic \
      -C link-arg=-L. \
      -C link-arg=-lbuiltins \
      -C link-arg=-lgtk-3 \
      -C link-arg=-lgdk-3 \
      -C link-arg=-lgobject-2.0 \
      -C link-arg=-lcurl \
      -C link-arg=-lc \
      -C link-arg=-lzip \
      -C link-arg=-lm \
      -C link-arg=-lglib-2.0 \
      -C debuginfo=0 \
      -C opt-level=0\
      -C strip=debuginfo \
      -A warnings \
      main.rs

# Aggressive stripping
strip --strip-all main
echo "Build complete. Run with: ./main main.prism"