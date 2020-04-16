#!/bin/sh

cd ../smtp-message
exec cargo fuzz run --all-features --jobs 4 $* fuzz_reply_parser \
    -- -dict=../fuzz/smtp-reply.dict
