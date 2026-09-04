//! Throwaway reviewer probe: is `wh keyset create ap --keys all` guarded?
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
    wh_proto::frame::frame(cmd | wh_proto::frame::REPLY_BIT, payload).unwrap()
}
fn defkey_payload(row_a: u8, row_b: u8, a: Option<u8>, b: Option<u8>) -> Vec<u8> {
    let mut p = vec![0u8; 45];
    p[1] = row_a;
    if let Some(u) = a { p[2] = u; }
    p[23] = row_b;
    if let Some(u) = b { p[24] = u; }
    p
}
fn matrix_lines_wasd() -> Vec<String> {
    let mut lines = Vec::new();
    for (i, &(a, b)) in [(0u8, 1u8), (2u8, 3u8), (4u8, 5u8)].iter().enumerate() {
        lines.push(out_line(&cmds::read_defkey_rows(a, b)));
        let payload = match i {
            0 => defkey_payload(a, b, Some(0x1A), Some(0x04)),
            1 => defkey_payload(a, b, Some(0x16), Some(0x07)),
            _ => defkey_payload(a, b, None, None),
        };
        lines.push(in_line(&reply(cmds::cmd::DEFKEY, &payload)));
    }
    lines
}
fn layout_read_lines(usage: u8, layout_id: u8, value: u16) -> Vec<String> {
    vec![
        out_line(&cmds::read_key_layout(usage, layout_id)),
        in_line(&reply(cmds::cmd::KEY, &[0x00, usage, layout_id, (value & 0xFF) as u8, (value >> 8) as u8])),
    ]
}
#[test]
fn probe_keyset_create_over_all_is_unguarded() {
    let mut lines = matrix_lines_wasd();
    lines.extend(matrix_lines_wasd());
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 2)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    let path = std::env::temp_dir().join(format!("wh-probe2-{}.jsonl", std::process::id()));
    std::fs::write(&path, lines.join("\n")).unwrap();
    let cfg = std::env::temp_dir().join(format!("wh-probe2-cfg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cfg);
    let out = Command::new(env!("CARGO_BIN_EXE_wh"))
        .env("WH_REPLAY", &path)
        .env("XDG_CONFIG_HOME", &cfg)
        .args(["keyset", "create", "ap", "--keys", "all", "--value", "1.50"])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    println!("STATUS {:?}", out.status);
    println!("STDOUT >>>{}<<<", String::from_utf8_lossy(&out.stdout));
    println!("STDERR >>>{}<<<", String::from_utf8_lossy(&out.stderr));
    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&cfg);
}
