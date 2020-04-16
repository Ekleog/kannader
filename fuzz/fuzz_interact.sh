#!/bin/sh

cd ../smtp-server
exec cargo fuzz run --all-features --release $* fuzz_interact \
    -- -dict=../fuzz/smtp-command.dict
