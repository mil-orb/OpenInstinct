# Developing

How the repo fits together, how to build it, and how to test changes on your PC before they go near a watch.

## Layout

| Path | What's in it |
|---|---|
| `payload/src/` | Portable Rust, built for both the watch and your PC. `face.rs` (the face renderer), `planecore.rs` (plane finder logic and screens), `navcore.rs` (navigation text and screen), `lib.rs` (the navigation payload's watch glue). |
| `payload/facehook/` | The second payload: face loader, page hooks and the plane finder's watch glue (`plane.rs`). |
| `payload/host/` | PC programs: unit tests, fuzzers and mocks for the portable code. |
| `payload/build.sh`, `build_face.sh` | Build the two payloads into `payload/build/*.bin`. |
| `tools/build.sh` | Builds both payloads and patches them into your stock update file. |
| `tools/patchgcd.py`, `gcd.py` | Read and write Garmin's GCD update format. |
| `tools/bw.py`, `elf2bin.py` | Encode the branch each hook writes, and turn a payload ELF into a flat binary. |
| `tools/mock/` | Renders the nav and plane screens to PNGs from the same Rust code. |
| `tools/media/` | Makes the GIFs in `media/`. |
| `faces/` | The face kit, the bundled faces' source, and prebuilt examples. |

## Two payloads

The firmware has one free run of flash at `0xf41ca`..`0xfffff`. The build puts:

| Payload | Address | Entry points |
|---|---|---|
| Faces, page hooks, plane finder | `0xf4200` | `face_entry`, `page_entry`, `tick_entry`, `page_flags_entry` |
| Navigation | `0xfd700` (top 10.25 KB) | `nav_entry`, `tick_entry`, `vibe_entry`, `evt_entry`, `alert_entry` |

Each hook overwrites the first instruction of a stock function (or one call or one constant) with a branch into a payload. Every hook, with the bytes it expects to find and the bytes it writes, is listed in `tools/build.sh` with a comment on what it does.

The faces payload grows upward toward the nav payload. The build prints how much room is left ("free after faces"). It's about 0.6 KB now, which is the main limit on new features.

## Toolchain

- Rust 1.98.1 with `rust-src` (`rustup toolchain install 1.98.1 --component rust-src`). The build makes its own `core` for `thumbv7em-none-eabihf`, so you don't need to add the target. Set `RUST_TOOLCHAIN` to use another version. Output bytes can change between versions.
- Python 3 with Pillow. Set `PYTHON` if `python3` isn't the right command (on Windows, `python3` can be the Store stub; the scripts fall back to `python`).
- A POSIX shell for the `.sh` scripts (Git Bash works on Windows).

## Build

```bash
sh tools/build.sh path/to/System_1910/GUPDATE.GCD
```

1. Checks the stock file's SHA-256 and stops if it's anything but Garmin's 19.10 file.
2. Builds the nav payload, then the faces payload (`opt-level=z`, fat LTO, `panic=abort`).
3. Checks each payload fits its region, no two patches overlap, and every hook site holds the expected stock bytes.
4. Writes `out/GARMIN/GUPDATE.GCD`, versioned 19.09 and marked to install even over 19.10, and `out/patches.txt` with every patch.
5. Prints the sizes, the free space, every patch and the output's SHA-256.

The same source and toolchain give a byte-identical file, so two people can compare SHA-256s.

## Test on your PC

The portable code builds as plain Rust on your PC. Build the host programs with overflow checks on. If your default toolchain can't link programs on Windows, add `+stable-x86_64-pc-windows-gnu` after `rustc`.

```bash
mkdir -p out/host
rustc -O -C debug-assertions=on -C overflow-checks=on --edition 2021 payload/host/planetest.rs -o out/host/planetest
rustc -O -C debug-assertions=on -C overflow-checks=on --edition 2021 payload/host/navtest.rs -o out/host/navtest
out/host/planetest
out/host/navtest
```

| Program | Checks |
|---|---|
| `planetest` | JSON scanner, bearing and distance maths, ranking, and that every screen stays inside the round window at real font sizes. Pass saved adsb.lol replies as arguments to test them too. |
| `navtest` | Distance parsing, units, text splitting and the nav screen layout. |
| `navfuzz N` | Hostile phone text through every navcore function. |
| `fuzz N FACE...` | Mutated face files through the renderer. It must never panic or overflow. |
| `facetest` + `compare.py` | Python and Rust face renderers agree pixel for pixel ([FACES.md](FACES.md)). |
| `foldtest` | The UTF-8 to ASCII folding used for song titles (a copy of `fold()` from `facehook/src/lib.rs`). |

### Mock screens

```bash
python tools/mock/screens.py
```

This builds `navmock` and `planemock`, runs them, and renders every screen they draw to `out/mock/`, plus `nav_sheet.png` and `plane_sheet.png`. The plane screens use `tools/mock/sample_reply.json` (made-up traffic around Lisbon). The watch's fonts load at run time, so the mock uses a bold sans at about the same size. Set `MOCK_FONT` to try another font. Check text fits with a margin, since the real font differs slightly.

### GIFs

```bash
python tools/media/make_gifs.py
```

Rebuilds `media/*.gif` from the mocks and from faces in `faces/out` or `faces/examples`. Run the scripts in `faces/src` first to build every face.

## Before you flash

1. Tests and fuzzers pass, the mock screens look right.
2. `tools/build.sh` ran clean, and you read its patch list.
3. You have Garmin's stock 19.10 file to hand and have read [RECOVERY.md](RECOVERY.md).
4. Change one thing per flash. If the watch misbehaves, you'll know which change did it.

After a crash, `GARMIN/Debug/err_log.txt` on the watch has the PC and a call stack. Addresses at `0xf4200` and up are in your payloads: look them up in `payload/build/facehook.elf` or `navhook.elf`.
