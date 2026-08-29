#!/usr/bin/env bash
# Saves the Windows clipboard into captures/<name>.jsonl, for use straight after
# running copy(window.__wh.jsonl()) in the browser console.
#
# Refuses to write anything that does not look like a capture, so a stale or
# empty clipboard cannot silently produce a file the golden test then trusts.
set -uo pipefail
[ $# -eq 1 ] || { echo "usage: $0 <scenario-name>"; exit 2; }
name="$1"
dest="captures/${name}.jsonl"
mkdir -p captures
tmp="$(mktemp)"
powershell.exe -NoProfile -Command "Get-Clipboard" 2>/dev/null | tr -d '\r' | grep -v '^$' > "$tmp"
lines=$(wc -l < "$tmp")
if [ "$lines" -eq 0 ]; then echo "clipboard is empty. Run copy(window.__wh.jsonl()) first."; rm -f "$tmp"; exit 1; fi
if ! head -1 "$tmp" | grep -q '"dir"'; then
  echo "clipboard does not look like a capture. First line was:"; head -c 120 "$tmp"; echo; rm -f "$tmp"; exit 1
fi
inb=$(grep -c '"dir":"in"' "$tmp" || true)
mv "$tmp" "$dest"
echo "wrote $dest: $lines frames, $inb inbound"
[ "$inb" -eq 0 ] && echo "WARNING: no inbound frames. The shim went in after the page opened the device."
exit 0
