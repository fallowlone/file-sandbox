# Linux Sandbox — Design Spec

**Date:** 2026-05-05
**Replaces:** 2026-05-04-tart-sandbox-environment-design.md (Tart macOS-VM backend, removed in commit 88ea87d)
**Status:** Approved for implementation planning.

## Goal

Replace the deleted Tart macOS-VM sandbox with a Linux-based equivalent that lets the user open untrusted files inside an isolated, ephemeral, read-only Linux VM on the macOS host. The VM provides a GUI desktop so the user can preview PDFs, images, video, and office documents before deciding whether to restore them to the host watch folder.

## Non-Goals

- Headless / batch malware analysis (commodity scanners ClamAV + VirusTotal already cover this on the host).
- Running multiple OSes (Windows/macOS) inside the sandbox.
- Multi-user concurrency. This is a single-user personal tool.
- Anonymity / Tor (different threat model — Whonix/Tails domain).

## Success Criteria

1. User clicks "Open in sandbox" on a quarantined file → within 5 s a window appears showing the file rendered inside a Linux VM running XFCE.
2. The host has no per-session disk artifacts (zero new files on host disk per session).
3. The VM has no network unless the user explicitly enables it for that session, and the network re-disables when the session ends.
4. Discarding a session removes all guest state from RAM. Host crash mid-session leaves no leftover.
5. Files exported from the sandbox via the menubar Export action go through the existing scan pipeline (ClamAV + VirusTotal) before re-entering the watch folder.
6. Base image is reproducible: `mkosi.conf` plus pinned Debian snapshot URL produces the same SHA-256 on rebuild.
7. The codebase compiles, the daemon test suite passes, the menubar app builds via `bash macos-menubar/build.sh`, and the new Sandbox Swift module has unit tests covering path validation and session lifecycle.

## Architecture

### Boundary diagram

```
┌──────────────────────── macOS host (M2 Pro arm64) ─────────────────────────┐
│                                                                            │
│  Node daemon (TS)                            Menubar app (Swift)           │
│  ─────────────────                           ─────────────────             │
│   • watcher → quarantine                      • UI (Jobs / Sandbox /       │
│   • ClamAV / VT scan                            Settings tabs)             │
│   • SQLite job log                            • Sandbox subsystem ←──────┐ │
│   • REST /api/jobs                              (NEW Swift module)       │ │
│       (no /api/sandbox/* anymore)                                        │ │
│                                                                          │ │
│  ┌────────────────────────────────────────────────────────────────────┐  │ │
│  │ Sandbox subsystem (Swift, in-process)                              │◀─┘ │
│  │                                                                    │    │
│  │  SandboxManager ──── creates ──┐                                   │    │
│  │   ▲                            ▼                                   │    │
│  │   │                       VZVirtualMachine ─── attached to ──┐     │    │
│  │   │                            │                             ▼     │    │
│  │   │                            └─ disk RO base.img       virtiofs  │    │
│  │   │                                                       in (RO)  │    │
│  │   │                                                       out (RW) │    │
│  │   │                                                          │     │    │
│  │   │       NSWindow ── hosts ── VZVirtualMachineView ◀────────┘     │    │
│  │   │                                                                │    │
│  │   └── SessionStore (JSON in Application Support)                   │    │
│  │   └── PathValidator                                                │    │
│  │   └── IdleMonitor (timer + NSWorkspace sleep hook)                 │    │
│  └────────────────────────────────────────────────────────────────────┘    │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘

                                  │  one Linux VM per session
                                  ▼

┌────────────────── Sandbox VM (Hardened Debian Slim arm64) ─────────────────┐
│                                                                            │
│  Boot: VZLinuxBootLoader → vmlinuz + initrd from base.img                  │
│  Root: squashfs RO mount of base.img                                       │
│  fstab: tmpfs at /tmp /var/log /var/tmp /run /home/sandbox /srv            │
│  Mounts: /mnt/in (virtiofs RO) /mnt/out (virtiofs RW)                      │
│  User: sandbox uid=1000, no sudo                                           │
│  Network: not attached unless explicitly enabled                           │
│  Display: virtio-gpu 2D, no virgl                                          │
│  Kernel cmdline: lockdown=confidentiality init_on_alloc=1 init_on_free=1   │
│                  randomize_kstack_offset=1 module.sig_enforce=1 oops=panic │
│  Apps: XFCE + evince, eog, mpv, libreoffice, file, strings (+ AppArmor)    │
└────────────────────────────────────────────────────────────────────────────┘
```

### Components

#### Daemon (TS) — what changes

- **Removed** in this work:
  - `src/ui-server.ts` — the 503-stub `/api/sandbox/*` endpoints (currently unused)
  - `src/sandbox-store.{ts,test.ts}` (state moves to Swift)
  - `src/sandbox-paths.{ts,test.ts}` (validation moves to Swift)
  - `sandbox*` config keys in `src/config.ts`
- **Unchanged** responsibilities: watcher, quarantine, ClamAV, VirusTotal, job store, jobs REST.

#### Menubar app — Sandbox Swift module (new)

Location: `macos-menubar/Sources/Sandbox/`.

| File | Responsibility |
|---|---|
| `SandboxManager.swift` | Public façade. `openSession(filePath:)`, `discardSession(id:)`, `exportFromSession(id:fileName:)`, `listSessions()`. Owns `VZVirtualMachine` instances. Threading: main actor. |
| `SessionStore.swift` | Persists session metadata to `~/Library/Application Support/FileSandbox/sandbox-sessions.json`. Schema: id, sourceFilePath, createdAt, lastActiveAt, status, networkEnabled. |
| `SandboxWindowController.swift` | One controller per session window. Owns the `NSWindow` + `VZVirtualMachineView`. Toolbar with Discard, Export, Network-toggle (with confirm). Watches FSEvents on `out/`. |
| `VMConfig.swift` | Builds `VZVirtualMachineConfiguration`: 2 vCPUs, 4 GB RAM (configurable), VZLinuxBootLoader pointing to base.img kernel/initrd, RO disk attachment for base.img, virtiofs in/out shares, virtio-gpu 2D, virtio-input keyboard+pointing. No virtio-net by default. |
| `PathValidator.swift` | Ports the validation rules from the deleted `src/sandbox-paths.ts`. Reject symlinks/hardlinks, resolve realpath, check inside allowed roots (watchPath, quarantinePath). |
| `IdleMonitor.swift` | Per-session inactivity timer. Listens to user input events on `VZVirtualMachineView` to reset the timer. Hard cap 4 h. Fires soft warning at T-5 min. Hooks `NSWorkspace.willSleepNotification` to discard all sessions before host sleeps. |
| `SandboxConfig.swift` | Owns sandbox-related settings. Persists to `~/Library/Application Support/FileSandbox/sandbox-config.json`. Daemon never sees these keys. |

#### Sandbox image

Location: `sandbox-image/`.

```
sandbox-image/
├── mkosi.conf                  # declarative spec (Distribution=debian, Mirror=snapshot…)
├── mkosi.skeleton/             # files copied verbatim into rootfs
│   ├── etc/fstab               # tmpfs lines
│   ├── etc/default/grub.d/     # kernel cmdline
│   ├── etc/apparmor.d/local/   # local profile overrides for evince/mpv/libreoffice
│   ├── etc/sandbox-init        # one-shot launcher: detect file in /mnt/in, exec viewer
│   └── etc/systemd/system/sandbox-launch.service
├── mkosi.postinst              # apt prune, useradd sandbox, disable services
├── mkosi.finalize              # mksquashfs of root, output to base.img
└── README.md
```

Output artifact: `sandbox-image/build/base.img` (squashfs, ~800 MB target after prune). Bundled into the menubar `.app` at build time, or staged at `~/Library/Application Support/FileSandbox/sandbox-base/<sha256>/base.img` if downloaded from a release.

#### Build pipeline

- **Local (developer):** `yarn sandbox:build` runs a Docker/Lima container that executes `mkosi` against `mkosi.conf`. Apt cache stored in `sandbox-image/build/cache/` for fast rebuild.
- **CI (staged for OSS):** `.github/workflows/sandbox-base.yml`, disabled by default. On tag `sandbox-base/v*` runs mkosi on Ubuntu runner, signs `SHA256SUMS` with cosign keyless OIDC, publishes GH Release. Activated when the repo goes public.
- **First-run on user machine:** the menubar app checks for `~/Library/Application Support/FileSandbox/sandbox-base/<digest>/base.img`. If missing, it either uses the bundled image (local-build path) or fetches the release (CI path), verifies SHA-256, caches.

### Data flow — happy path

1. User in **JobsTab** sees a quarantined file. Right-click → "Open in sandbox" (or button on row).
2. Menubar app calls `SandboxManager.openSession(filePath:)`.
3. `PathValidator` checks the path is inside allowed roots, no symlinks. Throws on violation.
4. `SandboxManager` creates a session directory under `~/Library/Application Support/FileSandbox/sandbox-sessions/<uuid>/in/` and `out/`. Hard-links (or copies if cross-volume) the input file into `in/`.
5. `VMConfig` builds a `VZVirtualMachineConfiguration` with: shared RO `base.img`, virtiofs RO mount of `<uuid>/in/`, virtiofs RW mount of `<uuid>/out/`, virtio-gpu, virtio-input, no virtio-net.
6. `SandboxManager` starts the `VZVirtualMachine`, opens a `SandboxWindowController` window, attaches `VZVirtualMachineView`.
7. Inside the guest, `sandbox-launch.service` (oneshot, late in boot) reads `/mnt/in/.fileToOpen` (single line, the file name), determines viewer by extension, exec's it as `sandbox` user. Failure → renders error in a fallback terminal.
8. User interacts with the file. `IdleMonitor` ticks; user input resets the timer.
9. **Export path:** user saves a file inside the guest to `/mnt/out/`. Host FSEvents fires → window banner: "1 file ready to export". User clicks Export → host moves the file to the watch dir → existing watcher pipeline picks it up and runs ClamAV+VT on it.
10. **Discard path:** user clicks Discard, or idle timer fires, or host enters sleep, or hard cap reached. `SandboxManager.discardSession(id:)` calls `vm.stop()`, deletes the session directory (in/ + out/ + state), updates `SessionStore`. Window closes.

### Data flow — concurrency

Multiple sessions run independently in their own NSWindow + own VM. They share the same `base.img` RO file (Apple Virt allows multiple VMs to attach the same RO disk image). Per-session isolation is enforced at the hypervisor boundary; a compromised guest cannot see siblings.

## Security Model

### Threat model

**In scope:**
- Untrusted file (PDF/office/image/video/archive) opened in sandbox attempts to: leak data to network, write to host disk, exploit viewer to gain code execution in guest, escalate to guest kernel, escape via virtio device into host hypervisor, persist past Discard.
- User-error class: forgetting to discard, accidentally exporting malicious file.

**Out of scope:**
- Attacker with physical host access.
- Hardware side-channels (Spectre-class) — relies on Apple's hardware/macOS mitigations.
- Apple Virtualization.framework zero-days — relies on macOS update cadence.
- Attacks on the user's host outside the sandbox flow.

### Defenses (mapped to surfaces)

| Surface | Defense |
|---|---|
| Hypervisor (Apple Virt) | Stay on latest macOS. Monitor Apple security release notes. |
| Guest kernel | Hardened Debian kernel. cmdline `lockdown=confidentiality init_on_alloc=1 init_on_free=1 randomize_kstack_offset=1 module.sig_enforce=1 oops=panic`. KASLR/SMEP/KPTI default. |
| Guest userspace | Viewer apps run as `sandbox` uid=1000, no sudo, no setuid binaries in PATH. AppArmor profiles for evince/eog/mpv/libreoffice. |
| Persistent state | Squashfs RO root + tmpfs for `/tmp /var/log /var/tmp /run /home/sandbox /srv`. No swap. No hibernation. VM state in RAM only — vanishes on poweroff and host sleep. |
| virtio-gpu | 2D only. No virgl/3D acceleration. |
| virtio-fs (RO in) | Host-enforced via `VZSharedDirectory(readOnly: true)`. Guest cannot write. |
| virtio-fs (RW out) | Tightly scoped to `<uuid>/out/`. Host PathValidator on every export. |
| virtio-net | Not attached unless `sandboxNetworkDefault=true`. Per-session toggle reverts after session. |
| virtio-sound | Not attached. |
| virtio-input clipboard | Not attached. |
| Host-side Swift code | All guest-emitted file names treated as untrusted input. Schema validation via Codable + assertion checks. Reject symlinks, hardlinks, non-allowed-root paths. |
| Build-time | mkosi consumes signed Debian Release files, builds from snapshot URL with pinned timestamp, output digest tracked in repo and verified before VM launch. |
| Runtime image integrity | On first launch, menubar verifies `base.img` SHA-256 against value in app's resource bundle. Mismatch = refuse to start sandbox. |
| Idle/forgotten sessions | 30 min default inactivity timeout, 4 h hard cap, sleep-discard. |

### Residual risk

Acknowledged and not mitigated:
- Apple Virt zero-day VM-escape — outside our control; mitigated by macOS updates and choice of Apple's first-party hypervisor over third-party.
- Hardware side-channels — outside our control; macOS/M2 ship mitigations.
- User explicitly exports a malicious file and runs it on the host — by design Export goes through the host scan pipeline, so 99% of commodity malware is caught; the residual user-bypass is accepted.

## Configuration

Sandbox configuration is owned entirely by the Swift menubar. The TS daemon has no awareness of these keys.

**Storage:** `~/Library/Application Support/FileSandbox/sandbox-config.json`. Read/written by `SandboxConfig.swift`.

| Key | Default | Range | Meaning |
|---|---|---|---|
| `sandboxEnabled` | `false` | bool | Master switch. When false, the Sandbox tab is hidden and no VMs may launch. |
| `sandboxIdleTimeoutMinutes` | `30` | 5 – 240 | Per-session inactivity timeout. |
| `sandboxNetworkDefault` | `false` | bool | If true, virtio-net is attached to new sessions by default. Off = no network device attached. |
| `sandboxVmMemoryMB` | `4096` | 1024 – 16384 | RAM allocated per VM. |
| `sandboxVmCpuCount` | `2` | 1 – 8 | vCPU count per VM. |

**TS daemon side — what is removed:**
- `sandboxEnabled`, `sandboxIdleTimeoutMinutes`, `sandboxNetworkDefault`, `sandboxSessionsDir`, `sandboxOutRetentionDays` deleted from `RawConfig` in `src/config.ts`.
- The corresponding fields are stripped from `/api/config` responses in `src/ui-server.ts`.

**Swift menubar side — what changes:**
- `SettingsStore.swift` no longer holds or POSTs `sandbox*` fields. It deals only with daemon-owned settings (paths, ports, VT key, etc.).
- The Sandbox section of `SettingsTabView.swift` binds to `SandboxConfig` instead of `SettingsStore`.

This split keeps the daemon's responsibilities tight (watch/scan/quarantine) and makes the sandbox subsystem self-contained.

## Entitlements + Codesigning

`macos-menubar/build.sh` currently produces an unsigned `FileSandboxMenuBar.app`. Apple Virtualization.framework refuses to start a VM unless the host process has the `com.apple.security.virtualization` entitlement, which requires codesigning.

Changes:

1. New file `macos-menubar/sandbox.entitlements`:
   ```xml
   <key>com.apple.security.virtualization</key>     <true/>
   ```
2. `build.sh` adds an ad-hoc codesign step after the bundle is assembled:
   ```bash
   codesign --force --sign - \
            --entitlements sandbox.entitlements \
            --options runtime \
            "$APP"
   ```
   `--sign -` is ad-hoc (no Developer ID required for local use). `--options runtime` enables Hardened Runtime so the same bundle is ready for notarization later.
3. `Package.swift` declares `Virtualization` framework as a linked framework on the executable target.
4. `Info.plist` already declares `LSMinimumSystemVersion` 13.0 — required for virtiofs and modern `VZGraphicsDeviceConfiguration`. No change needed.
5. Notarization is deferred until an OSS-release flow is in place; documented in `sandbox-image/README.md`.

## Testing Strategy

### Unit tests (Swift, XCTest)

- `PathValidatorTests` — symlink rejection, hardlink rejection, allowed-root enforcement, realpath resolution, edge cases (relative paths, /, ../).
- `SessionStoreTests` — JSON round-trip, concurrent reads, recovery from corrupted file (logs error, returns empty list).
- `VMConfigTests` — given inputs, produces expected `VZVirtualMachineConfiguration` (verify disk attachment is RO, no virtio-net unless flag set, virtio-gpu is 2D only).
- `IdleMonitorTests` — fake clock, verify timer resets on input event, soft warning fires, hard cap fires, sleep notification triggers discard.

### Integration tests (manual smoke)

Documented in `sandbox-image/README.md`:

1. `yarn sandbox:build` produces a base.img.
2. Launch menubar app, drop a benign PDF in watch folder, wait for quarantine.
3. Click "Open in sandbox" → window appears within 5 s with PDF rendered.
4. Verify no network: inside guest, `ip a` shows only `lo`.
5. Save a file from within guest to `/mnt/out/`. Host menubar shows export banner. Click Export. File appears in watch folder, gets scanned.
6. Click Discard. Window closes. Verify `<sessionDir>` is empty/deleted.
7. Open 2 sandboxes simultaneously. Discard one. The other survives.
8. Open a sandbox, do nothing for 30 min. Verify auto-discard.
9. Open a sandbox, put host to sleep. On wake, verify session was discarded and menubar shows the notification.

### What we are NOT testing

- Apple Virtualization.framework correctness (Apple's responsibility).
- Linux kernel correctness (Debian's responsibility).
- That LibreOffice renders every .docx perfectly (out of scope; we just need it to render).

## Migration / Rollout

1. Spec approved (this document).
2. Implementation plan written via `superpowers:writing-plans` skill.
3. Implementation on a feature branch (`feat/linux-sandbox`).
4. base.img buildable via `yarn sandbox:build`.
5. Smoke-test all manual integration steps.
6. Merge to main.
7. (Later, when going OSS) activate `.github/workflows/sandbox-base.yml`, publish first signed release, update menubar to download artifact.

No data to migrate — Tart sessions were already discarded by the surgical removal commit (88ea87d).

## Open Questions

None at this time. Decisions Q1–Q15 locked during the brainstorming session of 2026-05-05. Subsequent questions may surface during plan-writing or implementation; those will be raised at the appropriate phase, not pre-emptively answered here.

## References

- Deleted predecessor: `docs/superpowers/specs/2026-05-04-tart-sandbox-environment-design.md`
- Tart removal commit: `88ea87d` "refactor(sandbox): remove Tart macOS-VM backend in prep for Linux migration"
- Apple Virtualization.framework docs: https://developer.apple.com/documentation/virtualization
- mkosi: https://github.com/systemd/mkosi
- AppArmor on Debian: https://wiki.debian.org/AppArmor
