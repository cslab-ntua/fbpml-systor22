#!/bin/bash
#
# populate_multi.sh: This is employed by `$SCRIPT_DIR/build_rootfs_multi.sh`
#                    to prepare a MicroVM's Debian Linux rootfs image, while
#                    running inside a container.
#
# ckatsak, Wed 09 Feb 2022 10:10:17 PM EET
#
# Environment:
#   $ID    MicroVM ID
#   $IDh   MicroVM hex ID
#   $BENCH The name of the benchmark (string)
#   $ROOT  The absolute path where image's  /  has been bind-mounted inside
#          the container (string)

set -eux

apt-get -y update
apt-get -y upgrade
apt-get -y install systemd
apt-get -y install systemd-sysv udev dbus kmod procps iproute2
apt-get -y install default-dbus-session-bus policykit-1

# Populate /
for d in bin etc lib lib64 root sbin usr var bench; do
	tar c "/$d" | tar x -C "$ROOT";
done
for d in dev proc run sys var; do
	mkdir -vp "$ROOT/$d";
done

# Disable root password
sed -i "1s/x//" "$ROOT/etc/passwd"

# Set hostname
echo "${BENCH//_/-}-$IDh" >"$ROOT/etc/hostname"

###############################################################################
## FIXME(ckatsak): This is a mess, clean it up!
###############################################################################
rm -vf "$ROOT/sbin/init"
ln -s /lib/systemd/systemd "$ROOT/sbin/init"
chmod 777 "$ROOT/sbin/init"

echo "" >"$ROOT/etc/machine-id"
mkdir -vp "$ROOT/var/lib/dbus"
echo "" >"$ROOT/var/lib/dbus/machine-id"
###############################################################################

# Mount a tmpfs on /tmp allow opencv to use it as a local cache
echo 'tmpfs /tmp tmpfs nosuid,nodev,noatime 0 0' >>"$ROOT/etc/fstab"
mkdir -vm 0755 "$ROOT/tmp"  # mountpoints in /etc/fstab must exist a priori

# Any input from MinIO shall be downloaded into a writable tmpfs mount:
echo 'tmpfs /writable_tmpfs tmpfs nosuid,nodev,noatime 0 0' >>"$ROOT/etc/fstab"
mkdir -vm 0755 "$ROOT/writable_tmpfs"

# Configure network interfaces & routes for `networking` target
mkdir -vm 0755 "$ROOT/etc/network"
cat >"$ROOT/etc/network/interfaces" <<EOF
# ckatsak, Tue 01 Feb 2022 02:02:59 AM EET

auto lo
iface lo inet loopback

auto eth0
iface eth0 inet static
       address 10.0.$ID.2/24
       gateway 10.0.$ID.1
       hostname ${BENCH//_/-}-$IDh

EOF

# Also our traditional script for configuring the network
NET_SCRIPT="$ROOT/usr/sbin/netup_guest.sh"
cat >"$NET_SCRIPT" <<EOF
#!/bin/sh
#
# ckatsak, Tue 01 Feb 2022 02:02:59 AM EET

ip addr add 10.0.$ID.2/24 dev eth0
ip link set eth0 up
ip ro add default via 10.0.$ID.1 dev eth0

EOF
chmod +x "$NET_SCRIPT"

# Add a script to automatically spawn the benchmark after booting via openrc.
BENCH_INIT_SCRIPT="$ROOT/etc/rc.local"
cat >"$BENCH_INIT_SCRIPT" <<EOS
#!/bin/bash
#
# ckatsak, Thu 10 Feb 2022 05:50:38 PM EET

/usr/sbin/netup_guest.sh
sleep 2

# Sync clock to avoid MinIO's S3 Error (code: RequestTimeTooSkewed)
hwclock --hctosys

MINIO_ADDRESS="10.0.$ID.1:59000" /usr/local/bin/python3 /bench/server.py &

exit 0

EOS
chmod 0775 "$BENCH_INIT_SCRIPT"

chown -R root:root "$ROOT"

