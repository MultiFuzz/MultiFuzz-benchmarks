#!/bin/bash

export GHIDRA_SRC=./MultiFuzz/ghidra

# Note: `export ICICLE_ENABLE_SHADOW_STACK=1` allows for better callstack output when just looking
# at the fuzzer output. For other targets `gdb` backtrace command is recommended.

config=""

if [[ $1 == *"riot-ccn-lite-relay"* ]]; then
    config="./benchmarks/MultiFuzz/riot-ccn-lite-relay"
elif [[ $1 == *"riot-gnrc_networking"* ]]; then
    config="./benchmarks/MultiFuzz/riot-gnrc_networking"
elif [[ $1 == *"Gateway"* ]]; then
    export ICICLE_ENABLE_SHADOW_STACK=1
    config="./benchmarks/P2IM/Gateway"
elif [[ $1 == *"6LoWPAN_Receiver"* ]]; then
    export ICICLE_ENABLE_SHADOW_STACK=1
    config="./benchmarks/HALucinator/6LoWPAN_Receiver"
elif [[ $1 == *"GPSTracker"* ]]; then
    export ICICLE_ENABLE_SHADOW_STACK=1
    config="./benchmarks/uEmu/GPSTracker"
elif [[ $1 == *"utasker_MODBUS"* ]]; then
    export ICICLE_ENABLE_SHADOW_STACK=1
    config="./benchmarks/uEmu/utasker_MODBUS"
elif [[ $1 == *"utasker_USB"* ]]; then
    export ICICLE_ENABLE_SHADOW_STACK=1
    config="./benchmarks/uEmu/utasker_USB"
elif [[ $1 == *"Zephyr_SocketCan"* ]]; then
    config="./benchmarks/uEmu/Zepyhr_SocketCan"
else
    echo "Unknown replay file"
    exit
fi
ENABLE_DEBUG=1 REPLAY=$1 ./MultiFuzz/target/release/hail-fuzz $config
