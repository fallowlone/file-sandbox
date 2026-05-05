# Sandbox Image

Reproducible Debian-slim base image for the Linux sandbox VM. Built via `mkosi` inside a Lima Debian VM (since `mkosi` is Linux-only).

## Prerequisites

- macOS host (Apple Silicon recommended)
- [Lima](https://github.com/lima-vm/lima): `brew install lima`

## Build

```bash
yarn sandbox:build
```

This wraps `scripts/sandbox-build.sh`:

1. Creates a Lima VM `filesandbox-mkosi` (Debian bookworm) on first run.
2. Installs `mkosi`, `systemd-container`, `squashfs-tools`, `debootstrap` inside the VM.
3. Runs `mkosi --force` against `sandbox-image/mkosi.conf`.
4. Outputs squashfs image + extracted kernel/initrd to `sandbox-image/build/`.
5. Writes `SHA256SUMS` for verification.

## Output Artifacts

- `sandbox-image/build/base.img` — squashfs root filesystem (zstd -19 compressed).
- `sandbox-image/build/vmlinuz` — Linux kernel for `VZLinuxBootLoader`.
- `sandbox-image/build/initrd.img` — initial ramdisk.
- `sandbox-image/build/SHA256SUMS` — checksums for the three above.

## How the Menubar App Picks Up the Image

`macos-menubar/build.sh` stages artifacts into `~/Library/Application Support/FileSandbox/sandbox-base/current/` after the Swift build, then verifies SHA-256 against `SHA256SUMS`. Mismatch → menubar refuses to launch sandbox sessions (fail closed).

## Reproducibility

The image is reproducible if all of these are pinned:

- `Mirror=https://snapshot.debian.org/archive/debian/<TIMESTAMP>/` in `mkosi.conf` — change the timestamp to update.
- `Architecture=arm64` — only arm64 is supported (Apple Silicon).
- Package list in `[Content].Packages` — adding or removing packages changes the digest.

To produce a reproducible release, commit the timestamp pin and the resulting `SHA256SUMS`.

## Hardening Applied

Inside the rootfs:

- `/etc/fstab` — `tmpfs` for every writable path (`/tmp`, `/var/log`, `/var/tmp`, `/run`, `/home/sandbox`, `/srv`); `/mnt/in` virtiofs RO + `noexec`; `/mnt/out` virtiofs RW + `noexec`.
- `/etc/default/grub.d/99-sandbox.cfg` — kernel cmdline: `lockdown=confidentiality init_on_alloc=1 init_on_free=1 randomize_kstack_offset=1 module.sig_enforce=1 oops=panic`.
- `/etc/apparmor.d/local/usr.bin.{evince,eog,mpv,libreoffice}` — local profile additions denying network and `/proc/*/mem` reads for each viewer.
- `mkosi.postinst`:
  - Creates `sandbox` user (uid 1000, no sudo, locked password).
  - Locks `root` password.
  - Enables `sandbox-launch.service`.
  - Purges `cups-*`, `exim4-*`, `unattended-upgrades`.
  - Disables `systemd-resolved`, `systemd-networkd`, `systemd-timesyncd`, `cron`, `rsyslog`.
  - Removes apt caches, debconf caches, man pages.

## Smoke Checklist (post-build, manual)

1. `yarn sandbox:build` produces `sandbox-image/build/base.img`.
2. `bash macos-menubar/build.sh` stages artifacts under `~/Library/Application Support/FileSandbox/sandbox-base/current/`.
3. Open the menubar app → enable sandbox in Settings.
4. Drop a benign PDF in the watch folder.
5. Click "Open in sandbox" on the resulting Jobs row → VM window appears within 5 s, PDF rendered.
6. Inside the guest, `ip a` shows only `lo`.
7. Save a file in the guest to `/mnt/out/` → host banner shows "1 file ready to export"; clicking Export moves it through the host scan pipeline.
8. Click Discard → window closes, session dir removed.
9. Open 2 sandboxes simultaneously → one Discard does not affect the other.
10. Sleep the host while a sandbox is open → on wake, session is discarded.
