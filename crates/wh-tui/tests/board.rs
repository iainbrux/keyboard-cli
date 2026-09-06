mod support;
use support::*;
use wh_device::replay::ReplayTransport;
use wh_device::session::Session;
use wh_tui::board::{ap_keysets, global_ap, BoardModel, GlobalValue};

#[test]
fn board_model_reads_the_same_wire_sequence_as_snapshot_from_device() {
    let lines = build_script(); // sync, profile 0, global travel, matrix, six reads per key
    let t = ReplayTransport::from_jsonl(&lines.join("\n")).unwrap();
    let mut s = Session::new(t);
    let m = BoardModel::read(&mut s).unwrap();
    assert_eq!(m.firmware, "V1.0.0.001");
    assert_eq!(m.profile.one_based(), 1);
    assert_eq!(m.keys.len(), 2);
    assert_eq!(m.key(0x1A).unwrap().ap.0, 1200);
    assert!(s.into_inner().finished(), "script not fully consumed");
}

#[test]
fn global_ap_agrees_only_over_keys_outside_ap_keysets() {
    use wh_device::ops::KeySettings;
    use wh_proto::cmds::Mode;
    use wh_proto::value::Um;

    let outside = |usage: u8, ap: u16| KeySettings {
        usage,
        ap: Um(ap),
        mode: Mode::from_value(0x0010),
        rt_press: Um(0),
        rt_release: Um(0),
        ap_keyset: 0,
        rt_keyset: 0,
    };

    // Two keys agreed at 2.00mm outside any keyset: Agreed.
    let keys = vec![outside(0x1A, 2000), outside(0x04, 2000)];
    assert_eq!(global_ap(&keys), GlobalValue::Agreed(Um(2000)));

    // One of them moved into an ap keyset with a different value: the outside one still agrees.
    let mut in_keyset = outside(0x04, 2000);
    in_keyset.ap = Um(1200);
    in_keyset.ap_keyset = 1;
    let keys = vec![outside(0x1A, 2000), in_keyset.clone()];
    assert_eq!(global_ap(&keys), GlobalValue::Agreed(Um(2000)));

    // Two disagreeing outside keys: Mixed.
    let keys = vec![outside(0x1A, 2000), outside(0x04, 1500)];
    assert_eq!(global_ap(&keys), GlobalValue::Mixed);

    // All keys in keysets: NoneOutside.
    let mut a = outside(0x1A, 2000);
    a.ap_keyset = 1;
    let mut b = outside(0x04, 1200);
    b.ap_keyset = 2;
    let keys = vec![a, b];
    assert_eq!(global_ap(&keys), GlobalValue::NoneOutside);
}

#[test]
fn ap_keysets_group_by_index_and_sort() {
    use wh_device::ops::KeySettings;
    use wh_proto::cmds::Mode;
    use wh_proto::value::Um;

    let key = |usage: u8, ap_keyset: u16| KeySettings {
        usage,
        ap: Um(2000),
        mode: Mode::from_value(0x0010),
        rt_press: Um(0),
        rt_release: Um(0),
        ap_keyset,
        rt_keyset: 0,
    };

    // Three keys, two sharing ap_keyset 2, one in 1: expect [{1,[..]}, {2,[..,..]}].
    let keys = vec![key(0x1A, 2), key(0x04, 1), key(0x16, 2)];
    let groups = ap_keysets(&keys);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].index, 1);
    assert_eq!(groups[0].members, vec![0x04]);
    assert_eq!(groups[1].index, 2);
    assert_eq!(groups[1].members, vec![0x1A, 0x16]);
}
