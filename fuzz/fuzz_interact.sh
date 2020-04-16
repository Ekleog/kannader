#!/bin/sh

exec env \
    CRATE=smtp-server \
    TARGET=fuzz_interact \
    DICT=smtp-command.dict \
    ./run-fuzzer.sh "$0" "$@"
