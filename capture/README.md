# Capturing golden traffic

This is the procedure for recording real HID traffic between the vendor web
configurator and a Wallhack K-001 keyboard, so `cargo test -p wh-proto --test
golden` has something real to check the codec against. Nothing in this repo
has ever been checked against a byte the real device actually sent; this is
how we get that.

Captures are committed to this repository, with the device serial number
redacted automatically by the shim (see "Redaction" below). Do not hand-edit
the hex to redact anything yourself.

Before you start:

- Note which profile is active on the keyboard, somewhere outside this
  repository (your own notes, not a comment in the capture file). Nothing in
  the capture itself records which profile was active, and it may matter
  when the captures are read back later.
- Read the device's serial number off the vendor configurator's Device tab.
  You will give this value to the shim in step 2, and put a copy of it in
  `capture/serial.local` (step 1) so the test suite can independently check
  its own work. Do not skip either.

1. Copy the serial into `capture/serial.local`, one line, plain text (for
   example `echo "3483141393E03502" > capture/serial.local` from a shell on
   this machine). This file is gitignored; it never leaves your machine. If
   it is absent, `cargo test -p wh-proto --test golden` still runs, but
   prints a loud warning that the one check standing between a real serial
   number and a public commit was **skipped**, not passed. Do not treat that
   warning's absence, i.e. the file not existing, as "nothing to worry
   about": create the file.
2. Close any running `wh` process. Open Chrome on Windows.
3. Open DevTools (F12) on a blank tab, console tab. Navigate to
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

   Immediately after the shim installs, before connecting the device, run:

   ```js
   window.__wh.protect("3483141393E03502") // the same value as capture/serial.local
   ```

   `jsonl()` refuses to emit anything until this has been called; there is
   no "forgot to protect" failure mode, only a hard stop. If you connect the
   device and start capturing before calling `protect()`, that is not lost:
   calling `protect()` later re-scrubs everything already logged, but call
   it first anyway, out of habit, so you never have to rely on that.
4. Connect the keyboard in the web UI. Before your first real scenario,
   trigger a feature-report exchange once if the UI has one (switch
   calibration and switch-type selection, per the configurator's own
   screenshots, look like `sendFeatureReport`/`receiveFeatureReport`
   candidates), then run:

   ```js
   window.__wh.log.filter(e => e.dir.endsWith("-feature"))
   ```

   in the console, **not** `window.__wh.log[window.__wh.log.length - 1]`:
   switch calibration also drives ordinary `sendReport`/`inputreport`
   traffic, so the last entry logged is almost always a plain `"in"` line,
   not the feature-report one you meant to inspect, and reading it would
   confidently answer the wrong question. Check the filtered entries'
   `hex.length`: 130 (65 bytes) instead of 128 (64 bytes) means Chrome is
   including the report ID as the first byte of that feature report's data,
   unlike a normal input report, which the WebHID specification allows
   ("receiveFeatureReport() ... may include the report ID as the first byte
   ... if the device uses report IDs"). Note the answer in whatever you use
   to record the session (a PR description is fine) so a reviewer reading
   `in-feature`/`out-feature` lines later knows whether byte 0 is real
   payload or a report ID. `window.__wh.clear()` afterward to drop this
   probe before your first real scenario.
5. Work through the scenarios below in order. Perform ONE single-variable
   change per capture. That is what makes a diff between two captures
   readable, and it is the whole method: resist the urge to combine two
   changes into one capture even when it would be faster at the keyboard.
6. Before each scenario, run `window.__wh.begin("scenario-name")` in the
   console instead of `window.__wh.clear()`. It does the same thing (and
   does not un-protect the serial: `protect()` persists across `begin()`),
   but it also prints `[wh] capture started: scenario-name` so a forgotten
   clear between two scenarios is visible immediately, at the keyboard,
   rather than discovered later as two scenarios silently merged into one
   capture file with no signal that it happened.
7. After each scenario: `copy(window.__wh.jsonl())` in the console, then
   **before** pasting into `captures/<name>.jsonl`, search the copied text
   for the **lowercase hex encoding** of the serial you noted before
   starting, not the serial's own characters: the capture is hex-encoded
   text, so searching for the literal serial string can never match it. To
   produce the needle to search for, in the same console:

   ```js
   [...serial].map(c => c.charCodeAt(0).toString(16).padStart(2, "0")).join("")
   // with `serial` replaced by your actual serial string, in quotes
   ```

   If that string appears anywhere in the copied text, do not save the
   file: something is wrong (see "Redaction" below). If it does not appear,
   paste into `captures/<name>.jsonl` as usual. This search is a
   **confirmation** that `protect()` already did its job, not the only line
   of defence: `find_serial_leaks` in `golden.rs` runs the identical search
   independently in step 8, over `capture/serial.local`, so this step is
   catching a problem before a test run has to.
8. Re-run `cargo test -p wh-proto --test golden -- --nocapture`. The
   `--nocapture` matters: without it, cargo swallows the summary on a
   passing run and you see nothing but `ok`. The summary is also written to
   a file under `target/`, whose exact path is printed at the end of the
   run, so it survives even if you forget the flag, but reading it live is
   faster. With `capture/serial.local` present, the golden test
   independently re-scans every capture for the serial's hex encoding and
   hard-fails on any hit, anywhere, regardless of which frame or field it is
   in.
9. **If the test fails**, the natural reaction is to assume the capture is
   bad and redo it. For most failures, do not: a red test after a real
   capture usually means the device said something new and true that the
   codec does not yet handle, which is exactly what this exercise exists to
   surface. Keep the capture file as it is and report the failure (the
   summary explains which class it was and why); do not edit, redo, or
   discard it to make the test green. The one exception is a serial-leak
   failure, which does need a fix before the file is committed, not a
   shrug: if it happens, `protect()` was called with the wrong value, or
   called too late and `begin()`/`clear()` erased the pre-`protect()`
   entries before they could be re-scrubbed (`begin()`/`clear()` themselves
   are safe once `protect()` has already run), or the raw capture in
   `window.__wh.rawJsonl()` needs inspecting to see what actually happened.

## What a passing (or failing) run tells you

Every captured report must decode: magic, length, checksum. Commands the CLI
does not yet model, and frames with non-zero bytes past their declared
length, are counted and attributed to the scenario file and direction they
came from in the summary, not failed. Feature reports
(`in-feature`/`out-feature`) are also not required to be exactly 64 bytes;
a length mismatch there is reported, not failed, since redaction (see below)
does not depend on frame shape any more, but an INBOUND one still gets a
`WARNING:` callout, since it may be the report-ID-prefix behaviour described
in step 4. A genuine hard failure (bad magic, bad checksum, a declared
length the framing cannot represent, or a capture containing the protected
serial's hex encoding anywhere) fails the test, but only after the summary
above it has already printed and been written, so one bad frame in one
capture never hides what the other eight found.

## Redaction

Every earlier version of this shim's redaction guessed WHERE the serial
number would be: a fixed byte offset, a particular command byte, a
direction. Every defect found across multiple rounds of review traced back
to that one root cause. This version does not guess a location. It knows
the VALUE, because you gave it one with `protect()`, and matches on that
instead:

- `window.__wh.protect(serial)` tells the shim the serial. From that
  moment, every logged entry (current and future) has the serial's
  ASCII-bytes hex encoding replaced with zeros wherever it appears in the
  report's hex text: an exact, case-insensitive substring match,
  independent of which command carries it, which direction it came from,
  or how long the frame is. Each entry is stamped `redactions: N`, a count
  of how many times the substring was found and replaced in that entry, not
  a boolean: a boolean would be an assertion the shim cannot back ("I found
  everything"), a count is just what happened.
- `jsonl()` refuses to emit anything until `protect()` has been called.
  There is no "forgot to protect" failure mode.
- The pre-redaction bytes are kept in a second log the normal `jsonl()`
  never touches: `window.__wh.rawJsonl()`. **Never paste `rawJsonl()`
  output into a committed file.** It exists only so a botched capture is
  recoverable while you are still at the keyboard, since this capture is
  the one dataset this whole project is read against and there is no
  second chance at a hardware session.

This is exact and shape-independent: it does not matter which command
carries the serial, whether the frame is a feature report, or whether a
report-ID byte shifts everything by one, because none of that changes what
the serial's own bytes look like once hex-encoded. Its one disclosed limit:
it can only catch the serial in the ASCII-hex encoding `protect()` was told
about. If the device also emits it BCD-packed, byte-reversed, or in some
other encoding, this will not catch that occurrence. Nothing in any earlier
version of this shim could catch that case either, and would additionally
have scrubbed the wrong bytes while claiming success.

`golden.rs`'s `find_serial_leaks` re-verifies this independently: given
`capture/serial.local`, it scans the raw text of every `captures/*.jsonl`
file, byte for byte, for the same ASCII-hex needle, and hard-fails on any
hit, anywhere. It does not trust the shim's own `redactions` count; it
confirms the one fact that actually matters directly. This is the strongest
automated check in this project, and it is not optional: if
`capture/serial.local` is missing, the test still passes, but the summary
says, loudly, that the check was skipped, not that it found nothing.

The step 7 hex search you do yourself, by hand, before pasting a capture
in, is a confirmation that `protect()` already did its job, the right
relationship between the two: `find_serial_leaks` is the actual gate, the
manual search catches a problem before you even reach the test.

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
