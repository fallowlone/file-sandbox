#!/usr/bin/env bash
set -euo pipefail

# Builds sandbox-image/build/{base.img,vmlinuz,initrd.img} via mkosi running in Lima.
# Requires: Lima (limactl) + mkosi/squashfs-tools available inside the VM.

if ! command -v limactl >/dev/null; then
    echo "limactl not found. Install with: brew install lima" >&2
    exit 1
fi

VM_NAME="filesandbox-mkosi"
if ! limactl list --quiet | grep -q "^${VM_NAME}$"; then
    echo "Creating Lima VM '${VM_NAME}' (Debian bookworm)..."
    limactl start --name="${VM_NAME}" template://debian-12 --tty=false
fi

limactl shell "${VM_NAME}" -- bash -lc '
    sudo apt-get update -q
    sudo apt-get install -y -q mkosi systemd-container squashfs-tools debootstrap
'

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "${REPO_ROOT}/sandbox-image/build"

# Lima mounts /Users as read-only. Stage the mkosi config into the VM's
# writable /tmp, run mkosi there, then copy artifacts back.
limactl shell "${VM_NAME}" -- bash -lc "
    set -eux
    sudo rm -rf /tmp/mkosi-work
    mkdir -p /tmp/mkosi-work
    cp -R '${REPO_ROOT}/sandbox-image/.' /tmp/mkosi-work/
    cd /tmp/mkosi-work
    mkdir -p build
    sudo mkosi
    cd /tmp/mkosi-work/build
    sudo mksquashfs rootfs base.img -comp zstd -Xcompression-level 19 -no-progress -noappend
    KERNEL=\$(sudo find rootfs/boot -maxdepth 1 -name 'vmlinuz-*' -print -quit)
    INITRD=\$(sudo find rootfs/boot -maxdepth 1 -name 'initrd.img-*' -print -quit)
    sudo cp \"\$KERNEL\" vmlinuz
    sudo cp \"\$INITRD\" initrd.img
    sudo sha256sum base.img vmlinuz initrd.img | sudo tee SHA256SUMS > /dev/null
    sudo chown \$(id -u):\$(id -g) base.img vmlinuz initrd.img SHA256SUMS
"

# Pull artifacts back via limactl copy (host path is writable).
for f in base.img vmlinuz initrd.img SHA256SUMS; do
    limactl copy "${VM_NAME}:/tmp/mkosi-work/build/${f}" "${REPO_ROOT}/sandbox-image/build/${f}" \
        || echo "warn: failed to copy ${f}"
done

echo "Artifacts:"
ls -la "${REPO_ROOT}/sandbox-image/build/"
[ -f "${REPO_ROOT}/sandbox-image/build/SHA256SUMS" ] && cat "${REPO_ROOT}/sandbox-image/build/SHA256SUMS"
