# Capturing golden traffic

This is the procedure for recording real HID traffic between the vendor web
configurator and a Wallhack K-001 keyboard, so `cargo test -p wh-proto --test
golden` has something real to check the codec against. Nothing in this repo
has ever been checked against a byte the real device actually sent; this is
how we get that.

Captures are committed to this repository, with the device serial number
redacted automatically by the shim (see "Redaction" below). Do not hand-edit
the hex to redact anything yourselves; if a redaction ever looks wrong,
re-capture rather than patching the file by hand.

Before you start, note which profile is active on the keyboard. Nothing in
the capture itself records that, and it may matter when the captures are
read back later.

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
3. Connect the keyboard in the web UI, then work through the scenarios below
   in order. Perform ONE single-variable change per capture. That is what
   makes a diff between two captures readable, and it is the whole method:
   resist the urge to combine two changes into one capture even when it
   would be faster at the keyboard.
4. Before each scenario, run `window.__wh.begin("scenario-name")` in the
   console instead of `window.__wh.clear()`. It does the same thing, but it
   also prints `[wh] capture started: scenario-name` so a forgotten clear
   between two scenarios is visible immediately, at the keyboard, rather
   than discovered later as two scenarios silently merged into one capture
   file with no signal that it happened.
5. After each scenario: `copy(window.__wh.jsonl())` in the console, paste
   into `captures/<name>.jsonl`.
6. Re-run `cargo test -p wh-proto --test golden -- --nocapture`. The
   `--nocapture` matters: without it, cargo swallows the summary on a
   passing run and you see nothing but `ok`. The summary is also written to
   `captures/golden-summary.txt` on every run, so it survives even if you
   forget the flag, but reading it live is faster.
7. **If the test fails**, the natural reaction is to assume the capture is
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
came from in the summary, not failed. A genuine hard failure (bad magic, bad
checksum, or a declared length the framing cannot represent) fails the test,
but only after the summary above it has already printed and been written, so
one bad frame in one capture never hides what the other eight found.

## Redaction

`capture/hid-shim.js` zeroes the serial number (report bytes 13..29) on any
inbound SYNC reply before it is ever added to the log, and marks that entry
`"redacted": true` in the JSONL, visibly rather than silently. This is safe:
the checksum only ever covers the payload's last byte, which sits at index
35 or beyond for any SYNC reply carrying a firmware string, well past the
redacted window, so a redacted frame still has a valid checksum and still
parses. Before pasting a capture into `captures/`, you can sanity check this
yourself by searching the pasted text for `"redacted":true` on the
`initial-load` capture (the one most likely to contain a SYNC reply) and
confirming the hex around bytes 13..29 (characters 26..58 of the `hex`
field) reads as sixteen `00` pairs.

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
