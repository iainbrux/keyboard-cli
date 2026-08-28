# `wh` — Wallhack Keyboard CLI: Design

**Date:** 2026-08-28
**Status:** Approved pending user review
**Scope of this spec:** Phase 1 (foundation) only. Later phases are listed but intentionally not specified.

## Problem

The Wallhack K-001 keyboard is configured exclusively through https://terminal.wallhack.com/, which requires a Chromium browser (it uses a browser-to-device API only Chromium implements). The user wants on-the-fly configuration from a terminal, e.g.:

```
wh set rt --keys "w,a,s,d" --set 0.5
```

No public protocol documentation, vendor SDK, or third-party tooling is known to exist. The protocol must be reverse-engineered from the web configurator.

## Confirmed environment facts

Verified against the user's machine on 2026-08-28:

- Keyboard: **WALLHACK K-001**, USB VID `0x3879`, PID `0x0806`, connected to the Windows host.
- Composite USB device, 4 interfaces: `MI_00` keyboard, `MI_01` mouse, `MI_02` keyboard/consumer/system-control collections, **`MI_03` vendor-defined HID** — the presumed configuration channel (and the only interface a browser's WebHID can open, consistent with the Chromium-only requirement).
- Development shell is **WSL2**. The keyboard is **not visible inside WSL** (no usbipd-win installed; no `/dev/hidraw*`). Windows PowerShell is reachable from WSL for host-side probing.
- Chrome, Edge, and Brave are installed on the Windows side. Chrome will be used for instrumented capture (`--remote-debugging-port`).
- Rust 1.93 in WSL; no Rust toolchain on Windows; mingw-w64 cross toolchain available via apt.

## Key decisions (user-approved)

1. **Runtime target: Windows binary, invoked from WSL.** Cross-compile `x86_64-pc-windows-gnu` from WSL (apt `gcc-mingw-w64-x86-64` + rustup target). A small `wh` shell shim in WSL execs the `.exe`, so the CLI works identically from the zsh prompt and from Windows Terminal. Rationale: usbipd passthrough would detach the keyboard from Windows while attached — unacceptable for the user's only keyboard.
2. **Sequencing: foundation first.** Phase 1 delivers protocol discovery, transport, read-back, and rapid-trigger + actuation writes. Profiles, knob, keymap, RGB, and per-profile OS binding are follow-on phases, each specced after Phase 1's decoder exists.
3. **Discovery: static analysis first, live capture second, USB sniffing in reserve.**
4. **Safety posture: this is the user's only keyboard.** Auto-backup before every write, read-back verification after every write, `--dry-run` on all mutating commands, explicit `wh restore`.

## Architecture

Cargo workspace, four crates. The guiding rule: every reverse-engineered byte-layout fact lives in exactly one crate (`wh-proto`), and all hardware I/O passes through one trait (`Transport`).

### `wh-proto` — protocol codec (pure, no I/O)

- Types: `KeyId`, `Setting` (e.g. `RapidTrigger { sensitivity_mm }`, `ActuationPoint { depth_mm }`), `Command`, `Report`.
- Functions: `encode(Command) -> Vec<Report>` and `decode(&[u8]) -> Result<Event>`.
- Millimetre values are validated against device ranges discovered during RE; out-of-range values are errors here, not at the CLI layer.
- Fully unit-testable with no hardware.

### `wh-device` — transport

- `trait Transport { fn write_report(...); fn read_report(...); fn get_feature(...); fn set_feature(...); }`
- Implementations:
  - `HidTransport` — real device via `hidapi`, opening VID `0x3879` / PID `0x0806`, vendor interface (`MI_03`, confirmed during discovery by usage page).
  - `ReplayTransport` — serves captured traffic from `captures/*.jsonl`; used by all integration tests.
  - `RecordingTransport` — wraps another transport, logs every report exchanged (used by `--trace` and during RE).
- Detects exclusive-access failure (web configurator tab open in Chrome) and returns a dedicated error variant.

### `wh-config` — host-side state

- Config: `~/.config/wh/config.toml` (Linux) / `%APPDATA%\wh\config.toml` (Windows): user key groups, preferences.
- Backups: rolling directory (keep last 20), one TOML snapshot per mutating command.
- Profile files: TOML, human-readable, diffable.

### `wh-cli` — command surface (`clap`)

```
wh dump [--json]                         read full board config
wh get rt --keys <sel>                   read rapid-trigger settings
wh get ap --keys <sel>                   read actuation point
wh set rt --keys <sel> --set <mm>        set RT sensitivity
wh set rt --keys <sel> --off             disable RT
wh set ap --keys <sel> --set <mm>        set actuation depth
wh backup [--to <file>]                  explicit snapshot
wh restore [<file>|--last]               write snapshot back to board
wh keys list                             known key names + groups
wh keys group <name> <sel>               define a user group
```

All mutating commands accept `--dry-run` (print exact report bytes, send nothing).

### Key-selection grammar

One parser shared by every `--keys` flag:

- Comma list: `w,a,s,d`
- Built-in groups: `wasd`, `arrows`, `mods`, `all`
- User groups from config, e.g. `fps`
- Ranges: `a-z`, `f1-f12`
- Negation, applied left to right: `all,!space,!capslock`
- `--pick`: TUI keyboard-map picker as an alternative to the flag

## Safety model

1. **Auto-backup:** every mutating command first reads the full board config and snapshots it to the backup dir. Only then does it write.
2. **Read-back verification:** after writing, the setting is read back from the board. Output reports the board's actual value. A mismatch is reported as an error with both values; the backup is retained; the command exits non-zero.
3. **`wh restore --last`** re-applies the most recent snapshot.
4. **No-op smoke test:** `wh selftest` writes one setting to its *current* value and verifies read-back — proves the write path without changing state.
5. First-ever capture of a full config read is stored both as a golden test fixture and as backup #0.

## Discovery workflow — COMPLETED 2026-08-28 (static analysis)

Static analysis of the terminal.wallhack.com bundle succeeded beyond expectations. Key findings (full detail to live in `docs/protocol.md`; source artifacts under `research/`):

- **Transport confirmed: WebHID.** No WebUSB or Web Serial anywhere in the bundle. USB sniffing fallback is moot.
- **The firmware platform is Sparklink Playjoy** (ODM shared with Chilkey, RK, AJAZZ, FL Esports, …), and the vendor SDK embedded in Wallhack's bundle is an obfuscated copy of **`@sparklinkplayjoy/protocol-keyboard` — public, MIT-licensed, full TypeScript source on npm**. The protocol does not need reverse-engineering from scratch; it needs porting from readable MIT source (`research/proto/package/src/`).
- **Framing:** reportId 0, 64-byte zero-padded reports. `byte0=0x5C` magic, `byte1=len`, `byte2=cmd`, `byte3=crc` where `crc = (0x35 + 0x5C + len + cmd + lastPayloadByte) & 0xFF` — cross-validated between the deobfuscated bundle and upstream source.
- **Values are mm × 1000, little-endian u16.** Defaults observed: 4.0 mm max travel, 2.0 mm AP, 0.1 mm RT, 0.01 mm step.
- **Per-key settings** are 4-byte records `[hidUsage, layoutId, lo, hi]` batched under cmd `0x23`; layout IDs: `0x04` AP, `0x14` RT press, `0x15` RT release, `0x08` working mode (RT enable), `0x16/0x17` safe zone.
- **Global AP** via cmd `0x29`. Polling rate, profiles (`setConfig`), SOCD/DKS/advanced keys, lighting, and bootloader commands are all mapped (91 command templates decoded in `research/blob.json`).
- **The board is self-describing:** `READ_DEFKEY_MATRIX` (cmd `0x2B`) returns the full 6×21 HID-usage matrix — no per-model layout file needed. Key addressing is standard USB HID usage codes.
- Second VID possible: bundle also matches `0x1CAA:0x0806` (and `0x1CAA:0xFFF6/FFF8` for bootloader); our board enumerates as `0x3879:0x0806`. Match on either.
- The vendor collection is usage page `0xFFA0`, usage `0x01` — `wh-device` must select the HID interface by usage page, not by interface number.

**Remaining live-capture step (reduced scope):** everything above is static analysis, untested against hardware. Before first write: capture the web app's actual traffic for a handful of actions (Chrome `--remote-debugging-port` + `HIDDevice.prototype` logging shim) to confirm framing/CRC/scale byte-for-byte, and store as `captures/*.jsonl` golden fixtures. Cross-check rule stands: captures are authoritative.

**Known constraint:** WebHID holds the interface exclusively. The web configurator and `wh` cannot both hold the device; `wh` must detect and name this conflict.

**Licensing note:** implementation ports from the MIT upstream (`research/proto/`, attribution in-repo). Vendor-bundle-derived artifacts (`research/vendor-bundle/`, `deob2.js`, `strings.json`, `blob.json`) are kept locally for reference but gitignored, not distributed.

## Testing

- **Unit (`wh-proto`):** decode every captured report; re-encode; assert byte-identical round-trip. Encoding "set RT 0.5 on W" must equal the bytes Chrome sent for that action.
- **Integration (`wh-cli` + `wh-device`):** full command → bytes pipeline against `ReplayTransport`. No hardware in CI.
- **Hardware smoke (manual):** `wh selftest` as described above.

## Error handling

| Case | Behaviour |
|---|---|
| Device not found | "Is the keyboard plugged in? Is the web configurator tab open?" |
| Exclusive-access conflict | Named explicitly (close the browser tab), not a raw hidapi error |
| Read-back mismatch | Report expected vs. actual, keep backup, exit non-zero |
| Out-of-range mm value | Rejected in `wh-proto` with the valid range in the message |
| Unknown key name | Error listing near-matches and pointing at `wh keys list` |

## Phase plan

- **Phase 1 (this spec):** discovery harness, `wh-proto`/`wh-device` foundation, `wh dump`, `wh get/set rt`, `wh get/set ap`, backup/restore, key-selection grammar, `--pick` TUI picker.
- **Phase 2+ (separately specced, reusing the decoder):** profiles (`wh profile select/edit/save/apply`), knob configuration, keymap remapping, RGB, OS-layout binding.
- **Note on per-profile macOS binding:** if firmware stores OS mode as a single board-wide value, per-profile behaviour will be emulated host-side — `wh profile select 2` writes profile 2's recorded OS mode before switching. Same observable behaviour; decided in Phase 2 once discovery shows what the firmware stores.

## Risks

| Risk | Mitigation |
|---|---|
| ~~JS bundle heavily obfuscated~~ | Retired: MIT-licensed upstream source found (`@sparklinkplayjoy/protocol-keyboard`) |
| ~~Transport is WebUSB/Web Serial~~ | Retired: WebHID confirmed in bundle |
| Wallhack firmware diverges from upstream SDK | Live capture cross-check before first write; captures are authoritative over the port |
| Bad write leaves board in bad state | Auto-backup + restore; no-op selftest before first real write; vendor web app remains the recovery tool of last resort |
| Exclusive access confusion | Dedicated error variant + message |
| Firmware update changes protocol | Protocol doc records firmware version tested against; `wh dump` includes firmware version if readable |

## Open questions

Resolved by static analysis (see Discovery section): transport = WebHID; output reports, reportId 0, 64 bytes; values = mm×1000 LE u16; key addressing = USB HID usage codes.

Still open, to resolve during live capture / hardware smoke testing:

1. Is config persisted to flash automatically on write, or is a "commit" command required?
2. Does the upstream SDK version match the K-001's firmware exactly, or has Wallhack diverged? (Byte-for-byte capture comparison answers this.)
3. Response timing/ordering: does every command ack, and how should `wh-device` frame request/response matching?
