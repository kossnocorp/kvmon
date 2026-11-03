# Kvmon — GNOME version roadmap

Status: proposed work, not implemented commitments. Updated: 2026-11-03.

This plan covers the **Linux/GNOME version**: the Rust GTK4/libadwaita application,
monitor-control service, and GNOME Shell Quick Settings extension. Shared changes
belong in `pkgs/core`; coordinate those changes with the macOS frontend, whose
product roadmap is outside this document.

## Goals and current baseline

1. Support software-controllable monitor KVMs across manufacturers, with clear
   compatibility reporting and extensible backends.
2. Make first use understandable from either an application-store installation
   or an extensions.gnome.org installation, including missing components.
3. Publish and maintain releases through the appropriate distribution channels.

**Preferred product shape:** a complete standalone GTK application on Flathub,
with no required GNOME Shell extension integration. It provides switching,
configuration, onboarding, and diagnostics itself. A dismissible hint links to the
optional Quick Settings extension, published separately on extensions.gnome.org.
The extension is a convenience client of the same control service/API. The app does
not depend on Shell APIs, install the extension, or require it to be enabled.
GNOME is the design target; ordinary app functionality should also work on other
compatible Linux desktops without a Shell dependency.

Today, `pkgs/gtk` discovers only Dell U4025QW displays, uses a timed `ddcutil`
subprocess adapter, and selects one monitor. The configured inputs and port names
come from a shared Dell profile. Aliases are local and keyed only by input value.
`pkgs/gnome-ext` supplies a GNOME Shell 50 tile; a session D-Bus service performs
hardware operations. GTK provides preferences, but not a standalone switching UI.
Installation builds from source and copies files into the user's data directory.
There is no Flatpak, store listing, general capability discovery, or onboarding.

Preserve the existing behavior: blocking hardware work stays off UI threads;
operations are serialized; background refreshes retain the last result and stable
menu rows; loading appears only before data exists. A successful write does not
confirm an active input until readback succeeds.

## 1. Broad monitor/KVM support

### Define what “universal” can mean

There is no universal monitor KVM API. The first general release should support
**monitors exposing input selection through DDC/CI**, normally MCCS VCP `0x60`.
It must also identify unsupported or partially supported devices without guessing
commands. Other control protocols require separate, verified adapters.

Report support independently for:

| Capability | What Kvmon can promise |
| --- | --- |
| Display discovery | Identify a connected display even when control is unavailable, where the OS exposes its EDID. |
| Video input switching | Read/select advertised or verified input values through a supported backend. |
| USB follows video | Explain that the monitor's OSD association controls this; verify with the user. |
| Independent USB switching | Offer only with a documented, tested device-specific implementation. |
| Proprietary/external KVM | Require a supported protocol adapter; otherwise show the limitation. |

DDC/CI does **not** provide attached computers' hostnames or reliably tell which
advertised inputs have a computer connected. Default labels describe ports, not
computers. User aliases provide friendly names. Remote-host discovery would be a
separate feature requiring another communication mechanism.

### Discovery, identities, and capabilities

- [ ] Enumerate all candidate displays, removing the Dell-only discovery filter.
  Return structured results for accessible, permission-denied, disconnected,
  unsupported, and ambiguous devices instead of one generic error string.
- [ ] Separate persistent monitor identity from its current transport address.
  Use available EDID manufacturer/product/serial information and a fingerprint;
  treat I²C bus numbers, DRM connectors, and enumeration indices as current paths.
  Missing/duplicate serials require disambiguation and sometimes user association.
  Do not silently move configuration between two identical monitors.
- [ ] Read MCCS version, input-source capability values, and the observed input.
  Parse nested capability strings tolerantly: an unrelated malformed feature must
  not invalidate otherwise usable `0x60` information.
- [ ] Generate port labels from MCCS metadata for the applicable version. Apply
  verified model overrides where necessary; otherwise show `Input 0xNN`.
  Keep advertised values, verified overrides, and manual entries distinguishable.
- [ ] Retain unknown observed inputs in state. Offer an advanced manual input
  mapping when capabilities are incomplete, with an explicit user-triggered test.
  Never discover inputs by writing every possible value.
- [ ] Move Dell-specific behavior into a matching profile/quirk layer: `0x19` as
  Thunderbolt, duplicated-byte readback, timing, and any future USB controls must
  not become generic assumptions for other monitors.
- [ ] Cache identity/capabilities and invalidate on relevant changes. Refresh
  observed state on demand and reconnect/wake; avoid full expensive discovery on
  every menu opening when the current device path is still valid.

### Backend and configuration work

- [ ] Extend core types/traits for monitor identities, input descriptors, support
  levels, structured errors, and per-monitor state. Keep platform I/O out of core.
- [ ] Evaluate `libddcutil` for structured discovery/capability access versus the
  existing subprocess adapter. A library integration must retain bounded operations
  and recovery; an uninterruptible FFI call may need a worker process. Select and
  document supported dependency versions before replacing the working adapter.
- [ ] Support multiple configured monitors, each with independent connection
  aliases, visible inputs, preferred order, capabilities, and last-observed state.
  Serialize operations per transport/device and handle contention with other DDC
  applications. Revalidate identity before writing after a reconnect.
- [ ] Introduce a versioned configuration schema keyed by monitor identity and
  input selector. Migrate `connections.json` once, binding its existing aliases to
  the selected Dell only; ask when more than one display could be the owner.
  Use atomic writes and preserve unreadable/newer-format configuration.
- [ ] Show a monitor chooser/cards in the app; group Quick Settings connections
  by monitor when more than one is configured. An unknown device must not inherit
  Dell input mappings. Keep multi-monitor coordinated switching for a later phase:
  writes across displays are not atomic and need partial-success semantics.
- [ ] Publish a compatibility matrix with model, firmware where available,
  connection path, backend, video/USB support, and level of real-hardware testing.
  Add vendor adapters only with protocol evidence and reproducible fixtures.

**Acceptance:** Dell behavior remains covered; test fixtures include other vendors,
malformed capabilities, absent/duplicate serials, unknown input values, and bus
renumbering. Before claiming broad support, demonstrate real switching on at least
two non-Dell models from different vendors and document any limitations. A fixture
or successful read alone is not a hardware-switching verification.

## 2. Distribution architecture: resolve before store onboarding

### Packaging constraints

- The Shell extension runs in GNOME Shell and must remain GJS. GTK/libadwaita
  belongs in a separate process. extensions.gnome.org does not allow bundled binary
  executables/libraries, so its ZIP cannot install the Rust service by itself.
- Flatpak device permissions currently have no dedicated I²C grant. `--device=kvm`
  means **kernel virtualization**, not keyboard/video/mouse switching. The USB
  portal grants raw USB access, not `/dev/i2c-*` access for DDC over video links.
- A direct-I²C Flatpak may need `--device=all`, plus working host device permissions.
  `--filesystem=/dev/i2c-…` is not a solution: `/dev` is reserved. Bundling ddcutil
  does not install host udev rules or load `i2c-dev`, and sandbox permissions cannot
  override host Unix device permissions.
- Flathub's current inclusion rules restrict Shell extensions, tray-only apps,
  host-dependent/system utilities, and environment-locked apps. Some categories
  allow case-by-case exceptions. A Flatpak containing only preferences for a
  separately installed helper/extension is not an assumed route to acceptance.

### First milestone: feasibility prototypes and a packaging decision

| Route | Benefit | Work/risk to resolve |
| --- | --- | --- |
| Flatpak app + bundled control backend | Most self-contained app-store experience | Prove I²C discovery/read/write with proposed permissions and normal user ACLs; discuss hardware-access and inclusion constraints with Flathub maintainers. Host setup may still be required. |
| Flatpak app + separately packaged host service | Narrow named D-Bus access; normal host device handling | Extra installation, independently updated components, distro packaging, and Flathub host-dependency exception. Cannot call this a one-click installation. |
| Native GTK app/service + Shell extension | Direct fit for ddcutil and distro udev integration | Maintain native packages; use this release route if a viable Flatpak/store path is unavailable. |

**Recommended sequence:** prototype the bundled-backend Flatpak first, while keeping
the native app/service installable. Ask Flathub maintainers about the actual working
design early. Use a host-service split if technically necessary, with its installation
cost and store eligibility explicitly resolved. If no acceptable Flathub path exists,
release the native GNOME app and extension rather than advertise a broken Flatpak.
A self-hosted Flatpak repository is a possible preview channel, but does not solve
hardware access or host setup.

The standalone GUI addresses the extension-only/tray-only concern and removes the
Shell-specific dependency from the Flatpak. This materially improves its suitability
for Flathub, but does not by itself establish eligibility: device access, host setup,
the system-utility classification, and the other inclusion requirements still need
review. Treat the optional extension hint as a discoverability feature, not a setup
step required to make the app functional.

- [ ] Test each viable topology on clean Fedora Workstation, Fedora Atomic Desktop,
  and another mainstream GNOME distribution. Test with and without ddcutil/udev
  prerequisites, not just the developer's already configured host.
- [ ] Record required permissions, package ownership, supported OS/Flatpak versions,
  startup/activation behavior, install steps, and maintainer feedback in a decision
  record. This is a release gate, not an assumed implementation detail.
- [ ] Keep the API independent of topology. Scope D-Bus access to the required names;
  avoid unrestricted bus access or a `flatpak-spawn --host` command-execution bridge.
  Prefer an unprivileged control process with normal device ACLs. If a privileged
  broker proves necessary, design/package its narrow API separately.

### Product and IPC changes common to all routes

- [ ] Make the GTK app useful on its own: monitor discovery, switching, aliases,
  setup, and troubleshooting. The GNOME extension adds Quick Settings convenience;
  users can use the app when the extension is absent, disabled, or incompatible.
- [ ] Add a versioned D-Bus contract: protocol/features, component versions,
  discovery/setup status, monitor IDs, per-monitor snapshots, configuration changes,
  and typed failures. Preserve compatibility across independent store updates.
- [ ] Open/activate the app through a stable application interface, including
  Wayland activation handling, instead of assuming the backend can spawn its own
  executable with `--settings`. Keep hardware operations out of the Shell process.
- [ ] Establish one configuration authority per installation. The app and extension
  use IPC rather than reading different host/sandbox copies of a JSON file.
  Provide an explicit native-to-Flatpak import and handle simultaneous installations
  without two services competing for the same bus name or monitor.
- [ ] Specify lifetime behavior: closing a window, disabling the extension, quitting
  the app, and stopping the backend are separate actions. Avoid activation loops;
  use demand-driven activity/idle shutdown and the appropriate background mechanism
  for the chosen packaging. Do not require the extension to run at the lock screen.

## 3. First-run onboarding and recovery

### Entry points

**Application installed first (Flathub or native package):** launch a normal GTK app
from GNOME Software/the app grid. Discover the hardware and complete setup there.
After setup, offer a dismissible “Use Kvmon from Quick Settings” hint with a link
to the published extension page. Keep the link available in Help after dismissal.
Installing/enabling it is a separate user action, not an onboarding requirement.
Show the hint on GNOME; its absence or dismissal changes no app functionality.
The app need not inspect or manage Shell extensions. The extension can diagnose
its own service/protocol compatibility when installed.

**Extension installed first:** display “Set up Kvmon…” if no compatible service is
available. Provide a small GJS preferences/setup page that works without the Rust
app: explain the companion requirement, link to the supported app/package download,
and offer “Check Again” / “Open Kvmon”. This bootstrap must not depend on the missing
service's `Settings` method. Do not silently download executables, edit host rules,
or attempt to enable/install other extensions. Account for Flathub not being enabled
and distinguish installation from merely enabling the extension.

### Setup flow

1. **Check components.** Report application/backend/protocol availability, hardware
   access, and optional Shell integration independently. Show “Ready”, “Needs setup”,
   or a specific problem, rather than a raw subprocess error.
2. **Choose a monitor.** Show detected model/vendor, useful connection details, and
   a serial suffix when needed to distinguish units. Let users configure more than
   one. Empty results must distinguish missing access from no supported hardware.
3. **Set up connections.** Present advertised/verified ports; let the user select
   which to show, add aliases, and reorder them. Explain that ports are not detected
   computer identities. Keep aliases optional and defaults useful.
4. **Explain USB behavior.** If applicable, show the model-specific OSD association
   instructions. Otherwise say that Kvmon selects video and that keyboard/mouse
   routing depends on monitor settings. Mark independent USB control unsupported
   unless the adapter actually implements it.
5. **Optional switching test.** Start with read-only checks. A test requires the user
   to choose an input and explicitly switch. Explain that control/display may move
   away and show how to return using the monitor OSD or another configured host.
   Do not promise automatic rollback after DDC access is lost. Record confirmed,
   unconfirmed, or failed results separately.
6. **Finish.** Show the working controls, optional Quick Settings integration, and
   a persistent route back to Setup/Diagnostics. Allow users to skip the physical
   switch test, resume interrupted setup, and configure newly attached monitors later.

### Actionable diagnostics

| Detected condition | User-facing recovery |
| --- | --- |
| Companion absent or incompatible | Identify which component is needed and open its supported install/update page. |
| Backend executable/library missing | Treat as a packaging fault for bundled builds; offer the distro-specific prerequisite for native builds. |
| I²C devices/module missing | Explain `i2c-dev` and provide supported host setup instructions. |
| Device permission denied | Explain host udev/session access separately from Flatpak permissions; link to the correct package/setup procedure. |
| DDC unavailable or ambiguous failure | Suggest enabling DDC/CI in the monitor OSD and checking cable/dock/adapter support; do not claim an exact cause without evidence. |
| Multiple indistinguishable displays | Ask the user to associate the monitor; never pick the first bus. |
| Readable monitor, missing input capabilities | Offer known-profile/manual configuration or explain unsupported control. |
| Disconnect, sleep, or control lost after switching | Preserve configuration, mark observation stale/unavailable, and retry without replaying a write. |
| Unsupported Shell version or extension disabled | Keep app controls usable and explain integration availability. |

Host changes need the user's normal package-management flow. Do not invoke
interactive sudo from GTK or Shell. Prefer distro-supplied udev rules; account for
immutable systems and show logout/reboot instructions only when that operation
actually requires them. Include “Check Again” after every recoverable setup step.

Use libadwaita status pages, monitor cards, inline actionable errors, and accessible
controls. Keyboard and screen-reader users must be able to rename connections
without double-clicking. Localize app, extension, diagnostics, and store metadata.
Keep normal operation compact; put raw details in an expandable diagnostics view.
Offer a previewable diagnostic export with serials and machine-specific paths
redacted by default. Hardware/profile reporting is user-initiated.

**Acceptance:** on a clean machine, users arriving through either entry point can
reach usable controls or a precise explanation and next action without finding the
developer README. Test missing app, missing backend, denied permissions, unsupported
monitor, offline setup, interrupted setup, and component-version mismatches.

## 4. Publishing and maintaining releases

### Common release foundation

- [ ] Choose a permanent application ID under a controlled domain or repository
  namespace and a permanent extension UUID. Replace local development identifiers
  deliberately; provide migration for aliases and existing extension installs.
- [ ] Add desktop entry, application icon, AppStream/MetaInfo, screenshots, support
  and source URLs, release notes, and explicit license files. Verify redistribution
  terms for Rust dependencies, ddcutil/libddcutil, and extension code/assets.
  Extension distribution must meet GNOME Shell's GPL-compatible licensing rules.
- [ ] Describe compatibility honestly in both listings: supported protocols, tested
  models, USB limitations, hardware prerequisites, and required companion components.
- [ ] Establish tagged releases, changelogs, reproducible builds, dependency/runtime
  maintenance, and a version compatibility table. Start with the actually tested
  Shell versions; do not add future versions to metadata preemptively.

### Flathub application

Conditional on the architecture/eligibility decision in section 2:

- [ ] Ship a functional desktop app, with the Shell extension optional. Include its
  supported backend dependencies where the selected architecture allows it.
- [ ] Verify the complete app-only journey: install, launch, discover, configure,
  switch, restart, and update without any extension installed. Test the standalone
  app on a non-GNOME desktop as well; a promotional extension link must not become
  a hidden functional dependency.
- [ ] Have the maintainer prepare the Flatpak manifest using a supported GNOME
  runtime/SDK and Rust tooling. Supply pinned sources/Cargo dependency sources for
  offline builds, required exported app/D-Bus files, and justified permissions.
- [ ] Validate the build, installed app, AppStream metadata, desktop file, and
  permissions with Flathub tooling. Test `x86_64` and `aarch64` or explicitly limit
  architectures. Verify user/system installation, activation, upgrade, removal,
  configuration persistence, and behavior without the extension.
- [ ] Complete a real-world release/maintenance history before submission. Current
  Flathub rules require a stable initial submission; its beta branch is available
  after acceptance, not as an initial submission shortcut.
- [ ] The maintainer submits and handles review, permission exceptions, and app
  verification. Recheck current requirements at submission time. As currently
  documented, Flathub prohibits AI-generated/assisted manifests and automated AI
  submission/review interactions, and requires disclosure of included generated
  material; plan this as maintainer-owned work.

GNOME Software discovers the app through configured Flathub/distro repositories
and AppStream metadata; it is not a separate publication destination.

### extensions.gnome.org

- [ ] Submit a runtime-only ZIP: readable GJS, metadata, needed interface/resources,
  translations, bootstrap preferences, and license notices. Exclude Rust binaries,
  build/install scripts, and unrelated development files.
- [ ] Add the project/support URL and a clear companion requirement. Link to the
  actual released application/setup page, not a future store listing.
- [ ] Audit enable/disable cleanup, pending async callbacks, signals/timers, missing
  service behavior, and Shell-version APIs. Add JS linting and test installation
  using GNOME's extension tooling. The maintainer must understand and be able to
  explain the submitted code under the review guidelines.
- [ ] Revisit the current Quit action that disables the extension through Shell's
  extension manager: extension-system interactions are reviewed case by case.
  Define a supported pause/quit/re-enable experience before submission.
- [ ] Test advertised GNOME versions in real sessions, including Wayland, extension
  disable/re-enable, service restart, and updates in either component order. Explain
  when a session restart is needed for code updates; never restart Shell automatically.

### Native packages and preview distribution

- [ ] Provide a Fedora package/COPR preview, then pursue distro repositories as
  appropriate. If a host helper is required, package its dependencies, activation
  files, and any justified device setup here. Fedora Atomic installation needs its
  own tested instructions; a mutable-system `dnf` recipe is not sufficient.
- [ ] Provide versioned source archives and checksummed release artifacts for
  testing. Add other distro packages only with maintainers and verified install
  paths. Keep store/package updates authoritative rather than self-updating files.
- [ ] Use these channels for hardware testing and early releases while store review
  is pending. Store approval and hardware acquisition are external dependencies,
  so avoid promising a publication date before those are resolved.

## 5. Delivery order and release gates

| Milestone | Deliverable | Completion evidence |
| --- | --- | --- |
| M0 — Distribution feasibility | Working packaging prototypes and recorded architecture decision | Real device access tested on clean hosts; required setup and store constraints understood. |
| M1 — Generic monitor model | Discovery/capabilities, scoped quirks, per-monitor configuration and migration | Dell regression coverage, non-Dell fixtures and hardware evidence, ambiguous identity handled. |
| M2 — Standalone app and protocol | App switching UI, versioned API, reliable activation/lifecycle | App works without Shell extension; mixed-version and restart tests pass. |
| M3 — Onboarding | Both entry paths, diagnostics, optional physical test, integration setup | Fresh-install walkthroughs reach ready or actionable unsupported/setup states. |
| M4 — Release candidate | Native packages and eligible Flatpak, extension ZIP, metadata/docs | Install/upgrade/uninstall tested; accessibility and supported GNOME/hardware matrix reviewed. |
| M5 — Publication and maintenance | Relevant store submissions and supported releases | Review feedback resolved, public support channels and ongoing release ownership established. |

M0 is the critical dependency for the installation UX. M1's model/backend work can
proceed alongside it; M2 and M3 depend on the resulting API and packaging decisions.
The main unknowns are Flatpak hardware access/store eligibility, incomplete vendor
capabilities, and access to representative hardware. Native backend and onboarding
work should remain useful even if Flathub distribution is deferred.

Keep automated hardware-free tests in CI: discovery/capability fixtures, profile
matching, identity/config migrations, serialized operations, protocol compatibility,
readback failures, and isolated D-Bus integration. Add GTK and nested-Shell checks
for layout, stable refreshes, service loss, and cleanup. Physical switching, USB
association, docks, hotplug, and sleep/wake remain explicit manual hardware tests.
Track packaging/frontend tasks under the relevant package's mise configuration.

## References and decisions still needed

Consulted 2026-11-03; recheck store rules before submission:

- [Flatpak sandbox/device and D-Bus permissions](https://docs.flatpak.org/en/latest/sandbox-permissions.html)
- [Flathub requirements and inclusion policy](https://docs.flathub.org/docs/for-app-authors/requirements)
- [Flathub submission process](https://docs.flathub.org/docs/for-app-authors/submission)
- [GNOME Extensions review guidelines](https://gjs.guide/extensions/review-guidelines/review-guidelines.html)
- [ddcutil capability reporting](https://www.ddcutil.com/cap_u3011_verbose_output/)
- [ddcutil I²C permissions](https://www.ddcutil.com/i2c_permissions/)
- [libddcutil API](https://www.ddcutil.com/api_main/)

Decide in M0/M1: permanent public identity, supported distro/Shell baseline, control
backend/topology, configuration ownership/migration, and the initial hardware test
matrix. Store approval, independent USB switching, and support for arbitrary
proprietary KVMs remain unproven until the corresponding work is completed.
