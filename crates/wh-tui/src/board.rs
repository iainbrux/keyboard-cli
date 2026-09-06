//! The board read on `wh tui`'s open: everything the tabs draw from, read once and held for the
//! session's lifetime. `wh` caches no device state anywhere else; this is the one place that
//! does, because a TUI redrawing every frame cannot afford a fresh HID roundtrip per keystroke.

use wh_device::ops::{self, KeySettings};
use wh_device::session::Session;
use wh_device::transport::{DeviceError, Transport};
use wh_proto::cmds::{DefKeyRow, GlobalTravel, ProfileNumber};
use wh_proto::value::Um;

/// The product name, not per-board data: the vendor's own device line and the ADVANCED > DEVICE
/// sub-tab both show it, and it never varies with which board is plugged in.
pub const DEVICE_NAME: &str = "WALLHACK K-001";

pub struct BoardModel {
    pub serial: String,
    pub firmware: String,
    pub profile: ProfileNumber,
    pub global: GlobalTravel,
    pub rows: Vec<DefKeyRow>,
    pub keys: Vec<KeySettings>, // matrix order, same flatten as ops::read_matrix
}

impl BoardModel {
    /// Reads the whole board: device info, profile, global travel, the matrix, then every key's
    /// settings in matrix flatten order. Same wire order as `snapshot_from_device`.
    pub fn read<T: Transport>(s: &mut Session<T>) -> Result<Self, DeviceError> {
        let info = ops::device_info(s)?;
        let profile = ops::profile(s)?;
        let global = ops::global_travel(s)?;
        let rows = ops::read_matrix_rows(s)?;

        let mut usages = Vec::new();
        for row in &rows {
            for (_, usage) in &row.keys {
                if !usages.contains(usage) {
                    usages.push(*usage);
                }
            }
        }

        let mut keys = Vec::with_capacity(usages.len());
        for usage in usages {
            keys.push(ops::read_key_settings(s, usage)?);
        }

        Ok(Self {
            serial: info.serial,
            firmware: info.firmware,
            profile,
            global,
            rows,
            keys,
        })
    }

    pub fn key(&self, usage: u8) -> Option<&KeySettings> {
        self.keys.iter().find(|k| k.usage == usage)
    }
}

#[derive(Debug, PartialEq)]
pub enum GlobalValue<T> {
    Agreed(T),
    Mixed,
    NoneOutside,
}

/// Folds `keys` outside any AP keyset (`ap_keyset == 0`) into a single verdict: `Agreed` if every
/// such key shares one AP, `Mixed` if they disagree, `NoneOutside` if there are none. A key's own
/// AP keyset membership never affects the fold, only whether it counts.
pub fn global_ap(keys: &[KeySettings]) -> GlobalValue<Um> {
    let mut outside = keys.iter().filter(|k| k.ap_keyset == 0).map(|k| k.ap);
    let Some(first) = outside.next() else {
        return GlobalValue::NoneOutside;
    };
    if outside.all(|ap| ap == first) {
        GlobalValue::Agreed(first)
    } else {
        GlobalValue::Mixed
    }
}

/// Same fold as `global_ap`, but over rapid-trigger-enabled keys outside any RT keyset
/// (`rt_keyset == 0`), pairing press and release sensitivity. `NoneOutside` means global rapid
/// trigger is off: no key outside a keyset has rapid trigger enabled at all.
pub fn global_rt(keys: &[KeySettings]) -> GlobalValue<(Um, Um)> {
    let mut outside = keys
        .iter()
        .filter(|k| k.rt_keyset == 0 && k.rt_enabled())
        .map(|k| (k.rt_press, k.rt_release));
    let Some(first) = outside.next() else {
        return GlobalValue::NoneOutside;
    };
    if outside.all(|pair| pair == first) {
        GlobalValue::Agreed(first)
    } else {
        GlobalValue::Mixed
    }
}

pub struct KeysetView {
    pub index: u16,
    pub members: Vec<u8>,
}

/// Groups `keys` by a non-zero keyset index, selected by `selector`, sorted by index ascending.
/// Shared fold behind `ap_keysets` and `rt_keysets`, which differ only in which field they read.
fn keysets_by(keys: &[KeySettings], selector: impl Fn(&KeySettings) -> u16) -> Vec<KeysetView> {
    let mut groups: Vec<KeysetView> = Vec::new();
    for k in keys {
        let index = selector(k);
        if index == 0 {
            continue;
        }
        match groups.iter_mut().find(|g| g.index == index) {
            Some(g) => g.members.push(k.usage),
            None => groups.push(KeysetView {
                index,
                members: vec![k.usage],
            }),
        }
    }
    groups.sort_by_key(|g| g.index);
    groups
}

pub fn ap_keysets(keys: &[KeySettings]) -> Vec<KeysetView> {
    keysets_by(keys, |k| k.ap_keyset)
}

pub fn rt_keysets(keys: &[KeySettings]) -> Vec<KeysetView> {
    keysets_by(keys, |k| k.rt_keyset)
}

/// A `BoardModel` literal with two keys and no wire, for `app`'s unit tests: those exercise
/// `draw`, not the read path, so they need a model to hold rather than a `Session` to read one
/// from. 'a' (0x04) carries a non-zero `rt_keyset` and a `rt_press` distinct from 'w's, so a test
/// that switches to the RapidTrigger tab can tell the two keys' matrix cells apart.
#[cfg(test)]
pub(crate) fn test_fixture() -> BoardModel {
    use wh_proto::cmds::Mode;

    BoardModel {
        serial: "SNTUITEST0000001".to_string(),
        firmware: "V1.0.0.001".to_string(),
        profile: ProfileNumber::from_wire_index(0).unwrap(),
        global: GlobalTravel {
            travel: Um(500),
            press_dead: Um(200),
            release_dead: Um(200),
        },
        rows: vec![
            DefKeyRow {
                row: 0,
                keys: vec![(0, 0x1A)],
            },
            DefKeyRow {
                row: 1,
                keys: vec![(0, 0x04)],
            },
        ],
        keys: vec![
            KeySettings {
                usage: 0x1A,
                ap: Um(1200),
                mode: Mode::from_value(0x0010),
                rt_press: Um(500),
                rt_release: Um(500),
                ap_keyset: 0,
                rt_keyset: 0,
            },
            KeySettings {
                usage: 0x04,
                ap: Um(1500),
                mode: Mode::from_value(0x0010),
                rt_press: Um(300),
                rt_release: Um(300),
                ap_keyset: 0,
                rt_keyset: 1,
            },
        ],
    }
}
