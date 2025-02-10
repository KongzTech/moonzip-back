# Moonzip

## Fee structure

We take 1% on each buy and 1% on each sell.
For your convenience, we would never require you to pay anything extra on top of
amount _you_ set.
This sets us apart from other trading platforms, like pumpfun.

- For buy

We take fee "as part of" the `SOL` you trade.

You want to buy for `100` `SOL`, we take `100 * 0.01 = 1` `SOL`,
and you receive as much tokens, as specific algorithm dictates for `100 - 1 = 99` `SOL`.

- For sell

We take fee "as part of" the `SOL` you trade, just as with buy.
You sell 1000 `TOKEN`, algorithm dictates you should receive `100` `SOL`.
We take `100 * 0.01 = 1` `SOL` as a fee, so instead you receive `100 - 1 = 99` `SOL`.

## Integrations

### Pumpfun

#### Fee structure

It takes `1%` on each buy and `1%` on each sell.

- For buy

It takes fee "on top of" the base amount:
You buy for `1000` `SOL`, it would additionally require `10` `SOL` for fee.
So that in reality you would spend `1010` `SOL`.

- For sell

It takes fee "as part of" the resulting sol amount:
You sell tokens for `1000` `SOL`, it would take `10` `SOL` for fee.
So that in reality you would spend `1000 - 10 = 990` `SOL`;
