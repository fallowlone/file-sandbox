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
limactl shell "${VM_NAME}" -- bash -lc "cd '${REPO_ROOT}/sandbox-image' && sudo mkosi --force"

echo "Artifacts:"
ls -la "${REPO_ROOT}/sandbox-image/build/"
cat "${REPO_ROOT}/sandbox-image/build/SHA256SUMS"
