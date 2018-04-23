#!/bin/sh

cd ../smtp-message
exec cargo fuzz run --all-features fuzz_command_parser \
    -- -dict=../fuzz/smtp-command.dict -only_ascii=1
