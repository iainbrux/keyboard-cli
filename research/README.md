# Research artifacts

Third-party source vendored for reference while porting the keyboard protocol to Rust.

| Directory | Origin | License |
|---|---|---|
| `proto/` | npm package `@sparklinkplayjoy/protocol-keyboard` 1.0.7 | MIT (per package.json) |
| `hidpkg/` | npm package `@sparklinkplayjoy/hid` 1.0.15 | MIT (per package.json) |
| `kbdocs/` | github.com/sparklinkplayjoy/keyboard-docs | see repo |
| `aure/` | github.com/AureTrix-Solutions/AureTrix_driver | ISC |

Files derived from the Wallhack Terminal web bundle (decoded command tables,
deobfuscated bundle copies) are intentionally **not** committed: see `.gitignore`.
All Sparklink Playjoy packages are the upstream of the SDK embedded in
https://terminal.wallhack.com/ and are the primary porting reference.
