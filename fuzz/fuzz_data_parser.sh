#!/bin/sh

cd ../smtp-message
exec cargo fuzz run --all-features --release $* fuzz_data_parser \
    -- -dict=../fuzz/smtp-data.dict -only_ascii=1
