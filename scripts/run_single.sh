#!/bin/bash
#
# ckatsak, Sun Feb 27 22:22:34 EET 2022

set -euo pipefail

#
# Prerequisites
#
CURL="$(command -v curl)"


# Return only when the specified ADDRESS:PORT is opened within the next 30sec.
#
# Arguments:
# 	$1: ADDRESS:PORT to poll
function wait_port() {
	addr="$(echo "$1" | awk -F':' '{print $1}')"
	port="$(echo "$1" | awk -F':' '{print $2}')"
	# shellcheck disable=SC2016
	timeout 20 sh -c 'until nc -z $0 $1; do sleep 1; done' "$addr" "$port"
	return $?
}


#
# Hardcoded global configuration variables
#
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)"
LOGS_DIR="$SCRIPT_DIR/../logs"
METRICS_DIR="$SCRIPT_DIR/../metrics"

# shellcheck source=config   # <-- assume SC will be run from repository's root
source "$SCRIPT_DIR/../config"


#
# Argument parsing stuff
#
function show_help() {
	cat <<-EOF

	USAGE:
	    ${0##*/} [OPTION]...

	OPTION:
	    -h, --help                  Show this help message and exit
	    -v, --verbose               Trace execution (bash -x)
	    -b, --bench <NAME>          The name of the benchmark
	    -c, --connect <ADDR:PORT>   Address & port of the server inside the uVM

	EOF
}
long='help,verbose,bench:,connect:'
short='h,v,b:,c:'
opts="$(getopt -l "$long" -o "$short" -- "$@")"
[ $? -ne 0 ] && show_help && exit 1
[ $# -eq 0 ] && show_help && exit 1
eval set -- "$opts"
while true; do
	case "$1" in
	-h|--help)          show_help         ; exit 0  ;;
	-v|--verbose)       set -x            ; shift   ;;
	-b|--bench)         BENCH="$2"        ; shift 2 ;;
	-c|--connect)       VM_ADDR="$2"      ; shift 2 ;;
	--)                 shift             ; break   ;;
	esac
done

#
# Setting more variables, now that we also got the command line arguments
#
VM_ADDR="${VM_ADDR:="$DEFAULT_VM_ADDR"}"

#rm -rvf "$LOGS_DIR" "$METRICS_DIR"
mkdir -vp "$LOGS_DIR" "$METRICS_DIR"
LOG_PATH="$LOGS_DIR/fc-$BENCH.log"
METRICS_PATH="$METRICS_DIR/fc-$BENCH.metrics"
truncate -s0 "$LOG_PATH" "$METRICS_PATH"


MICROVM_CONFIG="{
	\"boot-source\": {
		\"kernel_image_path\": \"$KERNEL_IMG_PATH\",
		\"boot_args\": \"8250.nr_uarts=0 reboot=k panic=1 pci=off ro noapic nomodules random.trust_cpu=on transparent_hugepage=always\"
	},
	\"drives\": [
		{
			\"drive_id\": \"rootfs\",
			\"path_on_host\": \"$SCRIPT_DIR/../rootfs/$BENCH/$BENCH-00.ext4\",
			\"is_root_device\": true,
			\"is_read_only\": true
		}
	],
	\"machine-config\": {
		\"mem_size_mib\": 512,
		\"vcpu_count\": 1,
		\"smt\": false
	},
	\"logger\": {
		\"log_path\": \"$LOG_PATH\",
		\"level\": \"Error\",
		\"show_level\": true,
		\"show_log_origin\": true
	},
	\"metrics\": {
		\"metrics_path\": \"$METRICS_PATH\"
	},
	\"network-interfaces\": [
		{
			\"iface_id\": \"eth0\",
			\"guest_mac\": \"AA:FC:00:00:05:00\",
			\"host_dev_name\": \"fcpmem01.00\"
		}
	]
}"


sock="/tmp/firecracker-$BENCH.socket"
rm -vf "$sock"

# Spawn the MicroVM (replace all "_" in $BENCH with "-" to be a valid FC id)
conf="$(mktemp --tmpdir='/tmp' "fc-$BENCH.XXXXXXXXXX.json")"
echo "$MICROVM_CONFIG" >"$conf"
"$FC_BIN" \
	--id "${BENCH//_/-}" \
	--config-file "$conf" \
	--api-sock "$sock" \
	>/dev/null \
	&
echo "MicroVM booting; PID: $!"

# Poll the TCP server inside it until it appears to be up
echo -n 'Waiting until the gRPC server is initialized...'
wait_port "$VM_ADDR"
echo ' Done!'

