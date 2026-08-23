#!/bin/bash
# Runs INSIDE the Lima VM (pve-builder, Ubuntu arm64, needs ~40GB free).
# Produces:
#   ~/out/Image.gz-whyred          mainline kernel + appended DTB
#   ~/out/pve_rootfs_arm64.img     ext4 rootfs with Proxmox VE arm64 (official repo)
#
# Stages are idempotent; re-run continues where it left off.
set -euxo pipefail

OUT="$HOME/out"; WORK="$HOME/rootfs-build"
mkdir -p "$OUT" "$WORK"; cd "$WORK"

export DEBIAN_FRONTEND=noninteractive
sudo apt-get update -qq
sudo apt-get install -y --no-install-recommends \
    debootstrap e2fsprogs git build-essential bc bison flex libssl-dev \
    libelf-dev kmod cpio rsync curl ca-certificates arch-test sudo

REPO_URL="${REPO_URL:-https://raw.githubusercontent.com/Egorich-print/whyred-pve-uefi/main}"

# ---------------------------------------------------------------- kernel ----
if [ ! -f "$OUT/Image.gz-whyred" ]; then
    [ -d linux ] || git clone --depth 1 https://github.com/sdm660-mainline/linux.git linux
    cd linux
    curl -fsSL "$REPO_URL/pve/kernel-config.fragment" -o fragment || \
        cp /tmp/kernel-config.fragment fragment   # synced from host as fallback
    make defconfig
    ./scripts/kconfig/merge_config.sh -m .config fragment
    make olddefconfig
    make -j"$(nproc)" Image dtbs
    make -j"$(nproc)" Image.gz || true   # needs CONFIG_KERNEL_GZIP
    KIMG=arch/arm64/boot/Image
    [ -f arch/arm64/boot/Image.gz ] && KIMG=arch/arm64/boot/Image.gz
    # whyred: append DTB to kernel (postmarketOS deviceinfo: append_dtb=true)
    cat "$KIMG" arch/arm64/boot/dts/qcom/sdm636-xiaomi-whyred.dtb > "$OUT/Image-whyred"
    cp "$OUT/Image-whyred" "$OUT/Image.gz-whyred"
    cd ..
fi

# ---------------------------------------------------------------- rootfs ----
R=rootfs
if [ ! -d "$R/bin" ]; then
    sudo rm -rf "$R"
    sudo debootstrap --arch=arm64 --variant=minbase --include=systemd,dbus \
        trixie "$R" http://deb.debian.org/debian
fi

# prevent services starting inside chroot
echo -e '#!/bin/sh\nexit 101' | sudo tee "$R/usr/sbin/policy-rc.d" >/dev/null
sudo chmod +x "$R/usr/sbin/policy-rc.d"

sudo mount --bind /dev     "$R/dev"     2>/dev/null || true
sudo mount --bind /proc    "$R/proc"    2>/dev/null || true
sudo mount --bind /sys     "$R/sys"     2>/dev/null || true

cat > /tmp/chroot-setup.sh <<'EOS'
#!/bin/bash
set -euxo pipefail
export DEBIAN_FRONTEND=noninteractive

hostname whyred-pve
echo whyred-pve > /etc/hostname
grep -q whyred-pve /etc/hosts || cat >> /etc/hosts <<EOF2
127.0.1.1 whyred-pve
10.15.0.1 host
EOF2

# official Proxmox VE arm64 repository (launched 2026-08-05)
curl -fsSL https://download.proxmox.com/debian/proxmox-release-trixie.gpg \
    -o /etc/apt/trusted.gpg.d/proxmox-release-trixie.gpg
echo "deb [arch=arm64] http://download.proxmox.com/debian/pve trixie pve-no-subscription" \
    > /etc/apt/sources.list.d/pve.list

apt-get update
echo "postfix postfix/main_mailer_type select No configuration" | debconf-set-selections
apt-get -y install systemd-sysv locales sudo ifupdown2 lxc lxc-pve \
    proxmox-ve postfix chrony open-iscsi
ln -sf /usr/share/zoneinfo/Europe/Moscow /etc/localtime || true

# serial console on UART (ttyMSM0)
mkdir -p /etc/systemd/system/serial-getty@ttyMSM0.service.d
printf '[Service]\nExecStart=\nExecStart=-/sbin/agetty -L 115200 ttyMSM0 vt100\n' \
    > /etc/systemd/system/serial-getty@ttyMSM0.service.d/override.conf
systemctl enable serial-getty@ttyMSM0.service

# USB RNDIS/Ethernet gadget network for Web UI access over cable
cat > /etc/network/interfaces.d/usb0 <<EOF2
auto usb0
iface usb0 inet static
    address 10.15.0.254/24
    gateway 10.15.0.1
EOF2
cat > /etc/modules-load.d/g_ether.conf <<EOF2
g_ether
EOF2

# root password (operator must change on first login)
chpasswd <<< 'root:whyred'
EOS
sudo cp /tmp/chroot-setup.sh "$R/root/" && sudo chmod +x "$R/root/chroot-setup.sh"
sudo chroot "$R" /root/chroot-setup.sh

# kernel modules into rootfs
cd linux
sudo make INSTALL_MOD_PATH="$(pwd)/../$R" modules_install
cd ..

# boot dir: kernel + extlinux (UEFI path) + cmdline reference
sudo mkdir -p "$R/boot/extlinux"
sudo cp "$OUT/Image.gz-whyred" "$R/boot/"
sudo tee "$R/boot/extlinux/extlinux.conf" >/dev/null <<EOF
default pve
menu title whyred PVE boot
timeout 30
label pve
    menu label Proxmox VE (whyred)
    linux /boot/Image.gz-whyred
    fdt /boot/sdm636-xiaomi-whyred.dtb
    append console=ttyMSM0,115200n8 earlycon=qcom_geni,0xc170000 root=PARTLABEL=userdata rootwait rw
EOF
# dtb alongside (UEFI loaders that take separate fdt)
find linux/arch/arm64/boot/dts -name 'sdm636-xiaomi-whyred.dtb' \
    -exec sudo cp {} "$R/boot/" \;

# fstab: userdata partition is the rootfs itself
sudo tee "$R/etc/fstab" >/dev/null <<EOF
PARTLABEL=userdata  /      ext4  defaults,noatime,errors=remount-ro  0 1
tmpfs               /tmp   tmpfs defaults,size=512m                 0 0
EOF

sudo umount -R "$R/dev" "$R/proc" "$R/sys" 2>/dev/null || true

# ------------------------------------------------------------- image out ----
IMG="$OUT/pve_rootfs_arm64.img"; SIZE_MB=8192
rm -f "$IMG"
dd if=/dev/zero of="$IMG" bs=1M count=0 seek=$SIZE_MB status=none
mkfs.ext4 -q -F -L rootfs -E offset=0 "$IMG"
TMPM=$(mktemp -d)
sudo mount "$IMG" "$TMPM"
sudo rsync -aHAX --exclude=/boot/linux "$R/" "$TMPM/"
sudo umount "$TMPM"; rmdir "$TMPM"
e2fsck -fy "$IMG" || true

echo "=== DONE ==="
ls -la "$OUT"
