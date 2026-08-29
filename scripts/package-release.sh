#!/usr/bin/env bash
# Builds the release archive that actually reaches a recipient of `wh.exe`: the binary itself,
# plus the licence, the attribution notice, and both third-party licence files. Apache-2.0 section
# 4(a) requires the recipient of a distributed binary to receive a copy of the licence; 4(d)
# requires them to receive the NOTICE contents. Neither reaches anyone from a bare .exe download,
# so this script is what actually satisfies both, rather than leaving it to whoever cuts a release
# to remember by hand.
#
# Run it from anywhere; it resolves the repository root from its own location. Reproducible: given
# the same source tree, it produces the same archive contents (the zip's internal file order and
# metadata may differ run to run, but every file inside is byte-identical). Requires only `cargo`,
# `bash`, and `python3` (used for zip creation, since Windows Explorer opens `.zip` natively and no
# `zip` binary is assumed to be installed).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

# `-n ... p` only prints a line that actually matched and substituted; a plain `s///` would pass
# a non-matching line (for example `version.workspace = true`, if this crate ever moves to that,
# as it already has for `edition` and `license`) through unchanged, which is non-empty and would
# slip past a bare emptiness check below, becoming part of the archive's own file name.
VERSION="$(grep -m1 '^version' crates/wh-cli/Cargo.toml | sed -n -E 's/^version = "(.*)"$/\1/p')"
if [ -z "$VERSION" ]; then
    echo "package-release: could not read a plain version = \"...\" from crates/wh-cli/Cargo.toml" \
         "(if it now reads \"version.workspace = true\", update this script to read the version" \
         "from the workspace Cargo.toml instead)" >&2
    exit 1
fi

TARGET="x86_64-pc-windows-gnu"
ARCHIVE_NAME="wh-${VERSION}-${TARGET}"
DIST_DIR="$ROOT_DIR/dist"
STAGE_DIR="$DIST_DIR/$ARCHIVE_NAME"
ARCHIVE_PATH="$DIST_DIR/$ARCHIVE_NAME.zip"

echo "package-release: building the release binary for $TARGET"
cargo build --release --workspace --target "$TARGET"

EXE="$ROOT_DIR/target/$TARGET/release/wh.exe"
if [ ! -f "$EXE" ]; then
    echo "package-release: expected $EXE after the build, but it is not there" >&2
    exit 1
fi

rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR"

cp "$EXE" "$STAGE_DIR/wh.exe"
cp "$ROOT_DIR/LICENSE" "$STAGE_DIR/LICENSE"
cp "$ROOT_DIR/NOTICE" "$STAGE_DIR/NOTICE"
cp "$ROOT_DIR/THIRD_PARTY_LICENSES.md" "$STAGE_DIR/THIRD_PARTY_LICENSES.md"
cp "$ROOT_DIR/THIRD_PARTY_NOTICES.md" "$STAGE_DIR/THIRD_PARTY_NOTICES.md"

# A short pointer, not the repository's own README.md: that file is written for someone building
# from source, not someone who just downloaded an .exe. This is the whole content a recipient of
# the archive gets before they go looking for more.
cat > "$STAGE_DIR/README.txt" <<EOF
wh ${VERSION}, a command line tool for the Wallhack K-001 hall effect keyboard

This archive is everything you need to run wh.exe from a WSL or a Windows shell. Full
documentation, source, and the issue tracker are at:

    https://github.com/iainbrux/keyboard-cli

What's in this archive:

    wh.exe                    the tool itself (x86_64-pc-windows-gnu)
    LICENSE                   this project's own licence (Apache-2.0)
    NOTICE                    attribution this project's licence requires you to keep, if you
                               redistribute this archive or a derivative of it
    THIRD_PARTY_LICENSES.md   licence texts for wh's own crates.io dependencies compiled into
                               wh.exe, plus a section on the Rust standard library's own runtime and
                               the mingw-w64 C runtime, which are not crates.io dependencies but are
                               still compiled in; not a claim to have traced every object file the
                               linker pulled in
    THIRD_PARTY_NOTICES.md    notices for research/ reference material never compiled into wh.exe,
                               except the Sparklink port, which is compiled in and is covered in
                               THIRD_PARTY_LICENSES.md instead, not here

If you redistribute wh.exe, or a modified build of it, keep this archive's LICENSE, NOTICE, and the
two THIRD_PARTY_*.md files with it. That obligation follows the binary to whoever you give it to,
not only to this repository.
EOF

mkdir -p "$DIST_DIR"
rm -f "$ARCHIVE_PATH"
python3 - "$DIST_DIR" "$ARCHIVE_NAME" "$ARCHIVE_PATH" <<'PYEOF'
import os
import sys
import zipfile

dist_dir, archive_name, archive_path = sys.argv[1], sys.argv[2], sys.argv[3]
stage_dir = os.path.join(dist_dir, archive_name)

with zipfile.ZipFile(archive_path, "w", zipfile.ZIP_DEFLATED) as zf:
    for root, _dirs, files in os.walk(stage_dir):
        for name in sorted(files):
            full = os.path.join(root, name)
            arcname = os.path.join(archive_name, os.path.relpath(full, stage_dir))
            info = zipfile.ZipInfo(arcname)
            # Fixed timestamp, not the moment this ran: the only thing that should differ
            # between two runs against the same source tree is nothing at all. Contents, not
            # just names, are reproducible without this too (every file above is a build
            # artefact or a checked-in text file, itself deterministic); this just extends that
            # to the archive's own bytes.
            info.date_time = (1980, 1, 1, 0, 0, 0)
            info.compress_type = zipfile.ZIP_DEFLATED
            # create_system 3 (Unix) is what makes an unzip tool honour external_attr's
            # permission bits at all; Windows itself ignores both, since it decides a file is
            # runnable by its .exe extension, not a permission bit, but a WSL user extracting
            # this with unzip or python3 -m zipfile should still get a runnable wh.exe without
            # a manual chmod.
            info.create_system = 3
            info.external_attr = (0o644 << 16) if name != "wh.exe" else (0o755 << 16)
            with open(full, "rb") as f:
                zf.writestr(info, f.read())
PYEOF

rm -rf "$STAGE_DIR"

echo "package-release: wrote $ARCHIVE_PATH"
echo "package-release: contents:"
python3 - "$ARCHIVE_PATH" <<'PYEOF'
import sys
import zipfile

with zipfile.ZipFile(sys.argv[1]) as zf:
    for info in sorted(zf.infolist(), key=lambda i: i.filename):
        print(f"  {info.file_size:>10}  {info.filename}")
PYEOF
