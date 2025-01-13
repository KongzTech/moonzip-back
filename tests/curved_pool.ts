import * as anchor from "@coral-xyz/anchor";
import { Program, SystemProgram } from "@coral-xyz/anchor";
import {
  getAccount,
  getAssociatedTokenAddress,
  getAssociatedTokenAddressSync,
  getMinimumBalanceForRentExemptAccount,
} from "@solana/spl-token";
import { Moonzip } from "../target/types/moonzip";
import {
  airdrop,
  getAuthority,
  keypairFromFile,
  mintToken,
  tokenBalance,
  tokenInit,
} from "./utils";
import { BN } from "bn.js";
import { Keypair, LAMPORTS_PER_SOL, PublicKey } from "@solana/web3.js";
import { expect } from "chai";
import * as chai from "chai";
import chaiAsPromised from "chai-as-promised";
import { token } from "@coral-xyz/anchor/dist/cjs/utils";
chai.use(chaiAsPromised);

export function getCurvedPoolAddress(pool_mint: PublicKey) {
  const main_program = anchor.workspace.Moonzip as Program<Moonzip>;
  const [poolAddress, _] = PublicKey.findProgramAddressSync(
    [anchor.utils.bytes.utf8.encode("curved-pool"), pool_mint.toBytes()],
    main_program.programId
  );
  return poolAddress;
}

export function pumpfunLikeConfig() {
  return {
    curve: {
      initialVirtualTokenReserves: new BN(1073000000000000),
      initialVirtualSolReserves: new BN(30000000000),
      initialRealTokenReserves: new BN(793100000000000),
      totalTokenSupply: new BN(1000000000000000),
    },
    tokenDecimals: 9,
    lamportsToClose: new BN("1000000000000000000"),
  };
}

async function provideConfig() {
  const authority = getAuthority();
  const main_program = anchor.workspace.Moonzip as Program<Moonzip>;
  const connection = main_program.provider.connection;
  let signature = await main_program.methods
    .setCurvedPoolGlobalConfig(pumpfunLikeConfig())
    .accounts({
      authority: authority.publicKey,
    })
    .signers([authority])
    .rpc();
  await connection.confirmTransaction(signature);
  console.log("global config provided for curved pool");
}

export async function createCurvedPool(mint: Keypair): Promise<PublicKey> {
  const main_program = anchor.workspace.Moonzip as Program<Moonzip>;
  const connection = main_program.provider.connection;

  const config = {
    minTradeableSol: null,
    maxLamports: new BN(LAMPORTS_PER_SOL),
  };

  const authority = getAuthority();
  const poolAddress = getCurvedPoolAddress(mint.publicKey);

  await airdrop(authority.publicKey, new BN(LAMPORTS_PER_SOL));
  await provideConfig();

  let signature = await main_program.methods
    .createCurvedPool({ config: config })
    .accounts({
      authority: authority.publicKey,
      mint: mint.publicKey,
    })
    .signers([authority, mint])
    .rpc();
  await connection.confirmTransaction(signature);
  console.log(`pool created: ${poolAddress}`);
  return poolAddress;
}

describe("curved pool", () => {
  anchor.setProvider(anchor.AnchorProvider.env());

  const main_program = anchor.workspace.Moonzip as Program<Moonzip>;
  const provider = main_program.provider as anchor.AnchorProvider;
  const connection = provider.connection;
  const authority = getAuthority();

  it("basic balances equality", async () => {
    const creator = anchor.web3.Keypair.generate();

    const user = anchor.web3.Keypair.generate();
    const curveConfig = pumpfunLikeConfig();

    const config = {
      minTradeableSol: null,
      maxLamports: new BN(LAMPORTS_PER_SOL),
    };

    const buyAmount = new BN(100000000);
    const sellAmount = new BN(80000000);
    const poolMint = anchor.web3.Keypair.generate();

    const poolAddress = await createCurvedPool(poolMint);

    let state = await main_program.account.curvedPool.fetch(poolAddress);

    expect(state.status).to.eql({ active: {} });
    expect(state.mint).to.eql(poolMint.publicKey);
    expect(state.config.maxLamports.toNumber()).to.eql(
      config.maxLamports.toNumber()
    );

    expect(state.curve.virtualTokenReserves.toNumber()).to.eql(
      curveConfig.curve.initialVirtualTokenReserves.toNumber()
    );
    expect(state.curve.virtualSolReserves.toNumber()).to.eql(
      curveConfig.curve.initialVirtualSolReserves.toNumber()
    );
    expect(state.curve.realTokenReserves.toNumber()).to.eql(
      curveConfig.curve.initialRealTokenReserves.toNumber()
    );
    expect(state.curve.realSolReserves.toNumber()).to.eql(0);
    expect(state.curve.totalTokenSupply.toNumber()).to.eql(
      curveConfig.curve.totalTokenSupply.toNumber()
    );

    console.log(
      `starting to purchasing from pool with owner ${user.publicKey}`
    );
    await airdrop(user.publicKey, new BN(LAMPORTS_PER_SOL));

    const preBuyBalance = await connection.getBalance(user.publicKey);
    let signature = await main_program.methods
      .buyFromCurvedPool({ tokens: buyAmount, maxSolCost: new BN(100000) })
      .accounts({
        authority: authority.publicKey,
        mint: poolMint.publicKey,
        user: user.publicKey,
      })
      .signers([authority, user])
      .rpc();
    await connection.confirmTransaction(signature);
    const solsLost =
      preBuyBalance - (await connection.getBalance(user.publicKey));
    expect(solsLost).to.gt(0);
    const actualSolAmount =
      solsLost - (await getMinimumBalanceForRentExemptAccount(connection));

    state = await main_program.account.curvedPool.fetch(poolAddress);
    let expectedState = {
      realTokenReserves: curveConfig.curve.initialRealTokenReserves.sub(
        new BN(buyAmount)
      ),
      realSolReserves: new BN(actualSolAmount),
      totalTokenSupply: curveConfig.curve.totalTokenSupply,
    };

    expect(state.curve.realTokenReserves.toNumber()).to.eql(
      expectedState.realTokenReserves.toNumber()
    );
    expect(state.curve.realSolReserves.toNumber()).to.eql(
      expectedState.realSolReserves.toNumber()
    );
    expect(state.curve.totalTokenSupply.toNumber()).to.eql(
      expectedState.totalTokenSupply.toNumber()
    );

    const preSellBalance = await connection.getBalance(user.publicKey);

    console.log("starting to selling back to pool");
    signature = await main_program.methods
      .sellFromCurvedPool({ tokens: sellAmount, minSolOutput: new BN(0) })
      .accounts({
        authority: authority.publicKey,
        mint: poolMint.publicKey,
        user: user.publicKey,
      })
      .signers([authority, user])
      .rpc();
    await connection.confirmTransaction(signature);

    const addedSols =
      (await connection.getBalance(user.publicKey)) - preSellBalance;
    expect(addedSols).to.gt(0);
    const finalTokenBalance = await tokenBalance(
      poolMint.publicKey,
      user.publicKey
    );
    expect(finalTokenBalance).to.eql(buyAmount.sub(sellAmount).toNumber());

    state = await main_program.account.curvedPool.fetch(poolAddress);
    expectedState.realTokenReserves = expectedState.realTokenReserves.add(
      new BN(sellAmount)
    );
    expectedState.realSolReserves = expectedState.realSolReserves.sub(
      new BN(addedSols)
    );
    expect(expectedState.realSolReserves.toNumber()).to.gt(0);

    expect(state.curve.realTokenReserves.toNumber()).to.eql(
      expectedState.realTokenReserves.toNumber()
    );
    expect(state.curve.realSolReserves.toNumber()).to.eql(
      expectedState.realSolReserves.toNumber()
    );
    expect(state.curve.totalTokenSupply.toNumber()).to.eql(
      expectedState.totalTokenSupply.toNumber()
    );
  });
});
