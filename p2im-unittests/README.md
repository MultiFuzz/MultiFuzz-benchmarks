# P2IM unit test

Runtime: 8 CPU hours (parallizable)
Memory: 30 MB per worker
Storage: 80 MB

Prerequisites: the `./build_all.sh` script in the repository root must be run before attempting to run the unittests.

This subdirectory contains the [P2IM unit tests](https://github.com/RiS3-Lab/p2im-unit_tests), with the merged groundtruth file and configuration from Fuzzware: [fuzzware-experiments/01-access-modeling-for-fuzzing/p2im-unittests](https://github.com/fuzzware-fuzzer/fuzzware-experiments/tree/cfe63a941ce89fe4ce7c8295618e9ed3bcc4ff53/01-access-modeling-for-fuzzing/p2im-unittests)

This experiment involves fuzzing each of the unit test binaries for 10 minutes, and checking that the fuzzer is able to discover inputs that reach various places in each of the binaries.

```
./run.sh [parallel-tasks-to-run]
```

After execution you should see the following printed to stdout:

```
66 successes, 0 errors
```

As part of this process a subdirectory (`workdir-p2im-unittests`) containing each of the fuzzer runs is created. Note: rerunning the `./run.sh` command will reuse any existing fuzzer runs in this directory.


These outputs are only used for this experiment and can be safely deleted after confirming the results above.
