// WebHID logging shim for terminal.wallhack.com.
// Paste into DevTools console BEFORE the page opens the device (i.e. right
// after a hard reload), or inject via CDP Page.addScriptToEvaluateOnNewDocument.
// Dump with: copy(window.__wh.jsonl())
(() => {
  const log = [];

  // `sendReport(reportId, data)` and the `inputreport` event both hand us a
  // BufferSource: a plain ArrayBuffer, or, in practice for `inputreport`, a
  // DataView over one. A DataView's `.buffer` is the WHOLE underlying
  // ArrayBuffer, not the view's own window into it, so reading
  // `new Uint8Array(buf.buffer)` silently logs the wrong bytes whenever the
  // view has a non-zero byteOffset or a byteLength shorter than the buffer.
  // Read the view's own offset and length explicitly instead.
  const hex = (buf) => {
    const view =
      buf instanceof ArrayBuffer
        ? new Uint8Array(buf)
        : new Uint8Array(buf.buffer, buf.byteOffset, buf.byteLength);
    return [...view].map((b) => b.toString(16).padStart(2, "0")).join("");
  };

  const origSend = HIDDevice.prototype.sendReport;
  HIDDevice.prototype.sendReport = function (reportId, data) {
    // We believe reportId is always 0 and hid.rs prepends 0 on every write,
    // but this capture is the one chance to confirm that against the
    // vendor's own traffic, so record it rather than dropping it.
    log.push({ ts: Date.now(), dir: "out", report_id: reportId, hex: hex(data) });
    return origSend.call(this, reportId, data);
  };

  // A device that is opened twice, or that reconnects mid-session, must not
  // get a second "inputreport" listener attached: that would log every
  // inbound report twice. Track which devices already have one.
  const listenedDevices = new WeakSet();
  const origOpen = HIDDevice.prototype.open;
  HIDDevice.prototype.open = function () {
    if (!listenedDevices.has(this)) {
      listenedDevices.add(this);
      this.addEventListener("inputreport", (e) => {
        log.push({ ts: Date.now(), dir: "in", report_id: e.reportId, hex: hex(e.data) });
      });
    }
    return origOpen.call(this);
  };

  window.__wh = {
    log,
    // A timestamp per entry (ts, epoch ms) plus line order together make the
    // sequence within one capture file unambiguous, and let a later reader
    // spot gaps, which is how you tell a device-initiated report apart from
    // a reply to something we sent.
    jsonl: () => log.map((e) => JSON.stringify(e)).join("\n"),
    clear: () => (log.length = 0),
  };
  console.log("[wh] HID shim installed");
})();
