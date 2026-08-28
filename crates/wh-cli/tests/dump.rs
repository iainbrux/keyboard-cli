//! End-to-end test of `wh dump --json` over a replay script, exercising the full
//! `snapshot_from_device` pipeline (SYNC, global travel, the key matrix, then per-key reads)
//! without a physical keyboard, via the `WH_REPLAY` seam.

use std::process::Command;
use wh_device::replay::hex;
use wh_proto::cmds::{self, layout};

fn out_line(bytes: &[u8; 64]) -> String {
    format!("{{\"dir\":\"out\",\"hex\":\"{}\"}}", hex(bytes))
}

fn in_line(bytes: &[u8; 64]) -> String {
    format!("{{\"dir\":\"in\",\"hex\":\"{}\"}}", hex(bytes))
}

fn reply(cmd: u8, payload: &[u8]) -> [u8; 64] {
    wh_proto::frame::frame(cmd, payload).unwrap()
}

/// A DEFKEY reply payload for one row pair: `[rw, row_a, 21 usages, row_b, 21 usages]`, with
/// at most the first column of each row populated. `None` leaves a row empty (no keys), which
/// is what the second and third row pairs of this two-key board need.
fn defkey_payload(row_a: u8, row_b: u8, a_col0: Option<u8>, b_col0: Option<u8>) -> Vec<u8> {
    let mut payload = vec![0u8; 45];
    payload[1] = row_a;
    if let Some(u) = a_col0 {
        payload[2] = u;
    }
    payload[23] = row_b;
    if let Some(u) = b_col0 {
        payload[24] = u;
    }
    payload
}

/// One key's [AP, MODE, RT_PRESS, RT_RELEASE] roundtrips, in the exact order
/// `ops::read_key_settings` sends them.
fn key_settings_lines(
    usage: u8,
    ap: u16,
    mode: u16,
    rt_press: u16,
    rt_release: u16,
) -> Vec<String> {
    let mut lines = Vec::new();
    for (layout_id, value) in [
        (layout::AP, ap),
        (layout::MODE, mode),
        (layout::RT_PRESS, rt_press),
        (layout::RT_RELEASE, rt_release),
    ] {
        lines.push(out_line(&cmds::read_key_layout(usage, layout_id)));
        let payload = [
            0x00,
            usage,
            layout_id,
            (value & 0xFF) as u8,
            (value >> 8) as u8,
        ];
        lines.push(in_line(&reply(cmds::cmd::KEY, &payload)));
    }
    lines
}

/// Composes, in order, exactly the frames `snapshot_from_device` sends against a two-key
/// board ('w' at usage 0x1A, 'a' at usage 0x04): the SYNC request and info reply, the global
/// travel DB read and reply, the three DEFKEY roundtrips that make up the key matrix (only the
/// first row pair carries keys), then four KEY reads and replies per key. Built with
/// `wh_proto::cmds` encoders, not hand-written hex, so the test breaks if an encoder changes
/// rather than silently drifting from it.
fn build_script() -> String {
    let mut lines = Vec::new();

    // SYNC: device_info
    lines.push(out_line(&cmds::sync()));
    let mut sync_payload = vec![0u8; 60];
    sync_payload[9..25].copy_from_slice(b"SNDUMPTEST000001");
    sync_payload[26..36].copy_from_slice(b"V1.0.0.001");
    lines.push(in_line(&reply(cmds::cmd::SYNC, &sync_payload)));

    // DB read: global_travel
    lines.push(out_line(&cmds::read_global_travel()));
    let db_payload = [0x00, 0, 0, 0xF4, 0x01, 0xC8, 0x00, 0xC8, 0x00]; // 500/200/200 um
    lines.push(in_line(&reply(cmds::cmd::DB, &db_payload)));

    // 3 DEFKEY roundtrips: only the first row pair carries keys, 'w' then 'a'.
    let row_pairs = [(0u8, 1u8), (2u8, 3u8), (4u8, 5u8)];
    for (i, &(a, b)) in row_pairs.iter().enumerate() {
        lines.push(out_line(&cmds::read_defkey_rows(a, b)));
        let payload = if i == 0 {
            defkey_payload(a, b, Some(0x1A), Some(0x04)) // row a col0 = 'w', row b col0 = 'a'
        } else {
            defkey_payload(a, b, None, None)
        };
        lines.push(in_line(&reply(cmds::cmd::DEFKEY, &payload)));
    }

    // Per-key reads, in matrix order: 'w' (0x1A) then 'a' (0x04).
    lines.extend(key_settings_lines(0x1A, 1200, 0x20, 500, 500));
    lines.extend(key_settings_lines(0x04, 1500, 0x00, 0, 0));

    lines.join("\n")
}

#[test]
fn dump_json_via_replay() {
    let script = build_script();
    let path = std::env::temp_dir().join(format!("wh-dump-{}.jsonl", std::process::id()));
    std::fs::write(&path, script).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_wh"))
        .env("WH_REPLAY", &path)
        .env("XDG_CONFIG_HOME", std::env::temp_dir())
        .args(["dump", "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["serial"], "SNDUMPTEST000001");
    assert_eq!(v["firmware"], "V1.0.0.001");
    assert_eq!(v["global"]["travel_mm"], 0.5);
    assert_eq!(v["keys"][0]["name"], "w");
    assert_eq!(v["keys"][0]["rt"], true);
    assert_eq!(v["keys"][1]["name"], "a");
    assert_eq!(v["keys"][1]["rt"], false);

    std::fs::remove_file(path).unwrap();
}
