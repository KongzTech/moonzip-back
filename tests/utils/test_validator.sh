#!/bin/bash
solana-test-validator --reset -q --rpc-port 18899 \
	--geyser-plugin-config config/test/geyser_grpc.json \
	--account 4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf tests/external/pumpfun/global.json \
	--account Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1 tests/external/pumpfun/event_authority.json \
	--account TSLvdd1pWpHVjahSpsvCXUbgwsL3JAcvokwaKt1eokM tests/external/pumpfun/mint_authority.json \
	--account 9DCxsMizn3H1hprZ7xWe6LDzeUeZBksYFpBWBtSf1PQX tests/external/raydium/config_account.json \
	--account 7YttLkHDoNj9wyDur5pM1ejNaAvT9X4eqaYcHQqtj2G5 tests/external/raydium/fee_account.json \
	--upgradeable-program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P tests/external/pumpfun/pumpfun.so mau6Cw3hZX7sPNtDcq69wNyyMbsNcUrubRmTPvtnkTN \
	--upgradeable-program metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s tests/external/mpl_metadata.so mau6Cw3hZX7sPNtDcq69wNyyMbsNcUrubRmTPvtnkTN \
	--upgradeable-program LocpQgucEQHbqNABEYvBvwoxCPsSbG91A1QaQhQQqjn tests/external/locker.so mau6Cw3hZX7sPNtDcq69wNyyMbsNcUrubRmTPvtnkTN \
	--upgradeable-program srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX tests/external/serum_openbook.so mau6Cw3hZX7sPNtDcq69wNyyMbsNcUrubRmTPvtnkTN \
	--upgradeable-program 675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8 tests/external/raydium/raydium_amm.so mau6Cw3hZX7sPNtDcq69wNyyMbsNcUrubRmTPvtnkTN \
	--upgradeable-program 544hmhQ5N72wv8aJFz92sgRMnDEqwmSuzGtG8T8CPgNb ./moonzip.so mau6Cw3hZX7sPNtDcq69wNyyMbsNcUrubRmTPvtnkTN
