#!/bin/sh

exec env \
    CRATE=smtp-message \
    TARGET=fuzz_command_parser \
    DICT=smtp-command.dict \
    ./run-fuzzer.sh "$0" "$@"
