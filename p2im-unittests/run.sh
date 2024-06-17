#!/bin/bash

if [[ $# -neq 0 ]]; then
    export WORKERS=$1
fi

PRINT_CRASHES=0 ICICLE_LOG='hail_fuzz::p2im_unit_tests=info' GHIDRA_SRC=../MultiFuzz/ghidra P2IM_UNIT_TESTS=../p2im-unittests/ ../MultiFuzz/target/release/hail-fuzz
