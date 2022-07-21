#!/bin/bash
#
# ckatsak, Wed 03 Nov 2021 07:41:50 PM EET
#
# Clean up host's side networking configuration for a specific MicroVM.

set -exo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)"

# Parse guest's ID provided in $1 and host's network interface provided in $2.
# NOTE: This script works only for IDs in [0, 255] for now, because of the way
# that IP & MAC addresses are assigned.
[[ -z "$1" || -z "$2" ]] \
    && echo -e "\nUsage:\n\t$ $0 <decimal-instance-ID> <host-interface>\n" \
    && exit 1
IDd="$1"
IDh="$(printf "%02X" "$1")"
HOST_IF="$2"

# Tap interfaces' names are in the form of `$TAP_PREFIX.$ID`.
TAP_PREFIX="${TAP_PREFIX:=fcpmem01}"

IP="$(command -v ip)"
IPTABLES="$(command -v iptables)"
[[ -z "$IP" || -z "$IPTABLES" ]] \
    && echo 'ERROR: "iproute2" and "iptables" must be installed & in PATH.' \
    && exit 1

# If this is the last tap interface with this prefix on the host, also clean up
# the one-time stuff that are common to all tap interfaces of guest instances.
#if [ "$(ip -o a s | grep -c $TAP_PREFIX)" -eq 1 ]; then
#    #echo 0 >/proc/sys/net/ipv4/ip_forward
#    "$IPTABLES" -t nat -D POSTROUTING -o "$HOST_IF" -j MASQUERADE
#    "$IPTABLES" -D FORWARD -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
#fi

# Clean up configuration specific to this instance's tap interface.
"$IPTABLES" -D FORWARD -i "$TAP_PREFIX.$IDh" -o "$HOST_IF" -j ACCEPT
"$IP" link del "$TAP_PREFIX.$IDh"

