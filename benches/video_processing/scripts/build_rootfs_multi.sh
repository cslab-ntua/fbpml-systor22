#!/bin/bash
#
# build_rootfs_multi.sh: Build many rootfs images for a specific benchmark, in
#                        which the benchmark will be spawned upon booting via
#                        openrc.
#
# ckatsak, Tue 01 Feb 2022 01:52:38 AM EET
#
# Environment:
#   $MANY           The number of distinct rootfs images to create
#   $BENCH          The name of the benchmark (e.g., 'helloworld')
#   $DEBIAN_IMG_TAG The 'name:tag' of the base container image
#   $WHOSE          User to `chown -R` the rootfs directory (default: who am i)

set -eux

SCRIPT_DIR="$(realpath "$(dirname "${BASH_SOURCE[0]}")")"
POPULATE_PATH="$SCRIPT_DIR/populate_multi.sh"

WHOSE="${WHOSE:=$(who am i | awk '{print $1}')}"

[ "$EUID" -ne 0 ] \
    && echo 'ERROR: Mounting the image requires root privileges' && exit 1

MP="$(mktemp -d)"
for id in $(seq 0 1 $(("$MANY" - 1))); do
	idh="$(printf "%02X" "$id")"
	rootfs_path="$SCRIPT_DIR/../../../rootfs/$BENCH/$BENCH-$idh.ext4"
	dd if=/dev/zero of="$rootfs_path" bs=2M count=768  # 1536 MiB "disk" capacity
	/sbin/mkfs.ext4 "$rootfs_path"
	mount "$rootfs_path" "$MP"
	docker run --rm \
	    --volume "$MP":/bench-rootfs \
	    --volume "$POPULATE_PATH":/populate.sh \
	    --hostname "$BENCH-$idh" \
	    -e "BENCH=$BENCH" \
	    -e 'ROOT=/bench-rootfs' \
	    -e "ID=$id" \
	    -e "IDh=$idh" \
	    "$DEBIAN_IMG_TAG" \
	    bash -c '/populate.sh'
	umount "$MP"
done
chown -R "$WHOSE":root "$SCRIPT_DIR/../../../rootfs"
rmdir -v "$MP"

