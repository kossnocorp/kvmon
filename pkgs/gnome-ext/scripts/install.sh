#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
extension="$data/gnome-shell/extensions/kvmon@kvmon.local"
binary="$data/kvmon/kvmon-gtk"
cd "$root"
cargo build --release -p kvmon-gtk
mkdir -p "$extension" "$data/kvmon" "$data/dbus-1/services"
install -m755 target/release/kvmon-gtk "$binary"
install -m644 pkgs/gnome-ext/{extension.js,metadata.json,interface.xml} "$extension/"
# Quote/escape the executable for D-Bus Exec parsing, including nonstandard XDG paths.
python3 - "$binary" "$data/dbus-1/services/io.github.kvmon.Monitor.service" <<'PY'
import pathlib
import sys
binary = sys.argv[1].replace('\\', '\\\\').replace('"', '\\"')
pathlib.Path(sys.argv[2]).write_text(
    '[D-BUS Service]\nName=io.github.kvmon.Monitor\nExec="' + binary + '"\n'
)
PY
echo "Installed KVM. Log out and back in after first installation, then run:"
echo "  gnome-extensions enable kvmon@kvmon.local"
