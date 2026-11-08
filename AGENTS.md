# Rules for AI coding agents

These apply to any agent (Claude Code, Codex, Copilot and the like) working in this repo. People are welcome to follow them too.

## Never

- **Never flash a watch, or write to a watch's drive, without the person's go-ahead for that exact file.** Before asking, say what changed, what could go wrong, and how to get back (Garmin's stock 19.10 file, [docs/RECOVERY.md](docs/RECOVERY.md)). One approval covers one file.
- **Never add Garmin firmware** or anything cut from it to the repo: no `.GCD` files, no dumps, no disassembly listings, no decompiled code. Addresses and short byte patterns for patch checks are fine.
- **Never download anything without asking first**: packages, toolchains, fonts, firmware, data sets. Say what, from where and how big.
- **Never patch the bootloader or the update path.** They're how a broken build gets fixed.
- **Never add a timer, poll or sensor that runs on its own.** Work happens on events the watch already has (the face's minute repaint, a key, a phone message), and sensors run only while a page that needs them is open. State the battery cost of any change in its description.

## Always

- Put logic in the portable files (`payload/src/*core.rs`, `face.rs`) and test it on the PC first: unit tests, the fuzzers if you touched parsing, and the mock screens if you touched layout ([docs/DEVELOPING.md](docs/DEVELOPING.md)).
- Lay out text from the font metrics the watch reports, and check every box against the round window with `box_ok`.
- Keep watch code free of writable statics, keep stack frames small, and handle every firmware call that can fail.
- Run `sh tools/build.sh` and report the free space it prints. If a change grows the faces payload, say by how much.
- Use made-up data in tests, mocks and docs. No real locations, names, or logs from anyone's watch or phone.
- Run Python that reads downloaded data with `-I`.

## Writing

Docs and comments are short and specific: numbers, file names, what was tested and what wasn't. No marketing words. If something is untested, say so.
