#!/bin/sh
#
# populate_multi.sh: This is employed by `$SCRIPT_DIR/build_rootfs_multi.sh`
#                    to prepare a MicroVM's Alpine Linux rootfs image, while
#                    running inside a container.
#
# ckatsak, Wed 09 Feb 2022 10:12:49 PM EET
#
# Environment:
#   $ID    MicroVM ID
#   $IDh   MicroVM hex ID
#   $BENCH The name of the benchmark (string)
#   $ROOT  The absolute path where image's  /  has been bind-mounted inside
#          the container (string)

set -eu

# Configure targets & services to load at boot
apk add --no-cache openrc util-linux
ln -s agetty /etc/init.d/agetty.ttyS0
echo "ttyS0" >/etc/securetty
rc-update add agetty.ttyS0 default
rc-update add devfs boot
rc-update add procfs boot
rc-update add sysfs boot
rc-update add hostname boot
rc-update add networking boot
rc-update add staticroute boot  # FIXME: Is this really needed?
rc-update add local default

# Populate /
for d in bin etc lib root sbin usr bench; do
	tar c "/$d" | tar x -C "$ROOT";
done
for d in dev proc run sys var; do
	mkdir "$ROOT/$d";
done

# Disable root password
sed -i "1s/x//" "$ROOT/etc/passwd"

# Set hostname (replacing all "_" with "-", as "_" is not valid in hostnames)
echo "${BENCH//_/-}-$IDh" >"$ROOT/etc/hostname"
# TODO: ^^ In POSIX sh, string replacement is undefined. [SC2039]
# NOTE: ^^ String replacement seems to be supported in Alpine 3.{6,15} though.

# Any input from MinIO shall be downloaded into a writable tmpfs mount:
echo 'tmpfs /writable_tmpfs tmpfs nosuid,nodev,noatime 0 0' >>"$ROOT/etc/fstab"
mkdir -vm 0755 "$ROOT/writable_tmpfs"

# Configure network interfaces & routes for `networking` target
ALPINE_MINOR_VERSION="$(cut -d'.' -f2 </etc/alpine-release)"
if [ "$ALPINE_MINOR_VERSION" -ge "13" ]; then
	NET_CONF_STR="address 10.0.$ID.2/24"
else
	NET_CONF_STR="address 10.0.$ID.2
	netmask 255.255.255.0"
fi
cat >"$ROOT/etc/network/interfaces" <<EOF
# ckatsak, Wed 19 Jan 2022 09:40:51 PM EET

auto lo
iface lo inet loopback

auto eth0
iface eth0 inet static
	$NET_CONF_STR
	gateway 10.0.$ID.1
	hostname ${BENCH//_/-}-$IDh

EOF

# Also our traditional script for configuring the network
NET_SCRIPT="$ROOT/usr/sbin/netup_guest.sh"
cat >"$NET_SCRIPT" <<EOF
#!/bin/sh
#
# ckatsak, Mon 31 Jan 2022 08:05:00 PM EET

ip addr add 10.0.$ID.2/24 dev eth0
ip link set eth0 up
ip ro add default via 10.0.$ID.1 dev eth0

EOF
chmod +x "$NET_SCRIPT"

# Add a script to automatically spawn the benchmark after booting via openrc.
BENCH_INIT_SCRIPT="$ROOT/etc/local.d/Bench.start"
mkdir -vpm 755 "$(dirname "$BENCH_INIT_SCRIPT")"
cat >"$BENCH_INIT_SCRIPT" <<EOS
#!/bin/sh
#
# ckatsak, Thu 10 Feb 2022 05:50:38 PM EET

# Sync clock to avoid MinIO's S3 Error (code: RequestTimeTooSkewed)
hwclock --hctosys

MINIO_ADDRESS="10.0.$ID.1:59000" /usr/local/bin/python3 /bench/server.py &

EOS
chmod 0775 "$BENCH_INIT_SCRIPT"

