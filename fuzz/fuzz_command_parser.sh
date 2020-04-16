#!/bin/sh

cd ../smtp-message
exec cargo fuzz run --all-features --jobs 4 $* fuzz_command_parser \
    -- -dict=../fuzz/smtp-command.dict
