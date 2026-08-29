# Capturing golden traffic

This is the procedure for recording real HID traffic between the vendor web
configurator and a Wallhack K-001 keyboard, so `cargo test -p wh-proto --test
golden` has something real to check the codec against. It has been run once
already, task 19's hardware session: ten scenarios, 1224 frames, zero framing
or checksum failures, recorded in `docs/protocol-inventory.md` and
`docs/protocol.md`. This document describes the procedure so it can be run
again, for a firmware update, a second board, or a new scenario, not as a
first-time exercise still to be proven out.

Captures are **not** committed to this repository: `captures/` is
gitignored. They are the operator's own device traffic and stay on their
machine; the golden test runs locally against them during a hardware
session. CI exercises the same harness through synthetic fixtures built
directly in `crates/wh-proto/tests/golden.rs`, so a missing `captures/`
directory is the normal state everywhere except your own machine during a
capture session, not a coverage gap. If you want your captures to survive
past this machine, back them up somewhere private yourself; nothing in this
repository will do that for you.

Before you start, note which profile is active on the keyboard, somewhere
outside this repository (your own notes, not a comment in the capture
file). Nothing in the capture itself records which profile was active, and
it may matter when the captures are read back later.

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
6. After each scenario: `copy(window.__wh.jsonl())` in the console, paste
   into `captures/<name>.jsonl`.
7. Re-run `cargo test -p wh-proto --test golden -- --nocapture`. The
   `--nocapture` matters: without it, cargo swallows the summary on a
   passing run and you see nothing but `ok`. The summary is also written to
   a file under `target/`, whose exact path is printed at the end of the
   run, so it survives even if you forget the flag, but reading it live is
   faster.
8. **If the test fails**, the natural reaction is to assume the capture is
   bad and redo it. Do not. A red test after a real capture usually means
   the device said something new and true that the codec does not yet
   handle, which is exactly what this exercise exists to surface. Keep the
   capture file as it is and report the failure (the summary explains which
   class it was and why); do not edit, redo, or discard it to make the test
   green.

## What a passing (or failing) run tells you

Every captured report must decode: magic, length, checksum. Commands the CLI
does not yet model, and frames with non-zero bytes past their declared
length, are counted and attributed to the scenario file and direction they
came from in the summary, not failed. Feature reports
(`in-feature`/`out-feature`) are also not required to be exactly 64 bytes;
a length mismatch there is reported, not failed, but an INBOUND one still
gets a `WARNING:` callout, since it may be the report-ID-prefix behaviour
described in step 3. A genuine hard failure (bad magic, bad checksum, or a
declared length the framing cannot represent) fails the test, but only
after the summary above it has already printed and been written, so one bad
frame in one capture never hides what the other nine found (the ten
scenarios below, task 19's session).

## Scenarios

The ten below are what task 19's session actually captured, not a proposal still to be run; the
original plan for this session named two scenarios, `rt-w-0.6` and `ap-w-1.2`, that were never
captured under those names (see below for what replaced them). Reproducing this list on a future
session, a firmware update, or a second board, does not require using exactly these names, but
each scenario should still change exactly one thing, the whole method this procedure exists for.

- `initial-load`: just connect, nothing else. What does the device volunteer
  on connect, before we change anything? This is the baseline every other
  capture is effectively diffed against.
- `rt-on-w-0.5`: enable Rapid Trigger on W at 0.5mm. Does enabling RT for a
  single key touch only that key's layout records, or does it also rewrite
  the global mode byte?
- `rt-continuous-toggle`: toggle CONTINUOUS RAPID TRIGGER only, nothing
  else. What does the mode byte's advanced nibble encode?
- `rt-separate-toggle`: toggle SEPARATE PRESS AND RELEASE only, nothing
  else. Same question as `rt-continuous-toggle`, a different bit.
- `rt-off-w`: turn Rapid Trigger back off for W. Does turning RT off restore
  the plain actuation-point value, or leave it untouched and just flip the
  mode nibble? (Answered: neither, it does something worse. The vendor
  writes touch nibble 1, not 0, when disabling RT on a key with its own
  actuation point; a naive nibble-0 write would have discarded it. This is
  the origin of the data-loss bug task 19b chunk 3 fixed.)
- `profile-switch`: switch from profile 1 to profile 2 and back. Does the
  protocol expose or select the active profile at all? Answered,
  favourably: yes, `cmd 0x00` sub-order `0x70` both reads and selects it.
- `ap-wasd-1.2`: set the actuation point for the whole WASD keyset to
  1.2mm, deliberately not the single-key `ap-w-1.2` the original plan
  named, since a multi-key write exercises the batching and per-key
  grouping a single-key capture cannot. Does a plain AP write use the same
  layout `0x04` path, and how does the vendor batch several keys' records
  into one report?
- `remap-one-key`: remap one key, then re-read its own layout `0x00`
  record. Confirms layout `0x00` is the live key mapping, not a fixed
  identifier: the remapped key's `0x00` value changed to match.
- `remap-matrix-read`: re-read the DEFKEY matrix (`cmd 0x2b`) with that
  same remap still live. Confirms the opposite for DEFKEY: it kept
  reporting the key's original, physical usage, not the new mapping, which
  is why `wh` can address a key by its DEFKEY-reported usage even after an
  operator remaps it.
- `nav-key-identify`: remap each of the four non-standard keys (`0xfa`,
  `0xfb`, `0xd6`, `0xfc`) to a distinct, recognisable key (F2 through F5)
  in turn and capture the write. Settles which physical key is which by
  measurement (the write names both the usage and the label clicked)
  rather than by counting rows in a photo, which an earlier pass got
  wrong.

## One more thing to record yourself

`window.__wh.installedAt` is a wall-clock timestamp (ISO 8601) taken once,
when the shim was installed. It is deliberately not included in `jsonl()`'s
output: every line `jsonl()` produces is a real captured report, and adding
one line that is not would make the golden harness special-case it. If you
want this anchor on record, note it yourself, for example in the PR
description or commit message for the captures, rather than in the capture
file.
