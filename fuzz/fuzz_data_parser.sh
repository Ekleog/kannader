#!/bin/sh

cd ../smtp-message
exec cargo fuzz run --all-features --jobs 4 $* fuzz_data_parser \
    -- -dict=../fuzz/smtp-data.dict
