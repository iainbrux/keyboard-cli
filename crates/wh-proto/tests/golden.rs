//! Golden-fixture verification harness.
//!
//! Decodes every capture in `captures/` at the workspace root and classifies
//! each frame rather than asserting uniformly over it. The device speaks far
//! more than `wh-proto` currently models: four profiles, SOCD, dynamic
//! keystroke, mod tap, a full gamepad mode with a joystick curve, RGB, switch
//! type selection, calibration. A real capture of the vendor web UI will be
//! full of command bytes this crate has never heard of, and `parse` accepts
//! any command byte, it only validates magic, length and checksum. A blanket
//! "every frame must re-encode byte-identically" assertion would therefore
//! fail on the first real capture for reasons that have nothing to do with a
//! codec bug, and the natural response to that failing test, weakening the
//! assert, would throw away the only thing it exists to check.
//!
//! So this harness sorts every frame into one of a small number of buckets
//! and only fails the test for the buckets that mean *our own code is wrong*.
//! Everything else is counted and printed: that inventory is the input to any
//! Phase 2 scoping conversation.
//!
//! Skips cleanly, with a printed reason, when `captures/` does not exist yet
//! (pre-hardware CI, i.e. every run before Task 19 supplies real captures).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use wh_proto::frame::{self, FrameError, REPORT_LEN};

/// Command bytes `wh-proto` has typed encoders/decoders for. Anything else is
/// an unmodelled command: real firmware traffic will contain plenty of these,
/// and the harness's job on first contact with it is to say so as data
/// rather than fail outright.
const MODELLED_CMDS: &[u8] = &[
    wh_proto::cmds::cmd::CMD,
    wh_proto::cmds::cmd::SYNC,
    wh_proto::cmds::cmd::KEY,
    wh_proto::cmds::cmd::DB,
    wh_proto::cmds::cmd::DEFKEY,
];

/// The classification of a single captured 64-byte report.
#[derive(Debug, PartialEq)]
enum Class {
    /// Bad magic, bad checksum, or a report shorter than 64 bytes. A codec
    /// bug or a framing assumption we get wrong: exactly what this harness
    /// exists to catch.
    FramingBug(String),
    /// The declared length exceeds the 60-byte cap `parse` rejects. Kept
    /// distinct from `FramingBug` because it is the known Task 2/3 deferred
    /// minor (multi-report replies for profile/matrix reads "must bypass or
    /// extend later"), not a random parse bug, and the two need different
    /// responses from whoever reads the failure.
    LenLimitation(u8),
    /// Command byte 0xFF: the device itself reported a failure. Legitimate
    /// protocol behaviour, not a bug in us, so it is counted rather than
    /// failed.
    DeviceFail(u8),
    /// Parsed cleanly, but bytes past the declared payload length are
    /// non-zero. Tests our zero-padding assumption against real firmware,
    /// which nothing has checked before now. Reported, not failed.
    TrailingBytes { cmd: u8, extra_nonzero: usize },
    /// Parsed cleanly, clean trailing padding, but the command byte is not
    /// one `wh-proto` models. Reported, not failed: this is the inventory of
    /// what to scope for Phase 2.
    Unmodelled { cmd: u8 },
    /// Parsed cleanly, clean trailing padding, command is modelled, but
    /// re-encoding through `frame::frame` did not reproduce the wire bytes.
    /// A real codec bug: hard failure.
    ReencodeMismatch { cmd: u8, detail: String },
    /// Parsed cleanly, clean trailing padding, command is modelled, and
    /// re-encoding through `frame::frame` reproduced the wire bytes exactly.
    /// The strict assertion the plan wanted; kept strict deliberately.
    RoundTripped { cmd: u8 },
}

/// Sort one already-decoded 64-byte report into a `Class`. Pure: never
/// panics, so callers can decide how each class is handled (and tests can
/// assert on it directly) without fighting `catch_unwind`.
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
            if !MODELLED_CMDS.contains(&reply.cmd) {
                return Class::Unmodelled { cmd: reply.cmd };
            }
            // A successful parse() already bounds declared_len (and so
            // reply.payload.len()) to <=60, which is frame()'s own contract,
            // so this cannot fail in practice; treated as a hard bug if it
            // somehow does rather than unwrapped blindly.
            let rebuilt = match frame::frame(reply.cmd, reply.payload) {
                Ok(r) => r,
                Err(e) => {
                    return Class::ReencodeMismatch {
                        cmd: reply.cmd,
                        detail: format!("frame() itself rejected the decoded payload: {e}"),
                    }
                }
            };
            if rebuilt[..] != report[..REPORT_LEN] {
                return Class::ReencodeMismatch {
                    cmd: reply.cmd,
                    detail: format!(
                        "rebuilt {:02x?} vs captured {:02x?}",
                        rebuilt,
                        &report[..REPORT_LEN]
                    ),
                };
            }
            Class::RoundTripped { cmd: reply.cmd }
        }
        Err(FrameError::BadLength(len)) => Class::LenLimitation(len),
        Err(FrameError::DeviceFail(code)) => Class::DeviceFail(code),
        Err(e) => Class::FramingBug(e.to_string()),
    }
}

#[derive(Default, Debug, PartialEq)]
struct Summary {
    checked: usize,
    round_tripped: usize,
    device_fail: usize,
    trailing_bytes_frames: usize,
    unmodelled: BTreeMap<u8, usize>,
}

fn decode_hex_report(hexs: &str, ctx: &str) -> Vec<u8> {
    assert!(
        hexs.len().is_multiple_of(2),
        "{ctx}: odd-length hex string ({} chars)",
        hexs.len()
    );
    (0..hexs.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hexs[i..i + 2], 16)
                .unwrap_or_else(|e| panic!("{ctx}: bad hex byte at offset {i}: {e}"))
        })
        .collect()
}

/// Scan `dir` for `*.jsonl` capture files, classify every report in every
/// line, and either panic (for the classes that mean our own code is wrong)
/// or fold the frame into `Summary`. Returns `None` only when `dir` itself
/// does not exist, which is the pre-hardware CI case.
fn classify_dir(dir: &Path) -> Option<Summary> {
    let entries = fs::read_dir(dir).ok()?;
    let mut summary = Summary::default();
    let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.extension().is_none_or(|e| e != "jsonl") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for (lineno, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let ctx = format!("{}:{}", path.display(), lineno + 1);
            let v: serde_json::Value =
                serde_json::from_str(line).unwrap_or_else(|e| panic!("{ctx}: invalid JSON: {e}"));
            let hexs = v["hex"]
                .as_str()
                .unwrap_or_else(|| panic!("{ctx}: missing \"hex\" field"));
            let bytes = decode_hex_report(hexs, &ctx);
            assert_eq!(
                bytes.len(),
                REPORT_LEN,
                "{ctx}: report is {} bytes, want {REPORT_LEN}",
                bytes.len()
            );
            summary.checked += 1;
            match classify(&bytes) {
                Class::FramingBug(msg) => panic!("{ctx}: codec bug: {msg}"),
                Class::LenLimitation(len) => panic!(
                    "{ctx}: framing limitation: declared length {len} exceeds the 60-byte cap \
                     parse() rejects. Likely a multi-report reply (a profile or matrix read), \
                     the known Task 2/3 deferred minor, not a random parse bug. This needs a \
                     framing extension, not a codec fix."
                ),
                Class::ReencodeMismatch { cmd, detail } => panic!(
                    "{ctx}: modelled cmd 0x{cmd:02X} did not re-encode byte-identically: {detail}"
                ),
                Class::DeviceFail(_) => summary.device_fail += 1,
                Class::TrailingBytes { .. } => summary.trailing_bytes_frames += 1,
                Class::Unmodelled { cmd } => {
                    *summary.unmodelled.entry(cmd).or_insert(0) += 1;
                }
                Class::RoundTripped { .. } => summary.round_tripped += 1,
            }
        }
    }
    Some(summary)
}

fn print_summary(summary: &Summary) {
    eprintln!("golden: {} reports checked", summary.checked);
    eprintln!(
        "golden: {} strictly round-tripped (modelled commands)",
        summary.round_tripped
    );
    eprintln!(
        "golden: {} device-reported failures (cmd 0xFF)",
        summary.device_fail
    );
    eprintln!(
        "golden: {} frames with non-zero trailing bytes past declared length",
        summary.trailing_bytes_frames
    );
    if summary.unmodelled.is_empty() {
        eprintln!("golden: 0 unmodelled command bytes");
    } else {
        eprintln!(
            "golden: {} distinct unmodelled command byte(s):",
            summary.unmodelled.len()
        );
        for (cmd, count) in &summary.unmodelled {
            eprintln!("golden:   cmd 0x{cmd:02X}: {count} frame(s)");
        }
    }
}

/// Every captured report must parse (magic/len/crc). Every command
/// `wh-proto` models must re-encode byte-identically from its decoded form.
/// Everything else, unmodelled commands, trailing padding garbage, device
/// failure replies, is counted and printed rather than failed. See the
/// module doc comment for why.
#[test]
fn all_captures_decode_and_classify() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../captures");
    let Some(summary) = classify_dir(&dir) else {
        eprintln!("no captures/ yet, skipping");
        return;
    };
    print_summary(&summary);
}

#[cfg(test)]
mod classifier_tests {
    use super::*;

    fn nonzero_pad_frame(cmd: u8, payload: &[u8], pad_at: usize, pad_byte: u8) -> Vec<u8> {
        let mut f = frame::frame(cmd, payload).unwrap().to_vec();
        f[pad_at] = pad_byte;
        f
    }

    #[test]
    fn modelled_command_round_trips() {
        // cmd::DB is one of the five commands wh-proto has typed support
        // for; a clean frame for it must classify as RoundTripped.
        let f = frame::frame(wh_proto::cmds::cmd::DB, &[0x00, 0x01, 0x02]).unwrap();
        assert_eq!(
            classify(&f),
            Class::RoundTripped {
                cmd: wh_proto::cmds::cmd::DB
            }
        );
    }

    #[test]
    fn unmodelled_command_is_reported_not_failed() {
        // 0x40 is not in MODELLED_CMDS: stands in for e.g. an RGB or gamepad
        // command the vendor UI can send that wh-proto has never heard of.
        let cmd = 0x40u8;
        assert!(!MODELLED_CMDS.contains(&cmd));
        let f = frame::frame(cmd, &[0xAA, 0xBB]).unwrap();
        assert_eq!(classify(&f), Class::Unmodelled { cmd });
    }

    #[test]
    fn trailing_nonzero_bytes_are_reported_not_failed() {
        // A clean 3-byte DB payload, but with a stray non-zero byte in the
        // padding region past the declared length. checksum() only ever
        // covers the declared payload, so this does not disturb it.
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
    fn declared_length_over_60_is_a_len_limitation_not_a_generic_bug() {
        // Hand-built rather than via frame::frame, which itself refuses to
        // build an oversize payload: we need to model what a real device
        // reply with a too-large declared length would look like on the wire.
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

    #[test]
    fn short_report_is_a_hard_failure_class() {
        let short = [frame::HEAD, 0x00, wh_proto::cmds::cmd::DB, 0xBA];
        match classify(&short) {
            Class::FramingBug(_) => {}
            other => panic!("expected FramingBug, got {other:?}"),
        }
    }

    /// Drives the classifier through the same file-reading, JSON-parsing and
    /// hex-decoding path a real `captures/*.jsonl` file goes through, using
    /// synthetic fixtures covering the non-panicking classes: one round trip,
    /// one unmodelled command, and one frame with trailing garbage. Proves
    /// the classifier sorts a real capture file correctly, not just a bare
    /// byte array.
    #[test]
    fn a_synthetic_capture_file_sorts_into_the_expected_buckets() {
        let dir = std::env::temp_dir().join(format!(
            "wh-golden-classify-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();

        let round_trip = frame::frame(wh_proto::cmds::cmd::KEY, &[0x00, 0x1A, 0x04, 0xF4, 0x01])
            .unwrap()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let unmodelled = frame::frame(0x40, &[0x01, 0x02])
            .unwrap()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let mut trailing_bytes = frame::frame(wh_proto::cmds::cmd::DB, &[0x00]).unwrap();
        trailing_bytes[50] = 0xEE;
        let trailing_bytes = trailing_bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();

        let jsonl = format!(
            "{{\"ts\":1,\"dir\":\"in\",\"report_id\":0,\"hex\":\"{round_trip}\"}}\n\
             \n\
             {{\"ts\":2,\"dir\":\"in\",\"report_id\":0,\"hex\":\"{unmodelled}\"}}\n\
             {{\"ts\":3,\"dir\":\"in\",\"report_id\":0,\"hex\":\"{trailing_bytes}\"}}\n"
        );
        fs::write(dir.join("synthetic.jsonl"), jsonl).unwrap();

        let summary = classify_dir(&dir).expect("dir exists, so this must be Some");
        assert_eq!(summary.checked, 3);
        assert_eq!(summary.round_tripped, 1);
        assert_eq!(summary.trailing_bytes_frames, 1);
        assert_eq!(summary.unmodelled.get(&0x40), Some(&1));
        assert_eq!(summary.device_fail, 0);

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
        assert_eq!(classify_dir(&dir), None);
    }
}
