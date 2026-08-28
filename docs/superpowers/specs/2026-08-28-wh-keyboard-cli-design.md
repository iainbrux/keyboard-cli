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

## Discovery workflow (precedes protocol implementation)

1. **Static analysis.** Fetch and prettify the JS bundles from terminal.wallhack.com. Locate WebHID call sites (`requestDevice`, `sendReport`, `sendFeatureReport`, `receiveFeatureReport`). Extract opcodes and field encodings into a first-draft `docs/protocol.md`.
2. **Live capture.** Launch Windows Chrome with `--remote-debugging-port=9222`; inject a shim over `HIDDevice.prototype` methods logging direction, report ID, bytes, timestamp. User performs single-variable changes in the web UI (RT on W → 0.5, then 0.6; AP → 1.2; …). Each diff isolates one field.
3. **Cross-check.** Captures are authoritative; the protocol doc is corrected to match observed bytes.
4. Captures stored as `captures/*.jsonl` in-repo (golden fixtures).

**Fallback:** if the bundle is obfuscated beyond use *and* the shim cannot hook the transport (e.g. WebUSB in a worker), install USBPcap + Wireshark on Windows (admin required) and sniff at the wire level.

**Known constraint:** WebHID typically holds the interface exclusively. The web configurator and `wh` cannot both hold the device; `wh` must detect and name this conflict.

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
| JS bundle heavily obfuscated | Live capture still yields ground truth; USBPcap in reserve |
| Transport is WebUSB/Web Serial, not WebHID | Shim strategy adjusts (`navigator.usb`/`serial` hooks); wire sniffing as fallback |
| Bad write leaves board in bad state | Auto-backup + restore; no-op selftest before first real write; vendor web app remains the recovery tool of last resort |
| Exclusive access confusion | Dedicated error variant + message |
| Firmware update changes protocol | Protocol doc records firmware version tested against; `wh dump` includes firmware version if readable |

## Open questions (resolved during discovery, before implementation)

1. Which transport does the web app actually use (WebHID assumed)?
2. Feature reports vs. output reports? Report IDs and sizes?
3. mm-value encoding (scale, offset, lookup)?
4. Key addressing scheme (matrix position, HID usage, vendor index)?
5. Is config persisted to flash automatically on write, or is a "commit" command required?
