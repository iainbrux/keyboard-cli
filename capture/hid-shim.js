// WebHID logging shim for terminal.wallhack.com.
// Paste into DevTools console BEFORE the page opens the device (i.e. right
// after a hard reload), or inject via CDP Page.addScriptToEvaluateOnNewDocument.
// Dump with: copy(window.__wh.jsonl())
// Start a fresh scenario with: window.__wh.begin("scenario-name")
//
// Redaction: a SYNC reply (cmd 0x01, arriving on any inbound direction)
// has its serial number, payload bytes 9..25, zeroed before it is ever
// logged, since captures are committed to a public repository. This is safe
// because the checksum only covers the payload's LAST byte, which
// parse_sync's own contract puts at index 35 or beyond, well past the
// serial window. Redacted entries carry "redacted": true, visibly, not
// silently.
//
// The prime directive of this shim is that the page behaves exactly as if
// it were not here. Every patched method calls straight through to the
// original, and every logging step is wrapped so that a logging failure
// (for example a detached buffer) can never stop a real write from
// reaching the device or a real read from reaching the page.
(() => {
  const log = [];

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
  const hex = (buf) => {
    const view = ArrayBuffer.isView(buf)
      ? new Uint8Array(buf.buffer, buf.byteOffset, buf.byteLength)
      : new Uint8Array(buf);
    return [...view].map((b) => b.toString(16).padStart(2, "0")).join("");
  };

  // cmd byte 0x01 is SYNC. A SYNC reply carries the serial number at
  // payload bytes 9..25, which is report bytes 13..29 once the 4-byte
  // header (magic, len, cmd, checksum) is accounted for. Only redact
  // inbound frames: the outgoing SYNC request has the same cmd byte but no
  // serial in it, and redacting it would just be a confusing false
  // positive in the "redacted": true flag.
  const SYNC_CMD = 0x01;
  const SERIAL_BYTE_START = 13;
  const SERIAL_BYTE_END = 29;
  const redact = (entry) => {
    const h = entry.hex;
    const cmdByte = h.length >= 6 ? parseInt(h.slice(4, 6), 16) : NaN;
    const isInbound = entry.dir.startsWith("in");
    if (!isInbound || cmdByte !== SYNC_CMD || h.length < SERIAL_BYTE_END * 2) {
      return { ...entry, redacted: false };
    }
    const zeros = "00".repeat(SERIAL_BYTE_END - SERIAL_BYTE_START);
    return {
      ...entry,
      hex: h.slice(0, SERIAL_BYTE_START * 2) + zeros + h.slice(SERIAL_BYTE_END * 2),
      redacted: true,
    };
  };

  const origSend = HIDDevice.prototype.sendReport;
  HIDDevice.prototype.sendReport = function (reportId, data) {
    try {
      // report_id is recorded, not dropped: we believe it is always 0 and
      // hid.rs prepends 0 on every write, but this capture is the one
      // chance to confirm that against the vendor's own traffic.
      log.push(
        redact({ ts: performance.now(), dir: "out", report_id: reportId, hex: hex(data) })
      );
    } catch (err) {
      console.warn("[wh] failed to log an outgoing report, page traffic is unaffected", err);
    }
    return origSend.call(this, reportId, data);
  };

  const origSendFeature = HIDDevice.prototype.sendFeatureReport;
  HIDDevice.prototype.sendFeatureReport = function (reportId, data) {
    try {
      log.push(
        redact({
          ts: performance.now(),
          dir: "out-feature",
          report_id: reportId,
          hex: hex(data),
        })
      );
    } catch (err) {
      console.warn("[wh] failed to log an outgoing feature report, page traffic is unaffected", err);
    }
    return origSendFeature.call(this, reportId, data);
  };

  const origReceiveFeature = HIDDevice.prototype.receiveFeatureReport;
  HIDDevice.prototype.receiveFeatureReport = function (reportId) {
    return origReceiveFeature.call(this, reportId).then((data) => {
      try {
        log.push(
          redact({
            ts: performance.now(),
            dir: "in-feature",
            report_id: reportId,
            hex: hex(data),
          })
        );
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
          log.push(
            redact({ ts: performance.now(), dir: "in", report_id: e.reportId, hex: hex(e.data) })
          );
        } catch (err) {
          console.warn("[wh] failed to log an inbound report, page traffic is unaffected", err);
        }
      });
    }
    return origOpen.call(this);
  };

  window.__wh = {
    log,
    // performance.now() is monotonic within a page load, so gaps between
    // entries are meaningful (a gap is how you tell a device-initiated
    // report apart from a reply to something we sent); Date.now() is not
    // monotonic and a clock adjustment could mislead that reading.
    // installedAt is the one wall-clock anchor, taken once, at shim install.
    installedAt: new Date().toISOString(),
    jsonl: () => log.map((e) => JSON.stringify(e)).join("\n"),
    clear: () => (log.length = 0),
    // Clears the log and announces the scenario name in the console, so a
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
      console.log(`[wh] capture started: ${name} (log cleared)`);
    },
  };
  console.log("[wh] HID shim installed");
})();
