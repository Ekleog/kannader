#!/bin/sh

exec env \
    CRATE=smtp-message \
    TARGET=fuzz_data_parser \
    DICT=smtp-data.dict \
    ./run-fuzzer.sh "$0" "$@"
