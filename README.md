# kvmon

A native Rust KVM switcher for the Dell U4025QW on macOS and GNOME. Click **KVM** in
the macOS menu bar or GNOME Quick Settings, then **MacBook** (Thunderbolt) or **Fedora PC** (DisplayPort).
The observed active input is checked and listed first. **Quit** exits the app.

## Run on macOS

Requires Apple Silicon, macOS 12+, Xcode Command Line Tools, [mise](https://mise.jdx.dev),
and `m1ddc` accessible through Homebrew or PATH:

```sh
brew install m1ddc
mise install
mise run //pkgs/macos:probe  # read-only hardware check
mise run //pkgs/macos:run
```

To build and open a standalone app bundle:

```sh
mise run //pkgs/macos:open
```

The locally ad-hoc-signed bundle is `target/release/kvmon.app`; copy it to
`/Applications` if desired. It still requires an installed `m1ddc`. The bundle
runs without a Dock icon. Quit an existing instance before starting another.

## Run on Fedora / GNOME

The GNOME Shell 50 extension adds a **KVM** tile beside Wi-Fi, Bluetooth, and
sound in Quick Settings. Its dropdown lists the configured computers, checks the
observed input, and includes **Quit**. Quit stops the service and disables the
extension; enable it again to restart.

Choose **Settings…** in the KVM submenu to open the native GTK4/libadwaita
preferences window. Double-click a connection or use its Edit button, then press
Enter or Save to commit; Escape or Cancel discards the edit. Empty or whitespace-only
names restore the port-name placeholder (Thunderbolt or DisplayPort-1, from the
verified Dell profile). DDC/CI does not report connected computers' hostnames.
The monitor card shows the discovered model and current I2C bus, or an explicit
unavailable state. Aliases are stored by input in
`${XDG_CONFIG_HOME:-~/.config}/kvmon/connections.json` and update Quick Settings
immediately. These are Linux-local aliases; monitor input mappings remain unchanged.
The settings window can also be opened with `mise run //pkgs/gtk:settings`.

After updating an existing installation, Quit KVM before reinstalling, then log out
and back in to reload the extension code and enable it again.

The UI uses GNOME Shell's native Quick Settings widgets (GJS). Monitor control
is a background Rust service using gtk-rs' `gio` and `glib` crates over the session
bus. GTK widgets cannot be embedded in Shell's Quick Settings, so no separate
GTK window is needed for switching; a GTK4/libadwaita window handles preferences.
The service is activated automatically by D-Bus.

```sh
sudo dnf install gcc glib2-devel gtk4-devel libadwaita-devel pkgconf-pkg-config ddcutil python3
mise install
mise run //pkgs/gtk:probe       # read-only; run without sudo to check permissions
mise run //pkgs/gnome-ext:install
# Log out and back in after the first installation so Shell discovers the extension.
mise run //pkgs/gnome-ext:enable
```

Enable DDC/CI in the monitor OSD. The service needs user access to `/dev/i2c-*`.
Fedora's ddcutil package supplies udev access rules; load `i2c-dev` with
`sudo modprobe i2c-dev` if the devices are absent. If your distribution uses an
`i2c` group instead of session `uaccess`, add your user to that existing group
(`sudo usermod -aG i2c "$USER"`) and log in again. Verify the read-only probe works
as your normal user; the UI never runs sudo. Do not make I2C devices world-writable.

Install locations (under `${XDG_DATA_HOME:-~/.local/share}`):
`gnome-shell/extensions/kvmon@kvmon.local`, `kvmon/kvmon-gtk`, and
`dbus-1/services/io.github.kvmon.Monitor.service`. To remove, disable the extension
with `gnome-extensions disable kvmon@kvmon.local`, stop the service with
`gdbus call --session --dest io.github.kvmon.Monitor --object-path /io/github/kvmon/Monitor --method io.github.kvmon.Monitor.Quit`,
then remove those three paths.

Linux discovers the Dell by manufacturer/model and current I2C bus each operation;
no bus number is hardcoded. Set `KVMON_DISPLAY_SERIAL` when more than one matching
Dell is connected, or `KVMON_DDCUTIL` for an alternate executable. For debugging,
stop the existing service, then run these environment variables with
`mise run //pkgs/gtk:run`. D-Bus activation uses the session bus's environment,
not variables from an arbitrary terminal.

The service refreshes at startup, on submenu open, and every ten seconds for
reconnect/wake recovery. DDC commands are serialized on a worker, each with an
eight-second timeout. A successful write is read back before checking an input;
loss of monitor access shows an error rather than an unverified checkmark.
Unknown inputs remain selectable to recover to a configured computer.

## macOS monitor behavior

- Uses `m1ddc` for DDC/CI VCP `0x60`, selected by discovered UUID rather than index.
- Reads at startup, on menu open, and every 10 seconds (also recovers after wake).
- Commands run serially on a worker with a five-second timeout per command.
- A successful write is read back before marking the requested input active.
  The monitor may stop answering the Mac after switching to the PC; in that case
  the menu shows an unavailable/error state until the Mac can read it again.
- Set the monitor's USB upstream associations in its OSD so USB follows video.
  This app switches video input; it does not separately program USB routing.
- HDMI input is recognized as an unconfigured input; the menu still allows
  switching to either configured computer.

Optional environment variables (set before launching the executable):

| Variable | Purpose |
| --- | --- |
| `KVMON_M1DDC` | Absolute path to an alternate `m1ddc` executable |
| `KVMON_DISPLAY_UUID` | Choose one Dell U4025QW when multiple are connected |

Computer labels and input mappings currently live in `pkgs/core/src/lib.rs`.
Other monitor models and editable settings are not implemented yet.

## Development

```sh
mise run fmt
mise run check
mise run test
```

`pkgs/core` contains platform-independent traits, profile/copy, and state behavior;
`pkgs/macos` uses `objc2` AppKit bindings. `pkgs/gtk` implements the Linux ddcutil
backend and gtk-rs GIO service; `pkgs/gnome-ext` implements the Quick Settings UI.
Their small typed D-Bus contract is `pkgs/gnome-ext/interface.xml`: `GetState`,
`Refresh`, `Select`, `Quit`, `Settings`, `SetAlias`, and `Changed` snapshots including resolved labels.
Tests use fixtures/fakes and never switch hardware. Run
`mise run //pkgs/gtk:smoke` for an isolated D-Bus/fake-monitor integration check.
`mise run //pkgs/gnome-ext:pack` creates an extension zip in `target/`; recipients
also need the Rust service installed. The discovery fixture is representative
ddcutil output, not a new hardware verification.

Manual GNOME check: open Quick Settings → KVM, check the observed input, intentionally
switch to MacBook and back, then test disconnect/reconnect, sleep/wake, missing
ddcutil/device permissions, Quit, and re-enable. Shell UI and real switching require
testing in a logged-in GNOME session; Rust tests alone do not verify that UI.

Manual macOS check: open the menu, verify MacBook is checked, select Fedora PC,
then switch back from Fedora using `ddcutil setvcp 0x60 0x19` on the detected bus.
Check disconnect/reconnect, sleep/wake, missing m1ddc, and Quit. Hardware writes
must be intentional; the read-only probe does not exercise switching.
