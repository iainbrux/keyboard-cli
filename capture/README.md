# Capturing golden traffic

This is the procedure for recording real HID traffic between the vendor web
configurator and a Wallhack K-001 keyboard, so `cargo test -p wh-proto --test
golden` has something real to check the codec against. Nothing in this repo
has ever been checked against a byte the real device actually sent; this is
how we get that.

Before you start, note which profile is active on the keyboard. Nothing in
the capture itself records that, and it may matter when the captures are
read back later.

1. Close any running `wh` process. Open Chrome on Windows.
2. Open DevTools (F12) on a blank tab, console tab. Navigate to
   https://terminal.wallhack.com/ but paste `capture/hid-shim.js` into the
   console the moment the page starts loading (or use
   `--remote-debugging-port=9222` with
   `Page.addScriptToEvaluateOnNewDocument`), so the shim is installed before
   the page ever opens the device.
3. Connect the keyboard in the web UI, then work through the scenarios below
   in order. Perform ONE single-variable change per capture. That is what
   makes a diff between two captures readable, and it is the whole method:
   resist the urge to combine two changes into one capture even when it
   would be faster at the keyboard.
4. After each scenario: `copy(window.__wh.jsonl())` in the console, paste
   into `captures/<name>.jsonl`, then `window.__wh.clear()` before starting
   the next scenario.
5. Re-run `cargo test -p wh-proto --test golden`. Every capture must decode
   (magic, length, checksum). Commands the CLI does not yet model, and
   frames with non-zero bytes past their declared length, are reported in
   the test output rather than failing it; a genuine codec bug (bad magic,
   bad checksum, or a length the framing cannot represent) fails the test.

## Scenarios

- `initial-load`: just connect. Full config read, the baseline every other
  capture is read against.
- `rt-on-w-0.5`: enable Rapid Trigger on W at 0.5mm.
- `rt-w-0.6`: change W's Rapid Trigger actuation to 0.6mm.
- `rt-off-w`: turn Rapid Trigger back off for W.
- `ap-w-1.2`: set W's actuation point to 1.2mm.
- `profile-switch`: switch from profile 1 to profile 2 and back. Does the
  protocol expose or select the active profile? The board has four profiles
  and the vendor's own export is labelled per profile, so this almost
  certainly matters and we do not yet model it.
- `rt-continuous-toggle`: toggle CONTINUOUS RAPID TRIGGER only, nothing else.
  What does the mode byte's advanced nibble encode?
- `rt-separate-toggle`: toggle SEPARATE PRESS AND RELEASE only, nothing
  else. Same question as above, a different bit.
- `remap-one-key`: remap one key, then re-read the matrix. Does addressing
  keys by HID usage survive a remap?
