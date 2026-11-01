# kvmon

A native Rust macOS menu-bar app for the Dell U4025QW's KVM. Click **KVM** in
the menu bar, then **MacBook** (Thunderbolt) or **Fedora PC** (DisplayPort).
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

## Monitor behavior

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
`pkgs/macos` uses `objc2` AppKit bindings. `pkgs/gtk` and `pkgs/gnome-ext` are
reserved for Fedora development. Tests use fixtures/fakes and never switch hardware.
On Fedora, use `mise exec -- cargo test -p kvmon-core` for the shared library.

Manual macOS check: open the menu, verify MacBook is checked, select Fedora PC,
then switch back from Fedora using `ddcutil setvcp 0x60 0x19` on the detected bus.
Check disconnect/reconnect, sleep/wake, missing m1ddc, and Quit. Hardware writes
must be intentional; the read-only probe does not exercise switching.
