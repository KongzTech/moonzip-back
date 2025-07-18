import * as anchor from "@coral-xyz/anchor";
import { Program, SystemProgram } from "@coral-xyz/anchor";
import {
  getAccount,
  getAssociatedTokenAddress,
  getAssociatedTokenAddressSync,
  getMinimumBalanceForRentExemptAccount,
} from "@solana/spl-token";
import { Moonzip } from "../../target/types/moonzip";
import {
  airdrop,
  approxEquals,
  beforeAll,
  createProject,
  feeAmount,
  getAuthority,
  getProjectAddress,
  keypairFromFile,
  mintToken,
  MZIP_FEE,
  provideGlobalConfig,
  pumpfunLikeConfig,
  removeFeePart,
  restoreFullAmount,
  tokenBalance,
  tokenInit,
} from "../utils/utils";
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

export async function createCurvedPool(
  project_id: BN,
  mint: Keypair
): Promise<PublicKey> {
  const main_program = anchor.workspace.Moonzip as Program<Moonzip>;
  const connection = main_program.provider.connection;

  const config = {
    minTradeableSol: null,
    maxLamports: new BN(LAMPORTS_PER_SOL),
  };

  const authority = getAuthority();
  const poolAddress = getCurvedPoolAddress(mint.publicKey);
  const projectAddress = getProjectAddress(project_id);

  await airdrop(authority.publicKey, new BN(LAMPORTS_PER_SOL));

  let signature = await main_program.methods
    .createCurvedPool({
      config: config,
      projectId: { 0: project_id },
    })
    .accounts({
      authority: authority.publicKey,
      mint: mint.publicKey,
      project: projectAddress,
    })
    .signers([authority, mint])
    .rpc();
  await connection.confirmTransaction(signature);
  console.log(`pool created: ${poolAddress}`);
  return poolAddress;
}

describe("curved pool", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  before(beforeAll);

  const main_program = anchor.workspace.Moonzip as Program<Moonzip>;
  const provider = main_program.provider as anchor.AnchorProvider;
  const connection = provider.connection;
  const authority = getAuthority();

  it("basic balances equality", async () => {
    const creator = anchor.web3.Keypair.generate();

    const user = anchor.web3.Keypair.generate();
    const curveConfig = pumpfunLikeConfig();

    const poolMint = anchor.web3.Keypair.generate();

    await airdrop(user.publicKey, new BN(LAMPORTS_PER_SOL));
    await airdrop(creator.publicKey, new BN(LAMPORTS_PER_SOL));

    let randomId = new BN(Math.floor(Math.random() * 100000).toString());
    await createProject(creator, randomId, {
      useStaticPool: false,
      curvePool: {
        moonzip: {},
      },
      devPurchase: null,
    });

    const poolAddress = await createCurvedPool(randomId, poolMint);

    let state = await main_program.account.curvedPool.fetch(poolAddress);

    expect(state.status).to.eql({ active: {} });
    expect(state.mint).to.eql(poolMint.publicKey);

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

    const preBuyBalance = await connection.getBalance(user.publicKey);
    let solToSpend = new BN(100000);
    let signature = await main_program.methods
      .buyFromCurvedPool({
        sols: solToSpend,
        minTokenOutput: new BN(0),
        projectId: { 0: randomId },
      })
      .accounts({
        authority: authority.publicKey,
        mint: poolMint.publicKey,
        user: user.publicKey,
        project: getProjectAddress(randomId),
      })
      .signers([authority, user])
      .rpc();
    await connection.confirmTransaction(signature);
    const solsLost =
      preBuyBalance - (await connection.getBalance(user.publicKey));
    expect(solsLost).to.gt(solToSpend.toNumber());
    const tokensGained = new BN(
      await tokenBalance(poolMint.publicKey, user.publicKey)
    );

    state = await main_program.account.curvedPool.fetch(poolAddress);
    let expectedState = {
      realTokenReserves:
        curveConfig.curve.initialRealTokenReserves.sub(tokensGained),
      realSolReserves: removeFeePart(new BN(solToSpend)),
      totalTokenSupply: curveConfig.curve.totalTokenSupply,
    };

    expect(state.curve.realTokenReserves.toNumber()).to.eql(
      expectedState.realTokenReserves.toNumber()
    );
    approxEquals(state.curve.realSolReserves, expectedState.realSolReserves);
    expect(state.curve.totalTokenSupply.toNumber()).to.eql(
      expectedState.totalTokenSupply.toNumber()
    );

    const preSellBalance = await connection.getBalance(user.publicKey);

    let sellAmount = new BN(Math.floor(tokensGained.toNumber() / 2));
    console.log("starting to selling back to pool", sellAmount);
    signature = await main_program.methods
      .sellFromCurvedPool({
        projectId: { 0: randomId },
        tokens: sellAmount,
        minSolOutput: new BN(0),
      })
      .accounts({
        authority: authority.publicKey,
        mint: poolMint.publicKey,
        user: user.publicKey,
        project: getProjectAddress(randomId),
      })
      .signers([authority, user])
      .rpc();
    await connection.confirmTransaction(signature);

    const addedSols =
      (await connection.getBalance(user.publicKey)) - preSellBalance;
    expect(addedSols).to.gt(0);

    console.log("added sols: ", addedSols.toString());
    const finalTokenBalance = await tokenBalance(
      poolMint.publicKey,
      user.publicKey
    );
    expect(finalTokenBalance).to.eql(tokensGained.sub(sellAmount).toNumber());

    state = await main_program.account.curvedPool.fetch(poolAddress);
    expectedState.realTokenReserves = expectedState.realTokenReserves.add(
      new BN(sellAmount)
    );

    let withFee = restoreFullAmount(new BN(addedSols));
    console.log("sol amount on sell with fee: ", withFee.toString());

    expectedState.realSolReserves = expectedState.realSolReserves.sub(withFee);
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
