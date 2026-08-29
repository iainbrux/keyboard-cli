# Third-party notices

This repository vendors third-party source under `research/` for reference while porting the
keyboard protocol to Rust. **None of it is covered by the repository's LICENCE**, none of it is
owned by Iain Brookes, and each item stays under its own licence.

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

An earlier version of `research/README.md` recorded `research/aure/` as ISC. It is MIT, per the
`LICENSE` file in that directory, copyright Ricardo Correia. It also recorded `@sparklinkplayjoy/hid`
as 1.0.15; the vendored copy is 1.0.16. Both are corrected here and in that file.
