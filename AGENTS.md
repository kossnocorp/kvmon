# kvmon

Rust desktop KVM switcher. Start with the Dell U4025QW; keep monitor control
extensible. macOS is a native menu-bar app; Fedora gets a GTK app and GNOME
extension. Work on each frontend independently, sharing semantics and UI copy.

## Layout

- `Cargo.toml`: workspace, Rust members under `pkgs/`.
- `pkgs/core`: platform-neutral types, backend traits, monitor profiles, UI copy,
  and shared state/switching behavior. No AppKit, GTK, or platform I/O dependencies.
- `pkgs/macos`: macOS app and monitor-control backend.
- `pkgs/gtk`: Fedora GTK app and Linux monitor-control backend.
- `pkgs/gnome-ext`: GNOME Shell extension (GJS); not a Cargo member unless it has
  a Rust companion. Define a small IPC boundary if it needs Rust functionality.

Core, macOS, and the Linux service/GNOME extension are implemented.
Use `pkgs/` rather than root-level package directories.

## UX and behavior

Native menu with one item per configured computer, a checkmark on the observed
active input, a separator, and Quit. Initial computers: MacBook and Fedora PC.
Clicking an inactive item selects that monitor input; clicking active is a no-op.
Do not add a main window or webview for the macOS MVP.

- macOS desktop UI is **AppKit**, not UIKit; prefer Rust `objc2` /
  `objc2-app-kit` bindings for `NSStatusItem`, `NSMenu`, and `NSMenuItem`.
- Keep blocking DDC/process I/O off the UI thread; serialize monitor operations.
- Refresh on startup and menu open; recover after disconnect/reconnect or sleep.
- Represent unavailable/unknown/read-error states explicitly. Never show an
  unverified selection as confirmed; switching may disconnect the controlling host.
- Surface failures without crashing. Keep the normal menu minimal.
- Computer labels/input mappings belong in shared profiles/configuration, not
  frontend logic. Separate display identity, input value, and friendly label.

## Confirmed hardware and commands

Monitor: **Dell UltraSharp 40 Curved Thunderbolt Hub Monitor — U4025QW**.
DDC/CI VCP `0x60` selects video input:

| Computer / port | Hex | Decimal |
| --- | --- | --- |
| MacBook / Thunderbolt | `0x19` | 25 |
| Fedora PC / DisplayPort-1 | `0x0f` | 15 |
| HDMI-1 (advertised; unused) | `0x11` | 17 |

Known working commands from the user:

```sh
# macOS: locate monitor, then switch to PC
m1ddc display list
m1ddc display 1 set input 15
# Fedora: inspect, then switch to MacBook
sudo ddcutil detect
sudo ddcutil getvcp 0x60
sudo ddcutil --bus=8 setvcp 0x60 0x19
```

Observed macOS UUID: `1D5ED280-77FB-4355-968B-A6CDCE617E83`.
Linux EDID: manufacturer `DEL`, model `DELL U4025QW`, product `0x4310`,
serial `7P3G684`; observed connector `card1-DP-1`, bus `/dev/i2c-8`.
Indices, buses, and identifiers above are discovery evidence, not portable defaults.
macOS also lists an “Unknown Display”; do not select the first display blindly.
Capabilities advertise `60(19 0F 11)`; ddcutil's unrelated `0x66` parse error does
not prevent input switching.

`MonitorBackend` defines discovery, reading input, and selecting input. macOS uses
an `m1ddc` subprocess adapter with UUID selection and timeouts. Verified `get input`
returns `6425` (`0x1919`) on this Dell's Thunderbolt input; the adapter accepts a
zero or duplicated high byte and uses the low byte as the input selector.
Direct native DDC can replace the adapter behind the same interface.
Linux can use ddcutil; document device permissions rather than invoking interactive
sudo from the UI. Video input selection relies on the monitor's configured USB/KVM
association; independent USB switching is not yet verified. Add other monitors only
when their mappings/behavior are known; do not assume Dell's `0x19` is universal.

## Tools and parallel development

- Use **mise** for tools and tasks. Rust stable + rustfmt/clippy are declared in
  `rust-toolchain.toml`, which mise already reads; avoid duplicate version pins.
- Root `mise.toml` already declares monorepo config roots `pkgs/*`. Put shared
  tasks at root and platform tasks in the relevant package; document actual tasks
  as implemented. Run tools via `mise exec -- …` when needed.
- Tasks: root `fmt`, `check`, `test`; `//pkgs/macos:run`, `:probe` (read-only),
  `:bundle`, `:open`. Bundle output: `target/release/kvmon.app`; requires m1ddc.
- Linux: `pkgs/gtk` uses gtk-rs GIO/GLib for a session D-Bus service with serialized
  ddcutil work and eight-second command timeouts. Switching uses GJS in
  `pkgs/gnome-ext` for a GNOME Shell 50 Quick Settings tile/submenu. Contract:
  `pkgs/gnome-ext/interface.xml`; profiles/labels come from Rust. Quit stops the
  service and disables the extension. D-Bus activates the service on next enable.
- GTK4/libadwaita preferences (`//pkgs/gtk:settings` or submenu Settings) edit
  Linux-local aliases in `$XDG_CONFIG_HOME/kvmon/connections.json`. Empty aliases
  restore shared profile port names on Linux; macOS retains its computer labels.
  DDC/CI does not expose remote hostnames. `GetMonitor` supplies discovered model/bus
  for the preferences card. `SetAlias` persists atomically and publishes labels;
  input mappings stay in core. Settings requires GTK4 and libadwaita development libs.
- Linux refreshes retain the last result while reading; Loading appears only before
  the first result. Quick Settings updates stable rows in profile order, without
  rebuilding/reordering on refresh. Writes still require readback confirmation.
- Linux tasks: `//pkgs/gtk:run`, `:probe` (read-only), `:build`, `:smoke` (isolated
  D-Bus with fake hardware); `//pkgs/gnome-ext:install`, `:enable`, `:pack`.
  Discovery uses Dell manufacturer/model and optional `KVMON_DISPLAY_SERIAL`,
  never a fixed bus. `KVMON_DDCUTIL` overrides the command. Document user I2C access;
  never invoke sudo from the service. Linux fixture tests do not verify hardware.
- Keep platform dependencies target-gated so core builds/tests on both OSes.
  Check the relevant frontend on its native OS; don't claim cross-platform UI
  verification from a core-only build.
- Coordinate changes to core APIs, shared copy, root manifests, and Cargo.lock;
  keep frontend work within its package. Never overwrite another platform's work.
- Format and run relevant checks/tests. Test meaningful shared state transitions
  and backend parsing with fixtures; hardware switching is an explicit manual test
  because it changes the user's active display. Do not switch inputs incidentally.
- Update this file when architecture or verified hardware behavior changes; keep
  it concise and distinguish implemented behavior from plans.
