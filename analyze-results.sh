#!/bin/bash

export POLARS_MAX_THREADS=1
cd analysis && ./target/release/plot-data && ./target/release/plot
