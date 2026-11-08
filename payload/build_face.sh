#!/bin/sh
# Offline build of the face loader (same toolchain and sysroot as build.sh).
# Produces build/facehook.elf and build/facehook.bin (raw bytes for 0xf4200).
set -e
cd "$(dirname "$0")"
RUST="${RUST_TOOLCHAIN:-1.98.1}"
SYSROOT="$(rustc +$RUST --print sysroot)"
HOST="$(rustc +$RUST -vV | sed -n 's/^host: //p')"
LLD="$SYSROOT/lib/rustlib/$HOST/bin/rust-lld"; [ -x "$LLD" ] || LLD="$LLD.exe"
PY="${PYTHON:-python3}"; "$PY" -c "" >/dev/null 2>&1 || PY=python
T=thumbv7em-none-eabihf
export RUSTC_BOOTSTRAP=1
[ -f sysroot/libcore.rlib ] || { echo "run build.sh once first (builds sysroot)"; exit 1; }
rustc +$RUST --edition 2021 --crate-type staticlib --crate-name facehook facehook/src/lib.rs \
  --target $T -C opt-level=z -C panic=abort -C lto=fat -C codegen-units=1 -L sysroot \
  -o build/libfacehook.a
"$LLD" -flavor gnu --nmagic -T facehook/link.x --gc-sections -e face_entry -o build/facehook.elf build/libfacehook.a
"$PY" -I ../tools/elf2bin.py build/facehook.elf build/facehook.bin
