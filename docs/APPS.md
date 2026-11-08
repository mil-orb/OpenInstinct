# Making apps

An app is a page of its own in the watch's UI, written in Rust (`no_std`, no allocator). It's opened from a button on the watch face, draws with the watch's own fonts, reads the buttons, compass and GPS, and can make HTTP requests through the phone. The plane finder is the worked example, and this guide walks through it.

Read [DEVELOPING.md](DEVELOPING.md) first for the build and the test loop.

## The shape of an app

Every app is split in two, so most of it can be tested on your PC:

| Part | Plane finder | Runs on | What goes here |
|---|---|---|---|
| **Core** | [payload/src/planecore.rs](../payload/src/planecore.rs) | PC and watch | Parsing, maths, ranking, and all layout and drawing, written against a small `Draw` trait. No firmware calls. |
| **Glue** | [payload/facehook/src/plane.rs](../payload/facehook/src/plane.rs) | Watch only | The page handler, events, firmware calls (compass, GPS, HTTP), state, and a `Draw` that calls the watch's drawing functions. |
| **Tests** | [payload/host/planetest.rs](../payload/host/planetest.rs) | PC | Unit tests of the core with overflow checks on. |
| **Mock** | [payload/host/planemock.rs](../payload/host/planemock.rs) | PC | Prints every screen's drawing calls. [tools/mock/screens.py](../tools/mock/screens.py) renders them to PNGs. |

`Draw` is all the core sees of the screen:

```rust
pub trait Draw {
    fn rect(&mut self, ink: bool, x: i32, y: i32, w: i32, h: i32);
    fn poly(&mut self, ink: bool, pts: &[i16]);            // x0, y0, x1, y1, ...
    fn text(&mut self, font: u8, x: i32, top: i32, align: u32, s: &[u8]);
    fn width(&mut self, font: u8, s: &[u8]) -> i32;
    fn metrics(&mut self, font: u8) -> (i32, i32);        // (cell height, ascent)
}
```

On the watch, `text` uses the firmware's own fonts, which load at run time. Lay out from `metrics()` and `width()`, never fixed sizes. `planecore.rs` has `fit_c` (cut text until it fits) and `box_ok` (is a box inside the round window and clear of the porthole). Use them for every line.

## Life of a page

1. **Opening.** The face page's key events pass through `page_entry` in [lib.rs](../payload/facehook/src/lib.rs). When it sees hold BACK (code `0x21`) it calls `plane::open()`, which asks the UI to open a page whose handler is `plane_page`, then sets the dispatcher's hand-off byte to 2 (see below).
2. **Events.** The UI calls the page handler with the event id at `EVT + 4` and, for keys, the key code at `EVT + 6`:

   | Event | Id | Plane finder does |
   |---|---|---|
   | create | `0x82` | Allocates its state block, sets up the canvas, registers the compass, starts a 250 ms page timer. |
   | paint | `0x5f` | Draws the current screen into the offscreen canvas. |
   | key | `0x2a` | BACK `0x05`, hold BACK `0x21`, BACK released after a hold `0x85`, GPS `0x04`, UP `0x0d`, DOWN `0x08`. |
   | tick | `0xf0` | Samples the compass, checks for the HTTP reply, asks for a repaint only when something changed. |
   | destroy | `0x53` | Releases the compass, hands an in-flight request to its callback, and frees nothing else (the UI frees the state block). |

3. **The hand-off byte** (`0x2001f4f4`). The dispatcher sets it to 1 before calling a page and skips the paint pass if it's still 1 afterwards. So a handler that handled create, paint, destroy or a key must store 2, or its drawing never reaches the screen. The stock popup does the same.
4. **State** lives in a block the UI allocates per page (`PAGE_STATE`) and frees on close. Put a magic number at the start and check it before use. Put buffers there, not on the stack: the UI task's stack is small. The plane finder keeps each event handler `#[inline(never)]` so only one handler's locals are on the stack at a time.
5. **No writable statics.** The linker script rejects `.data` and `.bss`, since the payload sits in flash with no RAM of its own. Everything mutable goes in the state block, or in an allocation you free yourself.
6. **Repaint only on change.** The tick sets a dirty flag. Paint when it's set and leave the LCD alone otherwise. A full-screen flush costs power.

## Services

Firmware calls are plain function pointers at fixed addresses in 19.10 (`const`s at the top of `plane.rs`, called through small `f0`..`f3` helpers).

| Service | Calls | Notes |
|---|---|---|
| Compass | `COMPASS_REG(period_ms) -> slot`, `COMPASS_GET(&f32) -> valid`, `COMPASS_REL(slot)` | Heading in radians from true north. Register only while you need it, and release on error screens and on destroy. |
| Position | `POSITION(&pos, 1)` | Last known fix in semicircles. Invalid values: `0x7fffffff`, or 0,0. |
| Phone position | `LOC_ASK()`, then `PHONE_POS(&rec)` | Asks the phone over the protobuf link; Gadgetbridge answers with Android's last fix. `rec` holds lat/lon in semicircles, the fix time (compare with `GARMIN_NOW`), accuracy in metres and a valid byte. Check `PB_LINK != 0xffff` before asking. See `read_pos` in `plane.rs`. |
| Phone | `PHONE() -> connected` | Check before any request. |
| HTTP via phone | `HTTP_INIT(req)`, set URL and callback, `HTTP_SEND(req) -> id` | Gadgetbridge fetches the URL. Allow its host in Gadgetbridge's internet settings. The callback gets `(id, status, body, len, http_code, ctx)`. |
| Time | `TICK_MS() -> ms` | Monotonic milliseconds. |
| Files | open, read, write, exists | Paths like `0:/GARMIN/FACES/PLANEOFF`. Use them for settings and kill switches. |

### HTTP without crashing

The reply callback can arrive after the page is gone, or never. The plane finder keeps the request's buffer in a separate block with an atomic state: idle, pending, or orphaned. `destroy` and timeouts mark a pending request orphaned, and the callback frees an orphaned block instead of touching the page. Copy that pattern (`Hdr`, `http_cb`, `send`, `drop_req`; the route lookup `Rt` is a second user) rather than writing your own.

A reply over 64 KB fails (the plane finder shows "unreadable reply"), so keep requests small. The plane finder asks for one radius around you, about 500 bytes per aircraft.

## Debugging on the watch

- `trap(code, a, b)` executes a permanently undefined instruction. The watch writes its registers to `GARMIN/Debug/err_log.txt` and restarts. r0 is your code, r1 is `0x0badc0de`, and r2 and r3 are your details. The plane finder traps on an event storm (over 100 events in a second) and on any handler call that takes over 3 seconds.
- `err_log.txt` keeps the PC and a call stack for any crash. Match the addresses against `payload/build/facehook.elf`.

## Adding your own app

1. Write `payload/src/<app>core.rs` with the logic and a `paint_*` function per screen, against `Draw`.
2. Write `payload/host/<app>test.rs` and `<app>mock.rs` (copy the plane ones) and get the tests passing and the mock screens looking right.
3. Write `payload/facehook/src/<app>.rs` with the page handler, `open()`, and a `Draw` for the watch (copy `Gfx` from `plane.rs`). Add `mod <app>;` in `lib.rs`, and hook a key event in `page_entry` to open it.
4. Give your page flags: `page_flags_entry` in `lib.rs` answers for `plane_page`. Add your handler the same way.
5. Build with `sh tools/build.sh ...` and watch the "free after faces" figure. The face loader and all apps share one region (under 0.1 KB free right now), so size matters. `opt-level=z` and LTO are already on, and the next step is moving code into the nav region or finding more free flash.
6. Flash with the stock 19.10 file at hand ([RECOVERY.md](RECOVERY.md)).

Ideas that fit the same pattern: a tide or weather detail page, a transit departure board, a phone finder, a "where did I park" compass page.
