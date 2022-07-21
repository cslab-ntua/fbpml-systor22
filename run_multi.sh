#!/bin/bash
#
# run_multi.sh: Run the ("multi-")benchmarks, assuming everything is set
#
# ckatsak, Thu 10 Feb 2022 12:17:15 PM EET
#
# Assumptions:
#  - All previous build steps have been completed.
#  - All participating benchmarks' snapshots initially live at:
#    `$SCRIPT_DIR/snapshot/$BENCH/{snapshot,memory}-$IDh.file`.
#  - All participating benchmarks' rootfs images initially live at:
#    `$SCRIPT_DIR/rootfs/$BENCH/$BENCH-$IDh.ext4`.
#  - Runs on a machine with multiple NUMA nodes.
#  - DCPM module is mounted on NUMA node 0 (Firecracker gets pinned there).
#
# Results directory outline:
#   $OUTDIR
#      +--chameleon
#      |    +--dcpm
#      |    |    + run00.csv
#      |    |    + run01.csv
#      |    |    + ...
#      |    |    + run$RUNS.csv
#      |    +--nvme
#      |    |    + run00.csv
#      |    |    + run01.csv
#      |    |    + ...
#      |    |    + run$RUNS.csv
#      |    +--ssd
#      |    |    + run0.csv
#      |    |    + run1.csv
#      |    |    + ...
#      |    |    + run$RUNS.csv
#      +--cnn_serving
#      |    +--dcpm
#      |    |    + run00.csv
#      |    |    + run01.csv
#      |    |    + ...
#      |    |    + run$RUNS.csv
#      |    +--nvme
#      |    |    + run00.csv
#      |    |    + run01.csv
#      |    |    + ...
#      |    |    + run$RUNS.csv
#      |    +--ssd
#      |    |    + run00.csv
#      |    |    + run01.csv
#      |    |    + ...
#      |    |    + run$RUNS.csv
#      + ...

set -euo pipefail

SCRIPT_DIR="$(realpath "$(dirname "${BASH_SOURCE[0]}")")"
# shellcheck source=config      # <-- assuming SC is run from repository's root
source "$SCRIPT_DIR/config"
NOW="$(date '+%Y%m%d%H%M%S')"
WHOSE="${WHOSE:=$(who am i | awk '{print $1}')}"
QUIET=false

# Paths to useful binaries
set +e
NUMACTL="$(command -v 'numactl')"
[ -z "$NUMACTL" ] && echo "ERROR: 'numactl' is required" && exit 1
TASKSET="$(command -v 'taskset')"
[ -z "$TASKSET" ] && echo "ERROR: 'taskset' is required" && exit 1
KILLALL="$(command -v 'killall')"
[ -z "$KILLALL" ] && echo "ERROR: 'killall' is required" && exit 1
IP="$(command -v 'ip')"
[ -z "$IP" ] && echo "ERROR: 'iproute2' is required" && exit 1
CP="$(command -v 'rsync')"
[ -z "$CP" ] && echo "ERROR: 'rsync' is required" && exit 1
FFORGET="$(command -v 'fforget')"
# To install it:  $ cargo install --git https://github.com/ckatsak/fforget-bin
[ -z "$FFORGET" ] && echo "ERROR: 'fforget' is required" && exit 1
set -e
CP2M="$SCRIPT_DIR/scripts/cp_2M/cp_2M"
[ ! -f "$CP2M" ] \
	&& echo "ERROR: Building 'cp_2M' is required" && exit 1
FBPML_MULTICLIENT="$SCRIPT_DIR/fbpml-rs/target/release/fbpml-multiclient"
[ ! -f "$FBPML_MULTICLIENT" ] \
	&& echo "ERROR: Building 'fbpml-multiclient' is required" && exit 1

# Makes sure the requested number of TAP interfaces are currently present.
#
# Parameters:
#   $1: Expected number of TAP interfaces
function taps_check() {
	local -r expected="$1"
	local found

	set +e
	found="$("$IP" a s | grep -c 'fcpmem01')"
	set -e
	if [ "$found" -lt "$expected" ]; then
		echo "ERROR: Expected $expected 'fcpmem01.*' tap interfaces; found $found."
		exit 2
	fi
}

# Fills the global $PHYS_CORES array with the physical cores of the provided
# NUMA node.
#
# Parameters:
#   $1: NUMA node number (e.g., 0, 1, ...)
function physical_cores() {
	local -r NODE="$1"
	local cores cs ce
	PHYS_CORES=()

	cores="$(lscpu | grep "node$NODE" | cut -d':' -f2 | tr -d '[:space:]')"
	cs="$(cut -d'-' -f1 <<< "$cores")"
	ce="$(cut -d'-' -f2 <<< "$cores")"
	for c in $(seq "$cs" 1 "$ce"); do
		PHYS_CORES+=("$c")
	done
}

# Fills the global $LOG_CORES array with the logical cores of the provided
# NUMA node.
#
# Parameters:
#   $1: NUMA node number (e.g., 0, 1, ...)
function logical_cpus() {
	local -r NODE="$1"
	LOG_CORES=()

	# Add the first hyperthread of each core of the given node
	local ht0z ht0z_s ht0z_e
	ht0z="$(lscpu | grep "node$NODE" | cut -d':' -f2 | tr -d '[:space:]' \
		| cut -d',' -f1)"
	ht0z_s="$(cut -d'-' -f1 <<< "$ht0z")"
	ht0z_e="$(cut -d'-' -f2 <<< "$ht0z")"
	for c in $(seq "$ht0z_s" 1 "$ht0z_e"); do
		LOG_CORES+=("$c")
	done

	# Add the second hyperthread of each core of the given node
	local ht1z ht1z_s ht1z_e
	ht1z="$(lscpu | grep "node$NODE" | cut -d':' -f2 | tr -d '[:space:]' \
		| cut -d',' -f2)"
	ht1z_s="$(cut -d'-' -f1 <<< "$ht1z")"
	ht1z_e="$(cut -d'-' -f2 <<< "$ht1z")"
	for c in $(seq "$ht1z_s" 1 "$ht1z_e"); do
		LOG_CORES+=("$c")
	done
}

function log_progress() {
	local b d out
	b="$(printf "Bench: %16s" "$2")"
	d="$(printf "Device: %6s" "$3")"
	out="$(printf "$b     $d   :  run %2d" "$1")"
	[ "$QUIET" = false ] && [ -t 2 ] && [ -n "$TERM" ] \
		&& echo -e "$(tput setab 8)[$(date -Ins)] $(tput setaf 2)$out$(tput sgr0)" 1>&2
}

function show_help() {
	cat <<-EOF
	$(show_version)

	USAGE:
	    ${0##*/} [OPTION]...

	OPTION:
	    -h, --help                  Show this help message and exit
	    -V, --version               Show version and exit
	    -v, --verbose               Print commands & args as they are executed
	    -q, --quiet                 Do not log progress to stderr at all
	    -b, --benchmark             The name of the benchmark to run
	        --num-uvms              # of MicroVMs to run in parallel [default=$DEFAULT_MANY]
	    -p, --pmem-path <PATH>      Snapshots' directory on mounted PMEM device
	    -n, --nvme-path <PATH>      Snapshots' directory on mounted NVMe device
	    -s, --ssd-path <PATH>       Snapshots' dir on mounted Flash SSD device
	    -r, --runs <RUNS>           Number of runs of each benchmark [default=$DEFAULT_RUNS]
	        --pre-warm <RUNS>       Number of warm runs before measuring warm
	    -o, --outdir <PATH>         Directory path to dump the resulting CSVs

	NOTE: By running this as root, attempts will be made to drop all page, dentries
	& inode caches caches between runs, whereas 'fforget' will be employed to only
	"uncache" specific files in the case of running this as a common unprivileged
	user.

	EOF
}

function show_version() {
	echo "${0##*/} -- $(basename "$SCRIPT_DIR") v$VERSION"
}

long='help,version,verbose,quiet,benchmark:,num-uvms:,pmem-path:,nvme-path:,ssd-path:,runs:,pre-warm:,outdir:'
short='h,V,v,q,b:,p:,n:,s:,r:,o:'
opts="$(getopt -l "$long" -o "$short" -- "$@")"
[ $? -ne 0 ] && show_help && exit 1
[ $# -eq 0 ] && show_help && exit 1
eval set -- "$opts"
while true; do
	case "$1" in
	-h|--help)          show_help          ; exit 0  ;;
	-V|--version)       show_version       ; exit 0  ;;
	-v|--verbose)       set -x             ; shift   ;;
	-q|--quiet)         QUIET=true         ; shift   ;;
	-b|--benchmark)     BENCH="$2"         ; shift 2 ;;
	--num-uvms)         MANY="$2"          ; shift 2 ;;
	-p|--pmem-path)     PMEM_PATH="$2"     ; shift 2 ;;
	-n|--nvme-path)     NVME_PATH="$2"     ; shift 2 ;;
	-s|--ssd-path)      SSD_PATH="$2"      ; shift 2 ;;
	-r|--runs)          RUNS="$2"          ; shift 2 ;;
	--pre-warm)         PREWARM="$2"       ; shift 2 ;;
	-o|--outdir)        OUTDIR="$2"        ; shift 2 ;;
	--)                 shift              ; break   ;;
	esac
done
VM_ADDR_FMT='10.0.ID.2:50051'
RUNS="${RUNS:-$DEFAULT_RUNS}"
MANY="${MANY:-$DEFAULT_MANY}"
PREWARM="${PREWARM:-0}"  # no pre-warming by default
OUTDIR="${OUTDIR:-${DEFAULT_OUTDIR}_${NOW}}"  # avoid overwriting

FC_NN=0  # Firecracker NUMA node (matters for NVDIMM bus)
CL_NN=1  # fbpml-client NUMA node (must not interfere with Firecracker)
#logical_cpus "$FC_NN"  # Populate the global $LOG_CORES array
physical_cores "$FC_NN"  # Populate the global $PHYS_CORES array

# The root directory of all rootfs images
ROOTFS_PATH="$SCRIPT_DIR/rootfs"

# Benchmarks' input arguments.
declare -A INPUTS=(
	['chameleon']='10 15'
	['cnn_serving']='0'
	['helloworld']=''
	['image_rotate']='2'
	['json_serdes']='0'
	['matmul_fb']='512 512'
	['matmul_fbpml']=''
	['lr_serving']='0'
	['lr_training']='0'
	['pyaes']=''
	['rnn_serving']='0'
	['video_processing']='0'
)

# Parse $BENCH's input arguments into the $ARGS array
IFS=' ' read -r -a ARGS <<< "${INPUTS["$BENCH"]}"


# Commonly-indexed arrays of devices and their final snapshot paths, based on
# the provided command-line arguments
set +u
DEVICES=()
DEVICE_PATHS=()
if [ -z "$PMEM_PATH" ]; then
	echo 'WARNING: Skipping runs on persistent memory; no such path was provided.'
else
	DEVICES+=('dcpm')
	DEVICE_PATHS+=("$PMEM_PATH")
fi
if [ -z "$NVME_PATH" ]; then
	echo 'WARNING: Skipping runs on NVMe; no such path was provided.'
else
	DEVICES+=('nvme')
	DEVICE_PATHS+=("$NVME_PATH")
fi
if [ -z "$SSD_PATH" ]; then
	echo 'WARNING: Skipping runs on Flash SSD; no such path was provided.'
else
	DEVICES+=('ssd')
	DEVICE_PATHS+=("$SSD_PATH")
fi
set -u


# Make sure all TAP interfaces are there
taps_check "$MANY"


# API socket path FMT string
SOCK_FMT="/tmp/firecracker-$BENCH-IDh.socket"

for d in "${!DEVICES[@]}"; do
	device="${DEVICES[$d]}"

	outdir="$OUTDIR/$BENCH/$device"
	mkdir -vp "$outdir"

	# Remove any existing old snapshots that may still be lying around from
	# earlier?
	#rm -rvf "${DEVICE_PATHS[$d]:?}/$BENCH"
	# Create the directory
	mkdir -vp "${DEVICE_PATHS[$d]}/$BENCH"
	state_file_fmt="${DEVICE_PATHS[$d]}/$BENCH/snapshot-IDh.file"
	memory_file_fmt="${DEVICE_PATHS[$d]}/$BENCH/memory-IDh.file"

	# Copy benchmark's snapshot files in the given path.
	for id in $(seq 0 1 $((MANY - 1))); do
		idh="$(printf "%02X" "$id")"
		sf="${state_file_fmt//IDh/$idh}"
		mf="${memory_file_fmt//IDh/$idh}"

		if [ "$device" = 'dcpm' ]; then
			if [ -f "$sf" ]; then
				echo "Snapshot $sf is already here. Skipping copying it..."
			else
				"$CP2M" "$SCRIPT_DIR/snapshot/$BENCH/snapshot-$idh.file" "$sf"
				"$CP2M" "$SCRIPT_DIR/snapshot/$BENCH/memory-$idh.file" "$mf"
			fi
		else
			"$CP" -av "$SCRIPT_DIR/snapshot/$BENCH/snapshot-$idh.file" "$sf"
			"$CP" -av "$SCRIPT_DIR/snapshot/$BENCH/memory-$idh.file" "$mf"
		fi
	done

	# Begin the runs for this device
	for run in $(seq 1 1 "$RUNS"); do
		outfile="$outdir/run$(printf "%02d" "$run").csv"

		# Remove snapshot and rootfs files from the cache
		echo 'Flushing all data to disk...'
		sync
		if [ "$EUID" -ne 0 ]; then
			echo 'Attempting to remove related files from the page cache...'
			shopt -s nullglob
			"$FFORGET" \
				"$(dirname "$state_file_fmt")"/*.file \
				"$ROOTFS_PATH/$BENCH"/*.ext4
			shopt -u nullglob
		else
			echo 'Dropping all page, dentry & inode caches...'
			echo 3 >/proc/sys/vm/drop_caches
		fi

		# Spawn all Firecracker instances
		sleep .2
		for id in $(seq 0 1 $((MANY - 1))); do
			idh="$(printf "%02X" "$id")"

			# Unlink any old API socket
			sock="${SOCK_FMT//IDh/$idh}"
			rm -vf "$sock"

			# Spawn a firecracker microVM, pinning all its threads (API server
			# and VCPU(s) (any others?)) to a physical core.
			# NOTE: There is no way to specify memory allocation policy using
			# `taskset` (vs `numactl`), but the default policy is to allocate
			# memory on the node of the core that triggers the allocation (see
			# set_mempolicy(2), flag MPOL_DEFAULT).
			"$TASKSET" \
				--all-tasks \
				--cpu-list "${PHYS_CORES[$((id % ${#PHYS_CORES[@]}))]}" \
				"$FC_BIN" \
					--id "${BENCH//_/-}-$idh" \
					--api-sock "$sock" \
					>/dev/null \
					&
		done

		log_progress "$run" "$BENCH" "$device"
		sleep .2

		# Run multiclient
		"$NUMACTL" \
			--localalloc \
			--cpunodebind="$CL_NN" \
			"$FBPML_MULTICLIENT" \
				--server-addr-fmt "$VM_ADDR_FMT" \
				--num-uvms "$MANY" \
				--pre-warm "$PREWARM" \
				restore \
					--api-sock "$SOCK_FMT" \
					--state-file "$state_file_fmt" \
					--memory-file "$memory_file_fmt" \
					"${BENCH//_/-}" \
						"${ARGS[@]}" \
				>>"$outfile"

		# Wait for all MicroVMs to terminate so that all API sockets and tap
		# interfaces are released for the next run
		"$KILLALL" --wait -TERM 'firecracker'
	done
done

chown -R "$WHOSE":root "$OUTDIR"

