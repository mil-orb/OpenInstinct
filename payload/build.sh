#!/bin/sh
# Build of the nav payload: rustc (RUST_TOOLCHAIN, default 1.98.1) + its rust-src + bundled rust-lld.
# Produces build/navhook.elf and build/navhook.bin (raw bytes for 0xfd700).
set -e
cd "$(dirname "$0")"
RUST="${RUST_TOOLCHAIN:-1.98.1}"
SYSROOT="$(rustc +$RUST --print sysroot)"
HOST="$(rustc +$RUST -vV | sed -n 's/^host: //p')"
LLD="$SYSROOT/lib/rustlib/$HOST/bin/rust-lld"; [ -x "$LLD" ] || LLD="$LLD.exe"
PY="${PYTHON:-python3}"; "$PY" -c "" >/dev/null 2>&1 || PY=python
SRC="$SYSROOT/lib/rustlib/src/rust/library"
[ -d "$SRC" ] || { echo "needs rust-src: rustup component add rust-src --toolchain $RUST"; exit 1; }
T=thumbv7em-none-eabihf
export RUSTC_BOOTSTRAP=1
mkdir -p sysroot build
if [ ! -f sysroot/libcore.rlib ]; then
  rustc +$RUST --edition 2024 --crate-type rlib --crate-name core "$SRC/core/src/lib.rs" \
    --target $T -C opt-level=s -C panic=abort -Z force-unstable-if-unmarked --out-dir sysroot
fi
if [ ! -f sysroot/libcompiler_builtins.rlib ]; then
  rustc +$RUST --edition 2021 --crate-type rlib --crate-name compiler_builtins cb.rs \
    --target $T -C opt-level=s -C panic=abort -L sysroot --out-dir sysroot -A warnings
fi
rustc +$RUST --edition 2021 --crate-type staticlib --crate-name navhook src/lib.rs \
  --target $T -C opt-level=z -C panic=abort -C lto=fat -C codegen-units=1 -L sysroot \
  -o build/libnavhook.a
"$LLD" -flavor gnu --nmagic -T link.x --gc-sections -e nav_entry -o build/navhook.elf build/libnavhook.a
"$PY" -I ../tools/elf2bin.py build/navhook.elf build/navhook.bin
