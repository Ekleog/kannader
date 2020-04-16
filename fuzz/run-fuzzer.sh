#!/bin/sh

# Note: This is intended to be called by one of the `fuzz_*` commands, hence the
# weird API
if [ "$#" -lt 3 -o "$2" "!=" "--jobs" ]; then
    echo "Usage: $1 --jobs [number of jobs] [other arguments to cargo fuzz]"
    exit 1
fi

njobs="$3"
shift 3

cd "../$CRATE"
exec cargo fuzz run --all-features --jobs "$njobs" "$@" "$TARGET" \
    -- -dict="../fuzz/$DICT"
