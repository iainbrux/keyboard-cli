# Research artifacts

Third-party source vendored for reference while porting the keyboard protocol to Rust.

| Directory | Origin | License |
|---|---|---|
| `proto/` | npm package `@sparklinkplayjoy/protocol-keyboard` 1.0.7 | MIT (per package.json) |
| `hidpkg/` | npm package `@sparklinkplayjoy/hid` 1.0.16 | MIT (per package.json) |
| `kbdocs/` | npm package `@xsyd/keyboard` 1.0.0 | ISC (per package.json) |
| `aure/` | github.com/BlastHappy82/AureTrix_driver | MIT (per its own LICENSE, (c) Ricardo Correia) |

Files derived from the Wallhack Terminal web bundle (decoded command tables,
deobfuscated bundle copies) are intentionally **not** committed: see `.gitignore`.
All Sparklink Playjoy packages are the upstream of the SDK embedded in
https://terminal.wallhack.com/ and are the primary porting reference.

None of this directory is covered by the repository's LICENCE, and none of it belongs to the
repository owner. Each item stays under its own licence. The required notices, including the full
licence texts, are in `THIRD_PARTY_NOTICES.md` at the repository root. Do not remove them.
