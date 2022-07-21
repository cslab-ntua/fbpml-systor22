#!/bin/bash
#
# ckatsak, Sun 08 May 2022 05:22:10 PM EEST
#
# Parameters:
#  $1: benchmark name
#
# Assumptions:
# - all executable clients/scripts are in place
# - all tap interfaces exist
# - all rootfs images for the benchmark are in ./rootfs/
# - all benchmark's snapshots are in ./snapshot/
# - minio server is already running, serving all required objects
# - no pre-warming required (though this is easy to add)

set -euxo pipefail

NUM_UVMS=(1 2 4 8 16 32 48 64)

RUNS=10

for n in "${NUM_UVMS[@]}"; do
        ./run_multi.sh \
                -b "$1" \
                --num-uvms "$n" \
                --outdir "/tmp/fbpml_out/$n" \
                --runs "$RUNS" \
                -p '/mnt/pmem0/ckatsak/fbpml_512Mi' \
                -n '/nvme/ckatsak/fbpml_512Mi' \
                -s '/opt/ckatsak/fbpml_512Mi'
done

