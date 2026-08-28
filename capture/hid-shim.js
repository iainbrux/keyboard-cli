// WebHID logging shim for terminal.wallhack.com.
// Paste into DevTools console BEFORE the page opens the device (i.e. right
// after a hard reload), or inject via CDP Page.addScriptToEvaluateOnNewDocument.
//
// BEFORE capturing anything, tell the shim the device's serial number, read
// off the vendor configurator's Device tab:
//   window.__wh.protect("3483141393E03502")
// Dump with: copy(window.__wh.jsonl())
// Start a fresh scenario with: window.__wh.begin("scenario-name")
//
// Redaction: every prior version of this shim guessed WHERE the serial
// number would be (a fixed byte offset, a command byte, a direction) and
// scrubbed that location. Every defect found across three rounds of review
// traced back to that one root cause: guessing the location. This version
// does not guess. The operator supplies the serial's VALUE via protect();
// every logged entry then has the serial's hex encoding replaced with
// zeros, an exact case-insensitive substring match over the report's hex
// text, wherever it appears, regardless of which command carries it,
// which direction it came from, or whether the frame is 64 bytes.
//
//   - `jsonl()` throws if `protect()` was never called. There is no
//     "forgot to protect" failure mode: it is a hard stop, not a
//     best-effort default.
//   - Each entry is stamped `redactions: N`, a count, not a boolean: a
//     boolean would be an assertion this shim cannot back ("I found and
//     removed everything"); a count is just what happened.
//   - Calling `protect()` re-scrubs every entry already logged, not just
//     future ones, in case the device connected before `protect()` was
//     called.
//   - The pre-redaction bytes are kept in a second, separate log that
//     `jsonl()` never touches: `window.__wh.rawJsonl()`. It exists purely
//     so a botched capture is recoverable while still at the keyboard, in
//     the one dataset this whole project is read against. NEVER paste
//     `rawJsonl()` output into a committed file.
//
// This is exact and shape-independent: it does not matter which command
// carries the serial, whether the frame is a feature report, or whether a
// report-ID byte shifts everything by one, because none of that changes
// what the serial's own bytes look like once hex-encoded. Its one known
// limit: it can only catch the serial in the encoding protect() was told
// about (its ASCII bytes, hex-encoded). If the device also emits it BCD-
// packed, byte-reversed, or in some other encoding, this will not catch
// that occurrence either. Nothing in any earlier version of this shim
// could catch that case either, and would additionally have scrubbed the
// wrong bytes while claiming success. The Rust-side check in golden.rs
// (`find_serial_leaks`) re-verifies the ASCII-hex occurrence independently
// before anything is allowed to pass; see that file's module doc comment.
//
// The prime directive of this shim is that the page behaves exactly as if
// it were not here. Every patched method calls straight through to the
// original, and every logging step is wrapped so that a logging failure
// (for example a detached buffer) can never stop a real write from
// reaching the device or a real read from reaching the page.
(() => {
  const log = [];
  const rawLog = [];
  let serialHex = null; // lowercase hex encoding of the protected serial's ASCII bytes, or null

  // `sendReport`'s `data` and `inputreport`'s `event.data` are both a
  // BufferSource: a DataView, a typed array, or a plain ArrayBuffer.
  // `ArrayBuffer.isView` is the correct discriminator: unlike
  // `instanceof ArrayBuffer`, it also works across realms (for example a
  // buffer that originated in an iframe), where `instanceof` silently fails
  // and used to make this function log nothing at all.
  //
  // For a view, `.buffer` is the WHOLE underlying ArrayBuffer, not the
  // view's own window into it, so `new Uint8Array(buf.buffer)` alone would
  // silently log the wrong bytes whenever the view has a non-zero
  // byteOffset or a byteLength shorter than the buffer. Read the view's own
  // offset and length explicitly instead.
  const bytesOf = (buf) =>
    ArrayBuffer.isView(buf)
      ? new Uint8Array(buf.buffer, buf.byteOffset, buf.byteLength)
      : new Uint8Array(buf);

  const hexOfBytes = (bytes) =>
    [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("");

  const hex = (buf) => hexOfBytes(bytesOf(buf));

  // The ASCII-bytes hex encoding of a plain string, e.g. "AB" -> "4142".
  // This is what a device would produce if it stored the serial as ASCII
  // text in a report, which is the assumption every earlier version of
  // this shim also made; the difference now is that this is the thing we
  // search FOR, not a location we scrub blind.
  const hexOfAsciiString = (s) => {
    let out = "";
    for (let i = 0; i < s.length; i++) {
      out += s.charCodeAt(i).toString(16).padStart(2, "0");
    }
    return out;
  };

  // Replaces every case-insensitive occurrence of `serialHex` in `hex` with
  // an equal-length run of zero bytes, non-overlapping, left to right.
  // Returns the redacted hex and how many occurrences were found. If
  // `protect()` has not been called yet, this is a no-op: `jsonl()`
  // refuses to emit anything in that state (see below), so a no-op here
  // cannot itself leak anything, but rebuildLog() still needs a well-formed
  // return value to work with before protect() has ever run.
  const redactSerialFromHex = (h) => {
    if (!serialHex) return { hex: h, redactions: 0 };
    const lower = h.toLowerCase();
    let redactions = 0;
    let result = "";
    let from = 0;
    for (;;) {
      const pos = lower.indexOf(serialHex, from);
      if (pos === -1) {
        result += h.slice(from);
        break;
      }
      result += h.slice(from, pos) + "00".repeat(serialHex.length / 2);
      redactions++;
      from = pos + serialHex.length;
    }
    return { hex: result, redactions };
  };

  // Returns a NEW entry: never mutates its argument, so `rawLog` and `log`
  // can each hold their own independent copy of the same underlying event.
  const redactEntry = (raw) => {
    const { hex: redactedHex, redactions } = redactSerialFromHex(raw.hex);
    return { ...raw, hex: redactedHex, redactions };
  };

  // Re-derives every entry in `log` from `rawLog`, in place (same array
  // reference, since `window.__wh.log` was already handed out). Called by
  // `protect()` so that any report captured BEFORE `protect()` was called
  // still gets scrubbed once the serial is known, rather than sitting in
  // `log` forever in its unredacted form.
  const rebuildLog = () => {
    log.length = 0;
    for (const raw of rawLog) log.push(redactEntry(raw));
  };

  // Records one entry: the raw (never redacted, never emitted by jsonl())
  // copy goes to rawLog, the redacted copy goes to log. Wrapped by every
  // call site in try/catch, since hex(data) can throw (a detached buffer,
  // null) and a logging failure must never stop the real HID call.
  const recordEntry = (fields) => {
    const raw = { ts: performance.now(), ...fields };
    rawLog.push(raw);
    log.push(redactEntry(raw));
  };

  const origSend = HIDDevice.prototype.sendReport;
  HIDDevice.prototype.sendReport = function (reportId, data) {
    try {
      // report_id is recorded, not dropped: we believe it is always 0 and
      // hid.rs prepends 0 on every write, but this capture is the one
      // chance to confirm that against the vendor's own traffic.
      recordEntry({ dir: "out", report_id: reportId, hex: hex(data) });
    } catch (err) {
      console.warn("[wh] failed to log an outgoing report, page traffic is unaffected", err);
    }
    return origSend.call(this, reportId, data);
  };

  const origSendFeature = HIDDevice.prototype.sendFeatureReport;
  HIDDevice.prototype.sendFeatureReport = function (reportId, data) {
    try {
      recordEntry({ dir: "out-feature", report_id: reportId, hex: hex(data) });
    } catch (err) {
      console.warn("[wh] failed to log an outgoing feature report, page traffic is unaffected", err);
    }
    return origSendFeature.call(this, reportId, data);
  };

  const origReceiveFeature = HIDDevice.prototype.receiveFeatureReport;
  HIDDevice.prototype.receiveFeatureReport = function (reportId) {
    return origReceiveFeature.call(this, reportId).then((data) => {
      try {
        recordEntry({ dir: "in-feature", report_id: reportId, hex: hex(data) });
      } catch (err) {
        console.warn("[wh] failed to log an incoming feature report, page traffic is unaffected", err);
      }
      return data;
    });
  };

  // A device that is opened twice, or that reconnects mid-session, must not
  // get a second "inputreport" listener attached: that would log every
  // inbound report twice. A genuine reconnect hands the page a new
  // HIDDevice instance, so a WeakSet keyed on the instance, not a boolean
  // flag on the prototype, is the right guard: it still lets a real
  // reconnect's new device get its own listener.
  const listenedDevices = new WeakSet();
  const origOpen = HIDDevice.prototype.open;
  HIDDevice.prototype.open = function () {
    if (!listenedDevices.has(this)) {
      listenedDevices.add(this);
      this.addEventListener("inputreport", (e) => {
        try {
          recordEntry({ dir: "in", report_id: e.reportId, hex: hex(e.data) });
        } catch (err) {
          console.warn("[wh] failed to log an inbound report, page traffic is unaffected", err);
        }
      });
    }
    return origOpen.call(this);
  };

  window.__wh = {
    log,
    rawLog,
    // performance.now() is monotonic within a page load, so gaps between
    // entries are meaningful (a gap is how you tell a device-initiated
    // report apart from a reply to something we sent); Date.now() is not
    // monotonic and a clock adjustment could mislead that reading.
    // installedAt is the one wall-clock anchor, taken once, at shim
    // install. It is not emitted by jsonl(): every jsonl() line must stay a
    // real report, so record installedAt yourself, e.g. as a note when you
    // save the capture file.
    installedAt: new Date().toISOString(),
    // Tells the shim the device's serial number (read off the Device tab),
    // so every currently-logged and every future entry gets it scrubbed.
    // Safe to call more than once, e.g. if you mistype it the first time:
    // each call re-derives `log` from `rawLog` from scratch with whatever
    // serial was passed most recently.
    protect: (serial) => {
      if (typeof serial !== "string" || serial.length === 0) {
        throw new Error(
          '[wh] protect(serial) needs a non-empty string, e.g. window.__wh.protect("3483141393E03502")'
        );
      }
      serialHex = hexOfAsciiString(serial).toLowerCase();
      rebuildLog();
      const hit = log.filter((e) => e.redactions > 0).length;
      console.log(
        `[wh] serial protected (${serial.length} char(s), ${serialHex.length}-char hex needle). ` +
          `Re-scrubbed ${log.length} existing entr${log.length === 1 ? "y" : "ies"}, ` +
          `${hit} of which contained it.`
      );
    },
    // Refuses to emit anything until protect() has been called: there is
    // no "forgot to protect" failure mode, only a hard stop.
    jsonl: () => {
      if (serialHex === null) {
        throw new Error(
          '[wh] jsonl() refused: call window.__wh.protect("<serial>") first, with the serial ' +
            "printed on the Device tab. Pasting an unprotected capture is the one mistake this " +
            "project cannot recover from."
        );
      }
      return log.map((e) => JSON.stringify(e)).join("\n");
    },
    // The pre-redaction log. NEVER paste this into a file under captures/;
    // it exists only to recover from a botched capture while still at the
    // keyboard.
    rawJsonl: () => rawLog.map((e) => JSON.stringify(e)).join("\n"),
    clear: () => {
      log.length = 0;
      rawLog.length = 0;
    },
    // Clears both logs and announces the scenario name in the console, so a
    // forgotten `clear()` between two scenarios is visible at the moment it
    // would happen rather than as a silently merged capture file discovered
    // later. Deliberately does not add a marker line to `log`: every line
    // `jsonl()` produces must be a real 64-byte report, or the golden
    // harness on the Rust side would have to special-case a non-report
    // line. The filename the operator saves to already carries the
    // scenario name; this is the at-the-keyboard signal, not a second copy
    // of it in the data. Does NOT clear the protected serial: it does not
    // change between scenarios in the same session.
    begin: (name) => {
      log.length = 0;
      rawLog.length = 0;
      console.log(`[wh] capture started: ${name} (log cleared)`);
    },
  };
  console.log("[wh] HID shim installed");
})();
