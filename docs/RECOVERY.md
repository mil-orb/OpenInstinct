# Recovery

Read this before your first install. Keep Garmin's untouched `System_1910/GUPDATE.GCD` on your PC the whole time you run a custom build.

## How installs work

The watch checks `GARMIN/GUPDATE.GCD` at boot and when you unplug USB. It installs the file if it's a newer version, or if the file is marked to install anyway. Then it restarts, copies the new image in, and deletes the file.

Our builds are marked to install anyway and carry version 19.09. So Garmin's own 19.10 file is always "newer" than what's on the watch, and installs over it with no edits. That's the way back to stock.

The watch's bootloader and the update code are never patched, so the update check runs Garmin's code even when a custom build is broken.

## Before every install

1. Charge above 50 %. The watch refuses updates on low battery.
2. Copy the watch's `GARMIN` folder to your PC as a backup.
3. Have Garmin's stock 19.10 `GUPDATE.GCD` ready.

## Plugging in

When you plug the watch into a PC it first shows up for a few seconds as an unknown USB device (Garmin, product id 0003), which Windows may flag with an error. Then it switches to a normal USB drive. Wait for the drive. If it never appears, unplug and plug in again. If a virtual machine grabs USB devices automatically, it can capture the watch during that switch, so release it to the host.

## What to do when something's wrong

Start at the top. Each step is more work than the one above it.

| # | Symptom | Fix | Tested |
|---|---|---|---|
| 1 | A face looks wrong | Copy another face over `GARMIN/FACES/ACTIVE.FACE`, or delete it. | Yes |
| 2 | The watch restarted while drawing a face | Nothing. The crash guard skips that file from then on. Copy a fixed face, or delete `GARMIN/FACES/STATE.BIN` to retry it. | Built in, never triggered on the watch |
| 3 | Faces cause trouble you can't pin down | Put a file named `OFF` in `GARMIN/FACES`. You get the stock face. | Built in, not tried |
| 4 | The plane finder misbehaves | Put a file named `PLANEOFF` in `GARMIN/FACES`. Holding BACK then does nothing on the face. A quick BACK press still fires on release, because that part is in the firmware patch. | Built in, not tried |
| 5 | Navigation crashes the watch | It only runs while the phone sends directions. Stop navigation or turn off Bluetooth on the phone, then go to step 6. | Not needed so far |
| 6 | Anything else, as long as the watch shows up as a USB drive | Copy Garmin's untouched 19.10 `GUPDATE.GCD` into `GARMIN`, eject, unplug. The watch installs it and restarts on stock 19.10. | Yes, settings, goals, records and activities kept |
| 7 | Step 6 didn't take | Garmin's `System_Backdate_1901` file (from the same download) forces 19.01 and **resets the watch to factory settings**. Then install 19.10 again. | Garmin ships it, not tried here |
| 8 | The watch won't boot far enough to show the USB drive | Pre-boot mode, reported on Fenix watches of the same era: hold LIGHT 15 to 30 s until the watch is fully off, start Garmin's WebUpdater on the PC, then hold START while you plug in USB and keep holding. It shows as "Garmin USB GPS", not as a drive, and WebUpdater can then send firmware. | **Not tried on an Instinct** |
| 9 | Nothing works | Garmin support. Tell them the firmware was modified. | |

## What we tested

On the development watch, in order:

1. Garmin's 19.10 image, marked to install anyway: installed, all settings kept.
2. The same image marked 19.09 (so it was a modified image): the bootloader accepted it.
3. Garmin's untouched 19.10 file over step 2: installed as a normal upgrade, back to stock.
4. Every custom build since then, each installed the same way and checked after.

Settings, goals, records, totals, activities and the `GARMIN/FACES` folder came through every step byte for byte. That's one watch, so treat it as evidence, not a promise.

## Reading the logs

- `GARMIN/Debug/err_log.txt`: the last crash, with the code address. Addresses from `0xf41d0` up are in the custom payloads.
- `GARMIN/EVNTLOGS/*.TXT`: install records. A successful install logs an update start, then a result of 1, then a restart.
