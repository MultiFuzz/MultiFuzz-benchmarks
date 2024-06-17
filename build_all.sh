#!/bin/bash

git submodule update --init --recursive

cargo build --release --manifest-path MultiFuzz/Cargo.toml
cargo build --release --manifest-path bench-harness/Cargo.toml
cargo build --release --manifest-path analysis/Cargo.toml


