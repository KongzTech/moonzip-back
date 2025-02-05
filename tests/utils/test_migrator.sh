#!/bin/bash
set -e

BASE_KEYS_DIR=/tmp/mzip_test_keys
mkdir -p $BASE_KEYS_DIR

for i in {0..15}; do
  solana-keygen new --no-passphrase -o $BASE_KEYS_DIR/key_$i.json &
done

migrator
