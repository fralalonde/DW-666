#! /bin/sh

# This script allows to print RTT output at the console while debugging with CLion + OpenOCD/GDB
#
# First, start OpenOCD from a CLion run config.
# Then run this script at the console:
# > watch_rtt.sh
#
# This script will:
# 1) tell OpenOCD to start it's RTT server
# 2) tail the RTT server's output
#
# Original recipe from https://ferrous-systems.com/blog/gdb-and-defmt/

# check that required apps are on path
type rust-nm >/dev/null 2>&1     || { echo >&2 "Missing rust-nm from cargo-binutils."; exit 1; }
type nc >/dev/null 2>&1          || { echo >&2 "Missing netcat."; exit 1; }
type openocd >/dev/null 2>&1     || { echo >&2 "Missing OpenOCD."; exit 1; }
type defmt-print >/dev/null 2>&1 || { echo >&2 "Missing defmt-print."; exit 1; }

# base env setup
BASEDIR="$( cd "$( dirname "$0" )" && pwd )"
TELNET_PORT=4444
RTT_PORT=8745
BUILD=${1:-debug}
ELF_FILE=$BASEDIR/target/thumbv7em-none-eabihf/$BUILD/DW_666

# OpenOCD should be running
if ! nc -z localhost $TELNET_PORT; then
  echo "OpenOCD not running? Else make sure it is listening for telnet on port $TELNET_PORT"
  # TODO start OpenOCD & flash $ELF_FILE ? assumed done by IDE (for debug) for now
  exit
else
  echo "OpenOCD running"
fi

if ! nc -z localhost $RTT_PORT; then
  # get address of static RTT buffer from binary
  block_addr=0x$(rust-nm -S $ELF_FILE | grep SEGGER_RTT | cut -d' ' -f1)
  echo "Starting RTT from block addr $block_addr (from $ELF_FILE)"

  # Tell GDB to start its RTT server
  # See  https://stackoverflow.com/questions/48578664/capturing-telnet-timeout-from-bash
  nc localhost $TELNET_PORT <<EOF
rtt server start $RTT_PORT 0
rtt setup $block_addr 0x3000 "SEGGER RTT"
rtt start
exit
EOF

  if ! nc -z localhost $RTT_PORT; then
    echo "RTT port still not up :("
    exit
  fi
else
  echo "RTT port is open"
fi

# if using plain RTT https://crates.io/crates/rtt-target
#echo "Watching RTT/text"
#nc localhost $RTT_PORT

# if using defmt over RTT
echo "Watching RTT/defmt from '$ELF_FILE'"
nc localhost $RTT_PORT | defmt-print -e $ELF_FILE


