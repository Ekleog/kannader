#!/bin/sh

cd ../smtp-server
exec cargo fuzz run --all-features fuzz_interact \
    -- -dict=../fuzz/smtp-command.dict -only_ascii=1
