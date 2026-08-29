# Third-party notices

This repository vendors third-party source under `research/` for reference while porting the
keyboard protocol to Rust. **None of the vendored copies under `research/` are covered by the
repository's LICENCE**, none of it is owned by Iain Brookes, and each item stays under its own
licence.

**This does not mean nothing in `crates/` is ported.** `crates/wh-proto/src/frame.rs` and
`crates/wh-proto/src/cmds.rs` are themselves a Rust port of MIT-licensed source from
`@sparklinkplayjoy/protocol-keyboard` (vendored at `research/proto/`) and, for `cmds.rs`, the same
package's SDK conventions also mirrored in `@sparklinkplayjoy/hid` (vendored at `research/hidpkg/`);
each file's own module documentation names exactly which upstream file it ports. Porting code
carries the licence that covers it: the Sparklink MIT notice reproduced below, under "research/proto
and research/hidpkg", attaches to those two ported files in `crates/wh-proto` as well as to the
vendored copies under `research/`. `LICENSE` section 1 says the same thing; this is the fuller
statement the licence points at.

These notices are reproduced because the licences require it. Do not remove them.

| Directory | Origin | Licence |
|---|---|---|
| `research/aure/` | [AureTrix_driver](https://github.com/BlastHappy82/AureTrix_driver), Ricardo Correia | MIT |
| `research/proto/` | npm `@sparklinkplayjoy/protocol-keyboard` 1.0.7 | MIT |
| `research/hidpkg/` | npm `@sparklinkplayjoy/hid` 1.0.16 | MIT |
| `research/kbdocs/` | npm `@xsyd/keyboard` 1.0.0 | ISC |

Material derived from the Wallhack Terminal web bundle (decoded command tables, deobfuscated bundle
copies) is deliberately **not** committed. See `.gitignore`.

## research/aure

AureTrix_driver, a web based configuration tool for hall effect keyboards. Its own `LICENSE` file is
retained in place at `research/aure/LICENSE` and is reproduced here.

```
MIT License

Copyright (c) 2025 Ricardo Correia

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## research/proto and research/hidpkg

Both are npm packages published by Sparklink Playjoy and both declare `"license": "MIT"` in their
`package.json`. They are the upstream of the SDK embedded in the vendor web configurator and are the
primary porting reference for this project.

Neither package ships a standalone licence file, so the MIT terms below are reproduced as the
licence each package declares.

```
MIT License

Copyright (c) Sparklink Playjoy

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## research/kbdocs

npm `@xsyd/keyboard` 1.0.0, which declares `"license": "ISC"` in its `package.json` and ships no
standalone licence file. The ISC terms it declares are reproduced below.

```
ISC License

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH
REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY
AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT,
INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM
LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR
OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
PERFORMANCE OF THIS SOFTWARE.
```

## A note on accuracy

An earlier version of `research/README.md` recorded `research/aure/` as ISC. That was not baseless:
`research/aure/package.json` declares `"license": "ISC"` and still does. But `research/aure/LICENSE`
is a full MIT licence text, and `research/aure/README.md` also states MIT in its own "License"
section. The upstream project is self-inconsistent about its own licence, not merely misreported
here. We follow `research/aure/LICENSE`, the file, over `package.json`'s field, and record `research/
aure/` as MIT above on that basis. It also recorded `@sparklinkplayjoy/hid` as 1.0.15; the vendored
copy is 1.0.16. Both are corrected here and in that file.

## A note on this project's own dependencies

The crates.io dependencies `wh` itself builds against (`thiserror`, `serde`, `serde_json`, `toml`,
`clap`, `directories`, `hidapi`) are not covered by any entry above; none of their licences require a
notice here. This is stated explicitly rather than left as an unexplained omission: while
redistribution of the Covered Work is forbidden under `LICENSE` section 2, no permissive-licence
attribution obligation triggered by redistributing a binary is currently live for anyone. If that
restriction is ever lifted, this file would need those dependencies' own notices added before
distributing a binary that links them.
