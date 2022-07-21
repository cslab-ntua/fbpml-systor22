#!/bin/bash
#
# ckatsak, Wed 03 Nov 2021 02:30:38 PM EET
#
# Configure host's side networking for a MicroVM.

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

# If no other tap interface with the same prefix is present on host, set up
# some basic one-time stuff common to all tap interfaces of guest instances.
if [ -z "$(ip -o a s | grep $TAP_PREFIX)" ]; then
    echo 1 >/proc/sys/net/ipv4/ip_forward
    "$IPTABLES" -t nat -A POSTROUTING -o "$HOST_IF" -j MASQUERADE
    "$IPTABLES" -A FORWARD -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
fi

# Configuration specific to this instance's tap interface.
"$IP" tuntap add "$TAP_PREFIX.$IDh" mode tap
"$IP" addr add "10.0.$IDd.1/24" dev "$TAP_PREFIX.$IDh"
"$IPTABLES" -A FORWARD -i "$TAP_PREFIX.$IDh" -o "$HOST_IF" -j ACCEPT
"$IP" link set "$TAP_PREFIX.$IDh" up

