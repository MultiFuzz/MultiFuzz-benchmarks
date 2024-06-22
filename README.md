# MultiFuzz-benchmarks

Benchmarking tools and datasets to support MultiFuzz artifact evaluation.

## Overview


* (E1): [P2IM Unit Tests](./p2im-unittests): This experiment runs MultiFuzz on the 46 unit test binaries from P2IM for a short period of time (10 mins per binary) and verifies that the fuzzer is able to find inputs that satisfy each unit test.

* (E2): [Code-coverage Evaluation](#code-coverage-evaluation): This experiment uses MultiFuzz to fuzz 23 real-world ARM firmware for 24 hours (repeated for 5 times) and evaluates the code-coverage reached over-time.

* (E3): [Ablation Study](#ablation-study): This experiment tests the fuzzing ability of MultiFuzz with various features disabled on the same set of binaries as E2.

* (E4): [Bug Analysis](./crash-analysis.md): This experiment validates that MultiFuzz finds new previously undiscovered bugs, and that the bugs discovered are real bugs and not false-positives.


## Prerequisites

First, ensure that the current user has access to Docker and following dependencies are installed:

* Rust
* Clang
* Docker
* libssl-dev
* pkg-config

On a Ubuntu 22.04 server this can be done by running the following commands:

```bash
curl https://sh.rustup.rs -sSf | bash -s -- -y
. "$HOME/.cargo/env"
sudo apt update
sudo apt install -y clang docker libssl-dev pkg-config libfontconfig-dev
sudo usermod -aG docker $USER
newgrp docker
```

After these dependencies have been installed, run `build_all.sh` script to compile and build MultiFuzz, the benchmark harness tool, and the postprocess and analysis tool.

## Testing

The `./replay.sh` script allows crashing inputs to be replayed for analysis. It also serves to test whether the fuzzer has been compiled and built correctly. Running the command:

```
./replay.sh crashes/Gateway/zero_length_sysex
```

Should print `[icicle] exited with: UnhandledException(code=ReadUnmapped, value=0x800080)` along with additional information for debugging. (see [crash-analysis.md](./crash-analysis.md) for further details).

To verify that the benchmarking tool and every fuzzer configuration is functional, a fast benchmark profile that takes approximately 20 CPU minutes is available by running the `./benchmark-test.sh` script.

```
./benchmark-test.sh [Number of parallel tasks to use]
```

After execution, the `./bench-harness/output/multifuzz/` directory should contain several sub-directories: `debug-all`, `debug-ext`, `debug-havoc`, `debug-i2s`, `debug-trim`.

## Code-coverage Evaluation

Our code-coverage evaluation is fully automated by running:

```
multifuzz-coverage.sh [<parallel-tasks-to-run>]
```

Note: The script should be run using `screen`/`TMUX` or similar virtual terminal tools to allow the script to run uninterrupted. Sending a `SIGTERM` (or hitting Ctrl+C) to the `bench-harness` process performs a clean shutdown of the harness tool. Any completed trials will be saved, but progress in any active benchmark trials will be lost.

WARNING: This requires a significant amount of CPU hours to execute (~130 CPU days)

The script reads uses the [multifuzz-all.jinja](bench-harness/config/multifuzz-all.jinja) template to generate fuzzer configurations to run using Docker. The evaluation can be manually configured to run on multiple machines by either modifying the template to execute a different subset of binaries on different machines, or by choosing a different set of trials to run in the trials array (e.g., machine 1: `trials: [0,1]`, machine 2: `trials: [2,3,4]`).

After execution, the `./bench-harness/output/multifuzz/` directory contain a `multifuzz-all`, subdirectory. (If running on multiple machines, you should merge the folders from each machine before running the analysis script).


These results can be automatically analyzed by running:

```
./analyze-results.sh
```

(Note: you may see warnings saying that some MultiFuzz data is missing, this is to be expected as the same script is used for the ablation study below).

The output of the analysis is located at:

* `./analysis/output/coverage.svg`: Coverage over time graph for all binaries (Figure 11).
* `./analysis/output/total_blocks_per_trial.csv`: Raw coverage information used for creating block coverage statistics (Table 2).


## Ablation Study

The benchmark trials required for the ablation study can be executed by running:

```
multifuzz-ablation.sh [<parallel-tasks-to-run>]
```

WARNING: This requires a significant amount of CPU hours to execute (~470 CPU days)

This experiment uses a similar set up to the coverage evaluation except the [multifuzz-ablation.jinja](bench-harness/config/multifuzz-ablation.jinja) template is used to generate the fuzzer configuration instead. Similar to the previous experiment this template file can be modified to run a subset of the trials.

After execution, the `./bench-harness/output/multifuzz/` directory should now contain `multifuzz-ext`, `multifuzz-havoc`, `multifuzz-i2s`, `multifuzz-trim`. If running on multiple machines, you should merge the folders from each machine before running the analysis script (If running on multiple machines, you should merge the folders from each machine before running the analysis script).

These results can be automatically analyzed by re-running:

```
./analyze-results.sh
```

The output of the analysis is located at:

* `./analysis/output/median_coverage.csv`: Median coverage for each binary with comparisons to the full version of the fuzzer (Table 1 of the paper). Note: in the paper version of Table 1, the percentages are relative to the previous column instead of relative to the full version of the fuzzer.
