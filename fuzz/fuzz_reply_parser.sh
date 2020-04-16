#!/bin/sh

exec env \
    CRATE=smtp-message \
    TARGET=fuzz_reply_parser \
    DICT=smtp-reply.dict \
    ./run-fuzzer.sh "$0" "$@"
