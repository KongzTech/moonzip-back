#!/bin/bash
solana-test-validator --reset -q --rpc-port 18899 \
	--geyser-plugin-config config/test/geyser_grpc.json \
	--account 4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf tests/external/pumpfun/global.json \
	--account Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1 tests/external/pumpfun/event_authority.json \
	--account TSLvdd1pWpHVjahSpsvCXUbgwsL3JAcvokwaKt1eokM tests/external/pumpfun/mint_authority.json \
	--upgradeable-program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P tests/external/pumpfun/pumpfun.so mau6Cw3hZX7sPNtDcq69wNyyMbsNcUrubRmTPvtnkTN \
	--upgradeable-program metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s tests/external/mpl_metadata.so mau6Cw3hZX7sPNtDcq69wNyyMbsNcUrubRmTPvtnkTN \
	--upgradeable-program LocpQgucEQHbqNABEYvBvwoxCPsSbG91A1QaQhQQqjn tests/external/locker.so mau6Cw3hZX7sPNtDcq69wNyyMbsNcUrubRmTPvtnkTN \
	--upgradeable-program 544hmhQ5N72wv8aJFz92sgRMnDEqwmSuzGtG8T8CPgNb ./moonzip.so mau6Cw3hZX7sPNtDcq69wNyyMbsNcUrubRmTPvtnkTN
