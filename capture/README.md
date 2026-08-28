# Capturing golden traffic

This is the procedure for recording real HID traffic between the vendor web
configurator and a Wallhack K-001 keyboard, so `cargo test -p wh-proto --test
golden` has something real to check the codec against. Nothing in this repo
has ever been checked against a byte the real device actually sent; this is
how we get that.

Captures are committed to this repository, with the device serial number
redacted automatically by the shim (see "Redaction" below). Do not hand-edit
the hex to redact anything yourself.

Before you start, note which profile is active on the keyboard, and note the
serial number printed on the device or its packaging somewhere outside this
repository (your own notes, not a comment in the capture file). You will use
it in step 6's redaction check. Nothing in the capture itself records which
profile was active, and it may matter when the captures are read back later.

1. Close any running `wh` process. Open Chrome on Windows.
2. Open DevTools (F12) on a blank tab, console tab. Navigate to
   https://terminal.wallhack.com/ but paste `capture/hid-shim.js` into the
   console the moment the page starts loading (or use
   `--remote-debugging-port=9222` with
   `Page.addScriptToEvaluateOnNewDocument`), so the shim is installed before
   the page ever opens the device. You should see `[wh] HID shim installed`
   printed. If you do not, and nothing logs later either, the DevTools
   console context selector (top left of the console panel, next to
   "top") is probably pointed at the wrong frame: it must be on the frame
   that actually owns the WebHID call, which may not be the top-level page
   if the configurator UI is embedded.

   If the shim was pasted in **after** the page had already opened the
   device (a page reload was skipped, or the device was already connected
   from a previous test), `open()` never gets called again and the shim's
   reply listener never attaches. You will see outgoing writes log but no
   inbound replies at all. If that happens, hard-reload the page and start
   over. The golden test also flags any capture file with zero inbound
   frames as a warning, but catching it live saves a wasted session.
3. Connect the keyboard in the web UI. Before your first real scenario,
   trigger a feature-report exchange once if the UI has one (switch
   calibration and switch-type selection, per the configurator's own
   screenshots, look like `sendFeatureReport`/`receiveFeatureReport`
   candidates) and run `window.__wh.log[window.__wh.log.length - 1]` in the
   console. Check `hex.length`: if it is 130 (65 bytes) instead of 128 (64
   bytes), or if `hex.slice(0, 2)` matches the entry's own `report_id` field
   when that field is non-zero, Chrome is including the report ID as the
   first byte of the feature report data, unlike a normal input report.
   Note that in whatever you use to record the session (a PR description is
   fine) so a reviewer reading `in-feature`/`out-feature` lines later knows
   whether byte 0 is real payload or a report ID. `window.__wh.clear()`
   afterward to drop this probe before your first real scenario.
4. Work through the scenarios below in order. Perform ONE single-variable
   change per capture. That is what makes a diff between two captures
   readable, and it is the whole method: resist the urge to combine two
   changes into one capture even when it would be faster at the keyboard.
5. Before each scenario, run `window.__wh.begin("scenario-name")` in the
   console instead of `window.__wh.clear()`. It does the same thing, but it
   also prints `[wh] capture started: scenario-name` so a forgotten clear
   between two scenarios is visible immediately, at the keyboard, rather
   than discovered later as two scenarios silently merged into one capture
   file with no signal that it happened.
6. After each scenario: `copy(window.__wh.jsonl())` in the console, then
   **before** pasting into `captures/<name>.jsonl`, search the copied text
   for the serial number you noted in step 0. If it appears anywhere, do not
   save the file: something is wrong with redaction for this capture (see
   "Redaction" below for what to do). If it does not appear, paste into
   `captures/<name>.jsonl` as usual.
7. Re-run `cargo test -p wh-proto --test golden -- --nocapture`. The
   `--nocapture` matters: without it, cargo swallows the summary on a
   passing run and you see nothing but `ok`. The summary is also written to
   a file under `target/`, whose exact path is printed at the end of the
   run, so it survives even if you forget the flag, but reading it live is
   faster. The golden test itself also hard-fails if any inbound SYNC frame
   in `captures/` is missing `"redacted": true`, as a backstop in case a
   capture was pasted from an older copy of the shim.
8. **If the test fails**, the natural reaction is to assume the capture is
   bad and redo it. For most failures, do not: a red test after a real
   capture usually means the device said something new and true that the
   codec does not yet handle, which is exactly what this exercise exists to
   surface. Keep the capture file as it is and report the failure (the
   summary explains which class it was and why); do not edit, redo, or
   discard it to make the test green. The one exception is a redaction
   failure (see below), which does need a fix before the file is committed,
   not a shrug.

## What a passing (or failing) run tells you

Every captured report must decode: magic, length, checksum. Commands the CLI
does not yet model, and frames with non-zero bytes past their declared
length, are counted and attributed to the scenario file and direction they
came from in the summary, not failed. Feature reports
(`in-feature`/`out-feature`) are also not required to be exactly 64 bytes;
a length mismatch there is reported, not failed, since a feature report has
no obligation to match our fixed HID report size. A genuine hard failure
(bad magic, bad checksum, a declared length the framing cannot represent, or
an inbound SYNC frame missing its redaction stamp) fails the test, but only
after the summary above it has already printed and been written, so one bad
frame in one capture never hides what the other eight found.

## Redaction

`capture/hid-shim.js` zeroes the serial number (report bytes 13..29, the
window `parse_sync` reads as payload bytes 9..25) on any inbound SYNC reply
before it is ever added to the log the shim emits, and marks that entry
`"redacted": true`, visibly rather than silently. This is safe: the
checksum only ever covers the payload's last byte, which sits at index 35 or
beyond for any SYNC reply carrying a firmware string, well past the redacted
window, so a redacted frame still has a valid checksum and still parses.

The redaction is guarded, not blind, because a wrong offset assumption
would otherwise scrub the wrong bytes and leave the real serial untouched,
with no way to tell from the output alone:

- Before scrubbing, the shim checks that the window actually looks like a
  serial (every byte printable ASCII or `0x00`). If it does not, the shim
  **declines to scrub**, prints a console warning, and stamps the entry
  `"redacted": false, "redaction_skipped": "..."` instead of destroying data
  that might not be a serial at all. If you see this, stop and report it;
  do not paste the capture in until someone has confirmed where the serial
  actually is.
- After scrubbing, the shim scans the rest of the frame for any run of six
  or more printable ASCII bytes (excluding the firmware string, which is
  always printable and always there, and is not a leak). If it finds one,
  it prints a console warning and stamps the entry with `"possible_leak":
  "..."`, since that is very likely the real serial sitting somewhere this
  shim did not expect. Treat this exactly like a declined redaction: do not
  commit until it's understood.
- The pre-redaction bytes are kept in a second log the normal `jsonl()`
  never touches: `window.__wh.rawJsonl()`. **Never paste `rawJsonl()` output
  into a committed file.** It exists only so a botched redaction is
  recoverable while you are still at the keyboard, since this capture is
  the one dataset this whole project is read against and there is no second
  chance at a hardware session.

The step 6 search (for the serial printed on the device, in the pasted
text, before saving) is the check that actually discriminates a working
redaction from a broken one; the shim's own flags (`redacted`,
`redaction_skipped`, `possible_leak`) are the shim telling you what it
believes happened; searching for the literal serial confirms it.

## Scenarios

- `initial-load`: just connect, nothing else. What does the device volunteer
  on connect, before we change anything? This is the baseline every other
  capture is effectively diffed against.
- `rt-on-w-0.5`: enable Rapid Trigger on W at 0.5mm. Does enabling RT for a
  single key touch only that key's layout records, or does it also rewrite
  the global mode byte?
- `rt-w-0.6`: change W's already-enabled RT actuation to 0.6mm. Does
  changing a value write only the RT_PRESS/RT_RELEASE layout records, or
  something wider than that?
- `rt-off-w`: turn Rapid Trigger back off for W. Does turning RT off restore
  the plain actuation-point value, or leave it untouched and just flip the
  mode nibble?
- `ap-w-1.2`: set W's actuation point to 1.2mm, outside of RT. Does a plain
  AP write use the same Layout_DB0 path our own AP write already assumes?
- `profile-switch`: switch from profile 1 to profile 2 and back. Does the
  protocol expose or select the active profile at all? The board has four
  profiles and the vendor's own export is labelled per profile, so this
  almost certainly matters and we do not yet model it.
- `rt-continuous-toggle`: toggle CONTINUOUS RAPID TRIGGER only, nothing
  else. What does the mode byte's advanced nibble encode?
- `rt-separate-toggle`: toggle SEPARATE PRESS AND RELEASE only, nothing
  else. Same question as `rt-continuous-toggle`, a different bit.
- `remap-one-key`: remap one key, then re-read the matrix. Does addressing
  keys by HID usage survive a remap?

## One more thing to record yourself

`window.__wh.installedAt` is a wall-clock timestamp (ISO 8601) taken once,
when the shim was installed. It is deliberately not included in `jsonl()`'s
output: every line `jsonl()` produces is a real captured report, and adding
one line that is not would make the golden harness special-case it. If you
want this anchor on record, note it yourself, for example in the PR
description or commit message for the captures, rather than in the capture
file.
