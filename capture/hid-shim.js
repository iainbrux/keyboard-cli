// WebHID logging shim for terminal.wallhack.com.
// Paste into DevTools console BEFORE the page opens the device (i.e. right
// after a hard reload), or inject via CDP Page.addScriptToEvaluateOnNewDocument.
// Dump with: copy(window.__wh.jsonl())
// Start a fresh scenario with: window.__wh.begin("scenario-name")
//
// Redaction: an inbound SYNC reply (cmd 0x01) has its serial number, payload
// bytes 9..25, zeroed before it is ever emitted by jsonl(), since captures
// are committed to a public repository. This scrub is guarded, not blind:
//
//   1. Before scrubbing, the window is checked to actually look like a
//      serial (every byte printable ASCII or 0x00). If it does not, the
//      shim refuses to scrub, warns loudly, and stamps the entry
//      "redacted": false, "redaction_skipped": "..." instead of destroying
//      data that might not be a serial at all.
//   2. After scrubbing, the rest of the frame is scanned for any run of six
//      or more printable ASCII bytes. If found, that is very likely the
//      real serial sitting somewhere other than where this shim expects it
//      (a wrong offset assumption), and it is warned about and stamped on
//      the entry as "possible_leak" rather than silently committed anyway.
//   3. The pre-redaction bytes are kept in a second, separate log that
//      jsonl() never touches: window.__wh.rawJsonl(). It exists purely so a
//      botched redaction is recoverable while still at the keyboard, in the
//      one dataset this whole project is read against. NEVER paste
//      rawJsonl() output into a committed file.
//
// The Rust side additionally hard-fails the whole golden test if any
// inbound SYNC frame in captures/ lacks "redacted": true, which closes the
// hole where a capture was pasted from an older copy of this file.
//
// The prime directive of this shim is that the page behaves exactly as if
// it were not here. Every patched method calls straight through to the
// original, and every logging step is wrapped so that a logging failure
// (for example a detached buffer) can never stop a real write from
// reaching the device or a real read from reaching the page.
(() => {
  const log = [];
  const rawLog = [];

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

  const bytesFromHex = (h) => {
    const out = new Uint8Array(h.length / 2);
    for (let i = 0; i < out.length; i++) {
      out[i] = parseInt(h.slice(i * 2, i * 2 + 2), 16);
    }
    return out;
  };

  const isPrintable = (b) => b >= 0x20 && b <= 0x7e;

  // Every byte in [start, end) is either printable ASCII or 0x00 (the
  // shape a null-padded ASCII serial takes). If anything else is in there,
  // this is not a serial and must not be scrubbed.
  const looksLikeSerial = (bytes, start, end) => {
    for (let i = start; i < end; i++) {
      const b = bytes[i];
      if (!(b === 0x00 || isPrintable(b))) return false;
    }
    return true;
  };

  const MIN_LEAK_RUN = 6;
  // Scans for the first run of MIN_LEAK_RUN or more consecutive printable
  // ASCII bytes anywhere in `bytes`. Used after redaction to check whether
  // something that looks like text, plausibly the real serial at an offset
  // this shim did not expect, is still sitting in the frame.
  const findPrintableRun = (bytes) => {
    let runStart = -1;
    for (let i = 0; i <= bytes.length; i++) {
      const printable = i < bytes.length && isPrintable(bytes[i]);
      if (printable) {
        if (runStart === -1) runStart = i;
        continue;
      }
      if (runStart !== -1) {
        const len = i - runStart;
        if (len >= MIN_LEAK_RUN) {
          return { start: runStart, end: i };
        }
      }
      runStart = -1;
    }
    return null;
  };

  const asciiOf = (bytes, start, end) =>
    Array.from(bytes.slice(start, end))
      .map((b) => String.fromCharCode(b))
      .join("");

  // cmd byte 0x01 is SYNC. A SYNC reply carries the serial number at
  // payload bytes 9..25, which is report bytes 13..29 once the 4-byte
  // header (magic, len, cmd, checksum) is accounted for. Only redact
  // inbound frames: the outgoing SYNC request has the same cmd byte but no
  // serial in it, and redacting it would just be a confusing false
  // positive in the "redacted": true flag.
  const SYNC_CMD = 0x01;
  const SERIAL_BYTE_START = 13;
  const SERIAL_BYTE_END = 29;

  // parse_sync also expects a firmware string at payload bytes 26..36
  // (report bytes 30..40), which is printable ASCII by design and always
  // present on a genuine SYNC reply. Left in the post-redaction leak scan
  // below, it would fire on every single well-formed capture, which is not
  // a leak, it is supposed to be there, and would drown out the one time
  // the scan finds something real. Excluded from the scan for that reason
  // only: it is still emitted in the output hex untouched, same as always.
  const FIRMWARE_BYTE_START = 30;
  const FIRMWARE_BYTE_END = 40;

  // Returns a NEW entry: never mutates its argument, so the caller can keep
  // the original around (see rawLog below).
  const redact = (entry) => {
    const bytes = bytesFromHex(entry.hex);
    const isInbound = entry.dir.startsWith("in");
    const cmdByte = bytes.length > 2 ? bytes[2] : -1;
    if (!isInbound || cmdByte !== SYNC_CMD || bytes.length < SERIAL_BYTE_END) {
      return { ...entry, redacted: false };
    }

    if (!looksLikeSerial(bytes, SERIAL_BYTE_START, SERIAL_BYTE_END)) {
      console.warn(
        `[wh] DECLINED to redact ${entry.dir} report: bytes ${SERIAL_BYTE_START}..` +
          `${SERIAL_BYTE_END} do not look like a serial (not all printable ASCII or 0x00). ` +
          "The serial offset assumption (payload 9..25) may be wrong for this device. This " +
          "frame will NOT be scrubbed. Do not commit it until a human has confirmed no real " +
          "serial number is present anywhere in it (check rawJsonl() and search for the " +
          "serial printed on the device)."
      );
      return {
        ...entry,
        redacted: false,
        redaction_skipped: "window did not look like a serial",
      };
    }

    const redactedBytes = bytes.slice();
    for (let i = SERIAL_BYTE_START; i < SERIAL_BYTE_END; i++) redactedBytes[i] = 0;

    const out = { ...entry, hex: hexOfBytes(redactedBytes), redacted: true };
    const scanBytes = redactedBytes.slice();
    for (let i = FIRMWARE_BYTE_START; i < Math.min(FIRMWARE_BYTE_END, scanBytes.length); i++) {
      scanBytes[i] = 0x00;
    }
    const leak = findPrintableRun(scanBytes);
    if (leak) {
      const ascii = asciiOf(redactedBytes, leak.start, leak.end);
      console.warn(
        `[wh] redacted bytes ${SERIAL_BYTE_START}..${SERIAL_BYTE_END}, but bytes ` +
          `${leak.start}..${leak.end} STILL look like text after redaction: "${ascii}". ` +
          "This may be the real serial sitting at a different offset than expected. Do not " +
          "commit this capture without checking (rawJsonl() has the unredacted original)."
      );
      out.possible_leak = `bytes ${leak.start}..${leak.end}: "${ascii}"`;
    }
    return out;
  };

  // Records one entry: the raw (never redacted, never emitted by jsonl())
  // copy goes to rawLog, the redacted copy goes to log. Wrapped by every
  // call site in try/catch, since hex(data) can throw (a detached buffer,
  // null) and a logging failure must never stop the real HID call.
  const recordEntry = (fields) => {
    const raw = { ts: performance.now(), ...fields };
    rawLog.push(raw);
    log.push(redact(raw));
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
    jsonl: () => log.map((e) => JSON.stringify(e)).join("\n"),
    // The pre-redaction log. NEVER paste this into a file under captures/;
    // it exists only to recover from a declined or wrong redaction while
    // still at the keyboard.
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
    // of it in the data.
    begin: (name) => {
      log.length = 0;
      rawLog.length = 0;
      console.log(`[wh] capture started: ${name} (log cleared)`);
    },
  };
  console.log("[wh] HID shim installed");
})();
