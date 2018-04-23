#!/bin/sh

cd ../smtp-message
exec cargo fuzz run --all-features fuzz_reply_parser \
    -- -dict=../fuzz/smtp-reply.dict -only_ascii=1
