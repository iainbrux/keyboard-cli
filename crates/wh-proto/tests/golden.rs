//! Golden-fixture verification harness.
//!
//! Decodes every capture in `captures/` at the workspace root and classifies each frame rather
//! than asserting on it uniformly. The device speaks commands `wh-proto` does not model (RGB,
//! gamepad, calibration, and more), so a blanket "every frame must re-encode byte-identically"
//! assertion would fail on the first real capture for reasons that have nothing to do with a
//! codec bug. This harness only fails the test for classes that mean our own code is wrong;
//! everything else is counted and attributed to the capture file and direction it came from,
//! forming the inventory for any Phase 2 scoping conversation.
//!
//! A malformed line (bad JSON, a missing "hex" field, a bad hex character, a report that is not
//! 64 bytes) does not abort the run: it is recorded as a failure with its `file:line`, so one bad
//! line in one capture file does not hide the rest. The summary is printed and written to
//! `<CARGO_TARGET_TMPDIR>/golden-summary.txt` (not under `captures/`: the summary is derived and
//! regenerated every run, and would go stale sitting next to the data it describes) before the
//! test decides whether to fail.
//!
//! `classify` does not re-encode frames and compare bytes: `frame::frame` and `frame::parse` are
//! exact structural inverses given the same checksum formula, so that check would be tautological
//! (confirmed over 2,000,000 synthetic frames with zero mismatches). What every class below
//! except `FramingBug`/`LenLimitation` actually proves is that our
//! `0x35 + HEAD + len + cmd + payload.last()` checksum formula reproduces the checksum byte the
//! device really sent: that is what a successful `frame::parse` confirms.
//!
//! Feature reports (`sendFeatureReport`/`receiveFeatureReport`, logged as `"out-feature"`/
//! `"in-feature"`) are not required to be exactly `REPORT_LEN` bytes in either direction: a length
//! mismatch is reported, never a hard failure. An inbound one also gets a `WARNING:` callout,
//! since the WebHID spec allows `receiveFeatureReport()` to prefix the report ID as byte 0 "if
//! the device uses report IDs" (unlike `inputreport`'s data, which never does). Everything else
//! (`"in"`/`"out"`) is our fixed HID report framing and must be exactly 64 bytes or it is a hard
//! failure.
//!
//! # Captures are not committed
//!
//! `captures/` holds the operator's own device traffic and stays on their machine; see
//! `capture/README.md`. CI still exercises the full classifier through the synthetic fixtures in
//! `classifier_tests` below, which drive the same file/JSON/hex path a real `captures/*.jsonl`
//! file does. A missing `captures/` directory is the normal state, not a coverage gap: this test
//! skips cleanly, with a printed reason, when it is absent.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use wh_proto::frame::{self, FrameError, REPORT_LEN};

/// Command bytes `wh-proto` has typed encoders/decoders for. Anything else is an unmodelled
/// command, reported as data rather than failed outright.
const MODELLED_CMDS: &[u8] = &[
    wh_proto::cmds::cmd::CMD,
    wh_proto::cmds::cmd::SYNC,
    wh_proto::cmds::cmd::KEY,
    wh_proto::cmds::cmd::DB,
    wh_proto::cmds::cmd::DEFKEY,
];

/// The classification of a single captured, exactly-`REPORT_LEN`-byte report. Feature reports of
/// a different length never reach this: see `process_line`.
#[derive(Debug, PartialEq)]
enum Class {
    /// Bad magic, bad checksum, or a report shorter than 64 bytes: a codec bug this harness
    /// exists to catch.
    FramingBug(String),
    /// The declared length exceeds the 60-byte cap `parse` rejects. Kept distinct from
    /// `FramingBug` because it is a known limitation (multi-report replies for profile/matrix
    /// reads), not necessarily a random parse bug. `parse` checks length before checksum, so this
    /// frame's checksum was never verified.
    LenLimitation(u8),
    /// Command byte 0xFF: the device itself reported a failure. Valid magic, length and checksum,
    /// so not a codec bug: legitimate protocol behaviour, counted rather than failed.
    DeviceFail(u8),
    /// Parsed cleanly, but bytes past the declared payload length are non-zero. Tests our
    /// zero-padding assumption against real firmware. Reported, not failed.
    TrailingBytes { cmd: u8, extra_nonzero: usize },
    /// Parsed cleanly, clean trailing padding, but the command byte is not one `wh-proto` models.
    /// Reported, not failed: this is the inventory of what to scope for Phase 2.
    Unmodelled { cmd: u8 },
    /// Parsed cleanly, clean trailing padding, command is one of the five `wh-proto` models. See
    /// the module doc comment for what this class does and does not prove.
    Modelled { cmd: u8 },
}

/// Sort one already-decoded 64-byte report into a `Class`. Pure: never panics, so callers decide
/// how each class is handled and tests can assert on it directly.
fn classify(report: &[u8]) -> Class {
    match frame::parse(report) {
        Ok(reply) => {
            let declared_len = report[1] as usize;
            let trailing = &report[4 + declared_len..REPORT_LEN];
            let extra_nonzero = trailing.iter().filter(|&&b| b != 0).count();
            if extra_nonzero > 0 {
                return Class::TrailingBytes {
                    cmd: reply.cmd,
                    extra_nonzero,
                };
            }
            // The device sets the high bit (`REPLY_BIT`) on every reply's command byte, so
            // classification masks it off. The raw wire byte is still what gets counted and
            // printed below.
            if !MODELLED_CMDS.contains(&(reply.cmd & !frame::REPLY_BIT)) {
                return Class::Unmodelled { cmd: reply.cmd };
            }
            Class::Modelled { cmd: reply.cmd }
        }
        Err(FrameError::BadLength(len)) => Class::LenLimitation(len),
        Err(FrameError::DeviceFail(code)) => Class::DeviceFail(code),
        Err(e) => Class::FramingBug(e.to_string()),
    }
}

fn hex_digit(b: u8) -> Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        other => Err(format!("invalid hex digit 0x{other:02x}")),
    }
}

/// Byte-indexed, not `&str`-indexed: a `&str` slice index that lands inside a multi-byte UTF-8
/// character panics, and this decodes untrusted JSONL pasted from a browser DevTools console.
/// `wh-device::replay::unhex` has the identical fix for the identical reason; `wh-proto` cannot
/// depend on `wh-device`, so it is duplicated here rather than shared.
///
/// Does not require exactly `REPORT_LEN` bytes: a feature report has no such obligation, and
/// `process_line` needs the actual length to classify it correctly. Length validation for the
/// `"in"`/`"out"` directions happens one level up, in `process_line`.
fn decode_hex_bytes(s: &str) -> Result<Vec<u8>, String> {
    let bytes = s.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return Err(format!(
            "hex string has an odd number of characters ({})",
            bytes.len()
        ));
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for i in (0..bytes.len()).step_by(2) {
        let hi = hex_digit(bytes[i])?;
        let lo = hex_digit(bytes[i + 1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

/// Which capture file and logged direction a frame came from. The capture method is one
/// single-variable change per file (see `capture/README.md`), so a frame is only informative when
/// traced back to the scenario and direction that produced it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Origin {
    /// The capture file's name, e.g. `"profile-switch.jsonl"`.
    scenario: String,
    /// The captured `"dir"` field verbatim: `"in"`, `"out"`, `"in-feature"`,
    /// `"out-feature"`, or `"?"` if the line did not carry one.
    dir: String,
}

#[derive(Default, Debug, PartialEq)]
struct Summary {
    checked: usize,
    /// Modelled command, clean padding, checksum formula matched. See the
    /// module doc comment for what this does and does not prove.
    modelled: usize,
    /// (origin, device failure code) -> count.
    device_fail: BTreeMap<(Origin, u8), usize>,
    /// (origin, cmd) -> count of frames with non-zero bytes past the
    /// declared payload length.
    trailing_bytes: BTreeMap<(Origin, u8), usize>,
    /// (origin, cmd) -> count of frames whose command byte is not modelled.
    unmodelled: BTreeMap<(Origin, u8), usize>,
    /// (origin, actual byte length) -> count, for `"*-feature"` lines whose
    /// length is not `REPORT_LEN`. Not a failure: feature reports have no
    /// obligation to match our fixed HID report size.
    feature_length_mismatch: BTreeMap<(Origin, usize), usize>,
    /// report_id -> count, across every line that carried one. Confirms or refutes the
    /// "report_id is always 0" assumption `hid.rs` makes.
    report_ids: BTreeMap<u8, usize>,
    /// scenario file name -> (saw an inbound line, saw an outbound line). No inbound lines at all
    /// is the signature of the shim being installed after the page already opened the device: the
    /// `inputreport` listener never attaches, so every write goes out with no reply ever logged.
    /// Reported as a warning, not a hard failure.
    scenario_directions: BTreeMap<String, (bool, bool)>,
    /// file:line-prefixed hard-failure messages. A non-empty list fails the test, but only after
    /// the summary has already been printed and written.
    failures: Vec<String>,
}

fn note_direction(summary: &mut Summary, scenario: &str, dir: &str) {
    let entry = summary
        .scenario_directions
        .entry(scenario.to_string())
        .or_insert((false, false));
    if dir.starts_with("in") {
        entry.0 = true;
    } else if dir.starts_with("out") {
        entry.1 = true;
    }
}

fn process_line(scenario: &str, line_no: usize, line: &str, summary: &mut Summary) {
    let ctx = format!("{scenario}:{line_no}");
    let v: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            summary.failures.push(format!("{ctx}: invalid JSON: {e}"));
            return;
        }
    };
    let Some(hexs) = v["hex"].as_str() else {
        summary
            .failures
            .push(format!("{ctx}: missing \"hex\" field"));
        return;
    };
    let bytes = match decode_hex_bytes(hexs) {
        Ok(b) => b,
        Err(e) => {
            summary.failures.push(format!("{ctx}: {e}"));
            return;
        }
    };
    let dir = v["dir"].as_str().unwrap_or("?").to_string();
    let origin = Origin {
        scenario: scenario.to_string(),
        dir: dir.clone(),
    };

    if bytes.len() != REPORT_LEN {
        if dir.ends_with("-feature") {
            // A feature report (either direction) need not be REPORT_LEN bytes: report it, don't
            // fail. Inbound specifically gets a WARNING in render_summary, since WebHID allows
            // receiveFeatureReport() to prefix a report ID as byte 0, so this may be exactly that.
            summary.checked += 1;
            if let Some(id) = v["report_id"].as_u64() {
                *summary.report_ids.entry(id as u8).or_insert(0) += 1;
            }
            note_direction(summary, scenario, &dir);
            *summary
                .feature_length_mismatch
                .entry((origin, bytes.len()))
                .or_insert(0) += 1;
        } else {
            summary.failures.push(format!(
                "{ctx}: report is {} bytes, want {REPORT_LEN}",
                bytes.len()
            ));
        }
        return;
    }
    let report: [u8; REPORT_LEN] = bytes
        .try_into()
        .expect("length already checked to equal REPORT_LEN above");

    summary.checked += 1;
    if let Some(id) = v["report_id"].as_u64() {
        *summary.report_ids.entry(id as u8).or_insert(0) += 1;
    }
    note_direction(summary, scenario, &dir);

    match classify(&report) {
        Class::FramingBug(msg) => summary.failures.push(format!("{ctx}: codec bug: {msg}")),
        Class::LenLimitation(len) => summary.failures.push(format!(
            "{ctx}: framing limitation: declared length {len} exceeds the 60-byte cap parse() \
             rejects. parse() checks length before checksum, so this frame's checksum was not \
             verified. This may be a multi-report profile or matrix read, or an \
             unrelated corrupt capture; either way it needs a human look, not a guess."
        )),
        Class::DeviceFail(code) => {
            *summary.device_fail.entry((origin, code)).or_insert(0) += 1;
        }
        Class::TrailingBytes { cmd, .. } => {
            *summary.trailing_bytes.entry((origin, cmd)).or_insert(0) += 1;
        }
        Class::Unmodelled { cmd } => {
            *summary.unmodelled.entry((origin, cmd)).or_insert(0) += 1;
        }
        Class::Modelled { .. } => summary.modelled += 1,
    }
}

/// Scan `dir` for `*.jsonl` capture files and classify every report on every line. Never panics:
/// every problem is collected into `Summary::failures` with its `file:line` so the caller can
/// print and persist the whole picture before deciding whether to fail. Returns `None` only when
/// `dir` itself does not exist (pre-hardware CI).
fn scan_dir(dir: &Path) -> Option<Summary> {
    let entries = fs::read_dir(dir).ok()?;
    let mut summary = Summary::default();
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.extension().is_none_or(|e| e != "jsonl") {
            continue;
        }
        let scenario = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                summary.failures.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        for (lineno, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            process_line(&scenario, lineno + 1, line, &mut summary);
        }
    }
    Some(summary)
}

/// Sums a `(Origin, K) -> usize` map into a plain `K -> usize` total, so the summary can show a
/// total per command byte or failure code, with the per-scenario/direction attribution underneath.
fn aggregate<K: Ord + Copy>(map: &BTreeMap<(Origin, K), usize>) -> BTreeMap<K, usize> {
    let mut out = BTreeMap::new();
    for ((_, k), count) in map {
        *out.entry(*k).or_insert(0) += count;
    }
    out
}

fn render_summary(summary: &Summary) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "golden: {} reports checked", summary.checked);
    let _ = writeln!(
        out,
        "golden: {} modelled-command frames, clean padding, checksum formula matched",
        summary.modelled
    );
    let _ = writeln!(
        out,
        "golden: (every checked frame that is not a FramingBug/LenLimitation failure already \
         confirms the checksum formula against real firmware, not just the modelled ones above)"
    );
    if summary.report_ids.is_empty() {
        let _ = writeln!(out, "golden: no report_id field seen on any line");
    } else {
        let ids: Vec<String> = summary
            .report_ids
            .iter()
            .map(|(id, n)| format!("0x{id:02X} ({n}x)"))
            .collect();
        let _ = writeln!(out, "golden: report_id(s) seen: {}", ids.join(", "));
    }

    let device_fail_agg = aggregate(&summary.device_fail);
    let _ = writeln!(
        out,
        "golden: {} distinct device-reported-failure code(s), {} frame(s) total:",
        device_fail_agg.len(),
        device_fail_agg.values().sum::<usize>()
    );
    for (code, count) in &device_fail_agg {
        let _ = writeln!(out, "golden:   code 0x{code:02X}: {count} frame(s) total");
    }
    for ((origin, code), count) in &summary.device_fail {
        let _ = writeln!(
            out,
            "golden:     code 0x{code:02X}: {count} frame(s) [{}] {}",
            origin.dir, origin.scenario
        );
    }

    let trailing_agg = aggregate(&summary.trailing_bytes);
    let _ = writeln!(
        out,
        "golden: {} distinct command byte(s) with non-zero trailing bytes past declared \
         length, {} frame(s) total:",
        trailing_agg.len(),
        trailing_agg.values().sum::<usize>()
    );
    for (cmd, count) in &trailing_agg {
        let _ = writeln!(out, "golden:   cmd 0x{cmd:02X}: {count} frame(s) total");
    }
    for ((origin, cmd), count) in &summary.trailing_bytes {
        let _ = writeln!(
            out,
            "golden:     cmd 0x{cmd:02X}: {count} frame(s) [{}] {}",
            origin.dir, origin.scenario
        );
    }

    let unmodelled_agg = aggregate(&summary.unmodelled);
    let _ = writeln!(
        out,
        "golden: {} distinct unmodelled command byte(s), {} frame(s) total:",
        unmodelled_agg.len(),
        unmodelled_agg.values().sum::<usize>()
    );
    for (cmd, count) in &unmodelled_agg {
        let _ = writeln!(out, "golden:   cmd 0x{cmd:02X}: {count} frame(s) total");
    }
    for ((origin, cmd), count) in &summary.unmodelled {
        let _ = writeln!(
            out,
            "golden:     cmd 0x{cmd:02X}: {count} frame(s) [{}] {}",
            origin.dir, origin.scenario
        );
    }

    if !summary.feature_length_mismatch.is_empty() {
        let inbound_total: usize = summary
            .feature_length_mismatch
            .iter()
            .filter(|((origin, _), _)| origin.dir.starts_with("in"))
            .map(|(_, count)| *count)
            .sum();
        if inbound_total > 0 {
            let _ = writeln!(
                out,
                "golden: WARNING: {inbound_total} inbound feature-report frame(s) with a \
                 length other than {REPORT_LEN} bytes. Not a failure, but the WebHID \
                 specification allows receiveFeatureReport() to prefix the report ID as byte \
                 0 \"if the device uses report IDs\", so this may be exactly that; see \
                 capture/README.md."
            );
        }
        let _ = writeln!(
            out,
            "golden: {} feature-report length-mismatch combination(s) (not a failure: a \
             feature report has no obligation to be {REPORT_LEN} bytes):",
            summary.feature_length_mismatch.len()
        );
        for ((origin, len), count) in &summary.feature_length_mismatch {
            let _ = writeln!(
                out,
                "golden:   {len} byte(s): {count} frame(s) [{}] {}",
                origin.dir, origin.scenario
            );
        }
    }

    let silent: Vec<&String> = summary
        .scenario_directions
        .iter()
        .filter(|(_, (saw_in, _))| !saw_in)
        .map(|(name, _)| name)
        .collect();
    if !silent.is_empty() {
        let _ = writeln!(
            out,
            "golden: WARNING: {} scenario file(s) contain no inbound frames at all (only \
             outgoing writes, never a logged reply). This is the signature of the shim being \
             installed after the page already opened the device: open() is never called again, \
             so the inputreport listener never attaches. Re-capture these after a hard reload, \
             shim first:",
            silent.len()
        );
        for name in silent {
            let _ = writeln!(out, "golden:   {name}");
        }
    }

    if !summary.failures.is_empty() {
        let _ = writeln!(out, "golden: {} hard failure(s):", summary.failures.len());
        for f in &summary.failures {
            let _ = writeln!(out, "golden:   {f}");
        }
    }
    out
}

/// Print the summary to stderr (visible with `cargo test -- --nocapture`, swallowed by a bare
/// `cargo test`) and write it to `<out_dir>/golden-summary.txt` so it survives either way.
/// `out_dir` is `CARGO_TARGET_TMPDIR` in the real test, not `captures/` itself: see the module
/// doc comment for why.
fn report_summary(summary: &Summary, out_dir: &Path) {
    let text = render_summary(summary);
    eprint!("{text}");
    let out_path = out_dir.join("golden-summary.txt");
    match fs::write(&out_path, &text) {
        Ok(()) => eprintln!("golden: summary written to {}", out_path.display()),
        Err(e) => eprintln!(
            "golden: could not write summary to {}: {e}",
            out_path.display()
        ),
    }
}

fn assert_no_hard_failures(summary: &Summary) {
    assert!(
        summary.failures.is_empty(),
        "{} hard failure(s):\n{}",
        summary.failures.len(),
        summary.failures.join("\n")
    );
}

/// Every captured report must parse (magic/len/crc). Everything else, unmodelled commands,
/// trailing padding garbage, device failure replies, is counted and attributed rather than
/// failed. See the module doc comment for the full design.
#[test]
fn all_captures_decode_and_classify() {
    let captures_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../captures");
    let Some(summary) = scan_dir(&captures_dir) else {
        eprintln!("no captures/ yet, skipping");
        return;
    };
    report_summary(&summary, Path::new(env!("CARGO_TARGET_TMPDIR")));
    assert_no_hard_failures(&summary);
}

/// The measured sweep behind 3.4's queueing change: every inbound, exactly-`REPORT_LEN`-byte
/// frame in `captures/` is offered to `adjust_event`, and the fires must equal exactly the
/// frames that parse with `cmd == REPLY_BIT` and a payload starting `00 be 00` or `00 be 01`,
/// with no outbound frame ever firing. Asserts the property, not the literal counts (measured
/// here as 4 fires across 3772 inbound frames, both from the two board-side capture files), since
/// the corpus grows. Skips silently, like `all_captures_decode_and_classify`, when `captures/` is
/// absent.
#[test]
fn adjust_event_fires_exactly_on_the_measured_edges_and_never_outbound() {
    let captures_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../captures");
    let Ok(entries) = fs::read_dir(&captures_dir) else {
        eprintln!("no captures/ yet, skipping");
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();

    let mut inbound_checked = 0usize;
    let mut inbound_fires = 0usize;
    let mut expected_edges = 0usize;
    let mut outbound_fires = 0usize;

    for path in paths {
        if path.extension().is_none_or(|e| e != "jsonl") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let Some(hexs) = v["hex"].as_str() else {
                continue;
            };
            let Ok(bytes) = decode_hex_bytes(hexs) else {
                continue;
            };
            if bytes.len() != REPORT_LEN {
                continue;
            }
            let report: [u8; REPORT_LEN] = bytes.try_into().expect("length checked above");
            let dir = v["dir"].as_str().unwrap_or("?");
            let fires = wh_proto::event::adjust_event(&report).is_some();

            if dir.starts_with("out") {
                if fires {
                    outbound_fires += 1;
                }
                continue;
            }
            if !dir.starts_with("in") {
                continue;
            }
            inbound_checked += 1;
            if fires {
                inbound_fires += 1;
            }
            if let Ok(reply) = frame::parse(&report) {
                if reply.cmd == frame::REPLY_BIT
                    && matches!(
                        reply.payload,
                        [0x00, 0xbe, 0x00, ..] | [0x00, 0xbe, 0x01, ..]
                    )
                {
                    expected_edges += 1;
                }
            }
        }
    }

    if inbound_checked == 0 {
        eprintln!("no captures/ yet, skipping");
        return;
    }
    eprintln!(
        "golden: adjust_event sweep: {inbound_fires} fire(s) across {inbound_checked} inbound \
         frame(s)"
    );
    assert_eq!(
        inbound_fires, expected_edges,
        "adjust_event's fires must equal exactly the frames whose payload starts 00 be 00|01"
    );
    assert_eq!(
        outbound_fires, 0,
        "adjust_event must never fire on an outbound frame"
    );
}

#[cfg(test)]
mod classifier_tests {
    use super::*;

    fn nonzero_pad_frame(cmd: u8, payload: &[u8], pad_at: usize, pad_byte: u8) -> Vec<u8> {
        let mut f = frame::frame(cmd, payload).unwrap().to_vec();
        f[pad_at] = pad_byte;
        f
    }

    fn unique_temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "wh-golden-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn jsonl_line(dir_field: &str, report_id: u8, report: &[u8]) -> String {
        let hexs: String = report.iter().map(|b| format!("{b:02x}")).collect();
        format!("{{\"ts\":0,\"dir\":\"{dir_field}\",\"report_id\":{report_id},\"hex\":\"{hexs}\"}}")
    }

    #[test]
    fn modelled_command_is_classified_as_modelled() {
        // cmd::DB is one of the five commands wh-proto has typed support for; a clean frame
        // for it must classify as Modelled.
        let f = frame::frame(wh_proto::cmds::cmd::DB, &[0x00, 0x01, 0x02]).unwrap();
        assert_eq!(
            classify(&f),
            Class::Modelled {
                cmd: wh_proto::cmds::cmd::DB
            }
        );
    }

    /// A real device reply carries the high bit on its command byte (e.g. `0xA9` for `cmd::DB` =
    /// `0x29`): must still classify as `Modelled`, keeping the raw wire byte (with the high bit),
    /// not the masked request-side byte.
    #[test]
    fn a_reply_with_the_high_bit_set_on_a_modelled_command_is_classified_as_modelled() {
        let reply_cmd = wh_proto::cmds::cmd::DB | frame::REPLY_BIT;
        let f = frame::frame(reply_cmd, &[0x00, 0x01, 0x02]).unwrap();
        assert_eq!(classify(&f), Class::Modelled { cmd: reply_cmd });
    }

    #[test]
    fn unmodelled_command_is_reported_not_failed() {
        // 0x40 is not in MODELLED_CMDS: stands in for an unmodelled command like RGB or gamepad.
        let cmd = 0x40u8;
        assert!(!MODELLED_CMDS.contains(&cmd));
        let f = frame::frame(cmd, &[0xAA, 0xBB]).unwrap();
        assert_eq!(classify(&f), Class::Unmodelled { cmd });
    }

    #[test]
    fn trailing_nonzero_bytes_are_reported_not_failed() {
        // A clean 3-byte DB payload with a stray non-zero byte in the padding region past the
        // declared length. checksum() only covers the declared payload, so this does not
        // disturb it.
        let cmd = wh_proto::cmds::cmd::DB;
        let f = nonzero_pad_frame(cmd, &[0x00, 0x01, 0x02], 40, 0x7F);
        assert_eq!(
            classify(&f),
            Class::TrailingBytes {
                cmd,
                extra_nonzero: 1
            }
        );
    }

    #[test]
    fn bad_checksum_is_a_hard_failure_class() {
        let mut f = frame::frame(wh_proto::cmds::cmd::DB, &[0x01]).unwrap();
        f[3] ^= 0xFF;
        match classify(&f) {
            Class::FramingBug(msg) => assert!(msg.contains("checksum")),
            other => panic!("expected FramingBug, got {other:?}"),
        }
    }

    #[test]
    fn bad_magic_is_a_hard_failure_class() {
        let mut f = frame::frame(wh_proto::cmds::cmd::DB, &[0x01]).unwrap();
        f[0] = 0x00;
        match classify(&f) {
            Class::FramingBug(msg) => assert!(msg.contains("magic")),
            other => panic!("expected FramingBug, got {other:?}"),
        }
    }

    #[test]
    fn short_report_is_a_hard_failure_class() {
        let short = [frame::HEAD, 0x00, wh_proto::cmds::cmd::DB, 0xBA];
        match classify(&short) {
            Class::FramingBug(_) => {}
            other => panic!("expected FramingBug, got {other:?}"),
        }
    }

    #[test]
    fn declared_length_over_60_is_a_len_limitation_not_a_generic_bug() {
        // Hand-built rather than via frame::frame, which refuses to build an oversize payload:
        // models what a real device reply with too-large declared length looks like on the wire.
        let mut f = [0u8; REPORT_LEN];
        f[0] = frame::HEAD;
        f[1] = 61; // REPORT_LEN - 4 is 60, so 61 trips the cap.
        f[2] = wh_proto::cmds::cmd::DB;
        assert_eq!(classify(&f), Class::LenLimitation(61));
    }

    #[test]
    fn device_reported_failure_is_counted_not_failed() {
        let f = frame::frame(frame::CMD_FAIL, &[0x03]).unwrap();
        assert_eq!(classify(&f), Class::DeviceFail(0x03));
    }

    /// Drives the classifier through the same file-reading, JSON-parsing, and hex-decoding path a
    /// real `captures/*.jsonl` file goes through, and checks that every count is attributed to
    /// the scenario file and direction, not just aggregated globally.
    #[test]
    fn a_synthetic_capture_file_sorts_into_the_expected_buckets_with_attribution() {
        let dir = unique_temp_dir("attribution");
        let round_trip =
            frame::frame(wh_proto::cmds::cmd::KEY, &[0x00, 0x1A, 0x04, 0xF4, 0x01]).unwrap();
        let unmodelled = frame::frame(0x40, &[0x01, 0x02]).unwrap();
        let mut trailing = frame::frame(wh_proto::cmds::cmd::DB, &[0x00]).unwrap();
        trailing[50] = 0xEE;
        let device_fail = frame::frame(frame::CMD_FAIL, &[0x03]).unwrap();

        let lines = [
            jsonl_line("in", 0, &round_trip),
            String::new(), // blank lines must be skipped, not counted
            jsonl_line("out", 0, &unmodelled),
            jsonl_line("in", 0, &trailing),
            jsonl_line("in", 5, &device_fail),
        ]
        .join("\n");
        fs::write(dir.join("profile-switch.jsonl"), lines).unwrap();

        let summary = scan_dir(&dir).expect("dir exists, so this must be Some");
        fs::remove_dir_all(&dir).unwrap();

        assert!(summary.failures.is_empty(), "{:?}", summary.failures);
        assert_eq!(summary.checked, 4);
        assert_eq!(summary.modelled, 1);

        let mut saw_unmodelled = false;
        for ((origin, cmd), count) in &summary.unmodelled {
            if *cmd == 0x40 {
                assert_eq!(origin.scenario, "profile-switch.jsonl");
                assert_eq!(origin.dir, "out");
                assert_eq!(*count, 1);
                saw_unmodelled = true;
            }
        }
        assert!(
            saw_unmodelled,
            "0x40 must be attributed to profile-switch.jsonl/out"
        );

        let mut saw_trailing = false;
        for ((origin, cmd), count) in &summary.trailing_bytes {
            if *cmd == wh_proto::cmds::cmd::DB {
                assert_eq!(origin.scenario, "profile-switch.jsonl");
                assert_eq!(origin.dir, "in");
                assert_eq!(*count, 1);
                saw_trailing = true;
            }
        }
        assert!(
            saw_trailing,
            "DB trailing bytes must be attributed to profile-switch.jsonl/in"
        );

        let mut saw_device_fail = false;
        for ((origin, code), count) in &summary.device_fail {
            if *code == 0x03 {
                assert_eq!(origin.scenario, "profile-switch.jsonl");
                assert_eq!(origin.dir, "in");
                assert_eq!(*count, 1);
                saw_device_fail = true;
            }
        }
        assert!(
            saw_device_fail,
            "device failure code 0x03 must be attributed"
        );

        assert_eq!(summary.report_ids.get(&0), Some(&3));
        assert_eq!(summary.report_ids.get(&5), Some(&1));

        // profile-switch.jsonl had inbound lines, so it must not appear in the "no inbound
        // frames" warning list.
        assert_eq!(
            summary.scenario_directions.get("profile-switch.jsonl"),
            Some(&(true, true))
        );
    }

    #[test]
    fn a_malformed_line_is_collected_not_panicked_and_does_not_hide_the_rest_of_the_file() {
        let dir = unique_temp_dir("malformed-line");
        let good = frame::frame(wh_proto::cmds::cmd::DB, &[0x01]).unwrap();
        let lines = [
            "not json at all".to_string(),
            "{\"dir\":\"in\",\"report_id\":0,\"hex\":\"deadbeef\"}".to_string(), // wrong length
            jsonl_line("in", 0, &good),
        ]
        .join("\n");
        fs::write(dir.join("noisy.jsonl"), lines).unwrap();

        let summary = scan_dir(&dir).expect("dir exists");
        fs::remove_dir_all(&dir).unwrap();

        // The one well-formed line must still have been checked and
        // classified, proving the two bad lines did not abort the scan.
        assert_eq!(summary.checked, 1);
        assert_eq!(summary.modelled, 1);
        assert_eq!(summary.failures.len(), 2);
        assert!(summary.failures[0].contains("noisy.jsonl:1"));
        assert!(summary.failures[0].contains("invalid JSON"));
        assert!(summary.failures[1].contains("noisy.jsonl:2"));
    }

    /// Proves the two remaining "wrong shape" hard-failure classes actually fail the test end to
    /// end, through `scan_dir` and `assert_no_hard_failures`, the same path
    /// `all_captures_decode_and_classify` uses, not just that `classify` returns the right enum
    /// variant in isolation.
    #[test]
    #[should_panic(expected = "hard failure")]
    fn a_bad_checksum_frame_fails_the_whole_run() {
        let dir = unique_temp_dir("bad-checksum-e2e");
        let mut f = frame::frame(wh_proto::cmds::cmd::DB, &[0x01]).unwrap();
        f[3] ^= 0xFF;
        fs::write(dir.join("bad.jsonl"), jsonl_line("in", 0, &f)).unwrap();
        let summary = scan_dir(&dir).expect("dir exists");
        fs::remove_dir_all(&dir).unwrap();
        assert_no_hard_failures(&summary);
    }

    #[test]
    #[should_panic(expected = "hard failure")]
    fn a_length_over_60_frame_fails_the_whole_run() {
        let dir = unique_temp_dir("len-over-60-e2e");
        let mut f = [0u8; REPORT_LEN];
        f[0] = frame::HEAD;
        f[1] = 61;
        f[2] = wh_proto::cmds::cmd::DB;
        fs::write(dir.join("bad.jsonl"), jsonl_line("in", 0, &f)).unwrap();
        let summary = scan_dir(&dir).expect("dir exists");
        fs::remove_dir_all(&dir).unwrap();
        assert_no_hard_failures(&summary);
    }

    /// An outbound feature report is not required to be REPORT_LEN bytes and carries no
    /// device-originated data, so a length mismatch on one must be reported, not treated as a
    /// hard failure.
    #[test]
    fn an_outbound_feature_report_of_unexpected_length_is_reported_not_failed() {
        let dir = unique_temp_dir("feature-length-out");
        let long = vec![0xBBu8; 65];
        fs::write(
            dir.join("feature-probe.jsonl"),
            jsonl_line("out-feature", 0, &long),
        )
        .unwrap();

        let summary = scan_dir(&dir).expect("dir exists");
        fs::remove_dir_all(&dir).unwrap();

        assert!(summary.failures.is_empty(), "{:?}", summary.failures);
        assert_eq!(summary.checked, 1);
        let mut saw_65 = false;
        for ((origin, len), count) in &summary.feature_length_mismatch {
            if *len == 65 {
                assert_eq!(origin.dir, "out-feature");
                assert_eq!(*count, 1);
                saw_65 = true;
            }
        }
        assert!(saw_65);
    }

    /// An inbound feature report of unexpected length is not a hard failure, but it may be
    /// exactly the WebHID report-ID-prefix behaviour the spec allows, so it must be called out
    /// in the `WARNING:` block, not described as benign.
    #[test]
    fn an_inbound_feature_report_of_the_wrong_length_is_reported_with_a_warning_not_failed() {
        let dir = unique_temp_dir("feature-length-in");
        let suspect = vec![0xAAu8; 65];
        fs::write(
            dir.join("feature-probe.jsonl"),
            jsonl_line("in-feature", 0, &suspect),
        )
        .unwrap();
        let summary = scan_dir(&dir).expect("dir exists");
        fs::remove_dir_all(&dir).unwrap();

        assert!(summary.failures.is_empty(), "{:?}", summary.failures);
        assert_eq!(summary.checked, 1);
        let mut saw_65 = false;
        for ((origin, len), count) in &summary.feature_length_mismatch {
            if *len == 65 {
                assert_eq!(origin.dir, "in-feature");
                assert_eq!(*count, 1);
                saw_65 = true;
            }
        }
        assert!(saw_65);

        let text = render_summary(&summary);
        assert!(text.contains("WARNING"), "{text}");
        assert!(text.contains("inbound feature-report"), "{text}");
    }

    /// A non-feature line ("in"/"out") of the wrong length is still a hard
    /// failure: only `"*-feature"` directions get any kind of pass.
    #[test]
    #[should_panic(expected = "hard failure")]
    fn a_non_feature_line_of_the_wrong_length_still_fails() {
        let dir = unique_temp_dir("non-feature-wrong-length");
        fs::write(dir.join("bad.jsonl"), jsonl_line("in", 0, &[0xAA; 32])).unwrap();
        let summary = scan_dir(&dir).expect("dir exists");
        fs::remove_dir_all(&dir).unwrap();
        assert_no_hard_failures(&summary);
    }

    /// `render_summary` on a hand-built `Summary` must mention every field that was populated.
    #[test]
    fn render_summary_includes_every_populated_field() {
        let mut summary = Summary {
            checked: 5,
            modelled: 2,
            ..Summary::default()
        };
        summary.report_ids.insert(0x00, 4);
        summary.unmodelled.insert(
            (
                Origin {
                    scenario: "profile-switch.jsonl".to_string(),
                    dir: "out".to_string(),
                },
                0x51,
            ),
            3,
        );
        summary
            .failures
            .push("demo.jsonl:1: codec bug: checksum mismatch".to_string());

        let text = render_summary(&summary);

        assert!(text.contains("5 reports checked"), "{text}");
        assert!(text.contains("2 modelled-command"), "{text}");
        assert!(text.contains("0x00 (4x)"), "{text}");
        assert!(
            text.contains("cmd 0x51: 3 frame(s) total"),
            "aggregate line missing: {text}"
        );
        assert!(
            text.contains("cmd 0x51: 3 frame(s) [out] profile-switch.jsonl"),
            "per-origin attribution line missing: {text}"
        );
        assert!(text.contains("1 hard failure(s):"), "{text}");
        assert!(
            text.contains("demo.jsonl:1: codec bug: checksum mismatch"),
            "{text}"
        );
    }

    #[test]
    fn render_summary_warns_about_a_scenario_with_no_inbound_frames() {
        let mut summary = Summary::default();
        summary
            .scenario_directions
            .insert("outbound-only.jsonl".to_string(), (false, true));
        summary
            .scenario_directions
            .insert("normal.jsonl".to_string(), (true, true));

        let text = render_summary(&summary);

        assert!(text.contains("no inbound frames"), "{text}");
        assert!(text.contains("outbound-only.jsonl"), "{text}");
        assert!(!text.contains("normal.jsonl\n"), "{text}");
    }

    #[test]
    fn report_summary_writes_the_rendered_text_to_a_file_in_out_dir() {
        let dir = unique_temp_dir("report-summary-write");
        let summary = Summary {
            checked: 1,
            modelled: 1,
            ..Summary::default()
        };

        report_summary(&summary, &dir);

        let written = fs::read_to_string(dir.join("golden-summary.txt")).unwrap();
        assert_eq!(written, render_summary(&summary));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_captures_dir_skips_cleanly() {
        let dir = std::env::temp_dir().join(format!(
            "wh-golden-definitely-does-not-exist-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert!(!dir.exists());
        assert_eq!(scan_dir(&dir), None);
    }
}
