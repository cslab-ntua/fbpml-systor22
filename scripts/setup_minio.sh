#!/bin/bash
#
# ckatsak, Mon 21 Feb 2022 04:40:34 PM EET

set -eu

SCRIPT_DIR="$(realpath "$(dirname "${BASH_SOURCE[0]}")")"
source "$SCRIPT_DIR/../config"

# Need root to mount tmpfs
[ "$EUID" -ne 0 ] && echo 'ERROR: Only root can mount tmpfs!' && exit 1

# Create the data drive
mkdir -vp "$MINIO_DATA_MP"
mount -t tmpfs tmpfs "$MINIO_DATA_MP"
chown "$(who am i | cut -d' ' -f1)" "$MINIO_DATA_MP"

# Spawn MinIO server in the background
MINIO_ROOT_USER=minioroot MINIO_ROOT_PASSWORD=minioroot \
	"$MINIO_SERVER_BIN" server --address ':59000' "$MINIO_DATA_MP" &

# Wait up to 30s for the MinIO server to start listening
timeout 30 sh -c 'until nc -z $0 $1; do sleep 1; done' 'localhost' '59000'

# Configure MinIO client
"$MINIO_CLIENT_BIN" alias set gold3 'http://localhost:59000' 'minioroot' 'minioroot'

"$MINIO_CLIENT_BIN" mb gold3/fbpml
for file in "$SCRIPT_DIR/../input/"*; do
	"$MINIO_CLIENT_BIN" cp "$file" gold3/fbpml
done

