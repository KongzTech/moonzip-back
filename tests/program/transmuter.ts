import { Program } from "@coral-xyz/anchor";
import {
  airdrop,
  beforeAll,
  createProject,
  feeAddress,
  getAuthority as getAuthority,
  getProjectAddress,
  mintToken,
  MZIP_FEE,
  feeAmount,
  tokenBalance,
  tokenInit,
} from "../utils/utils";
import { Moonzip } from "../../target/types/moonzip";
import * as anchor from "@coral-xyz/anchor";
import { createCurvedPool } from "./curved_pool";
import { BN } from "bn.js";
import * as chai from "chai";
import chaiAsPromised from "chai-as-promised";
import { Keypair, LAMPORTS_PER_SOL, PublicKey } from "@solana/web3.js";
import { expect } from "chai";
import {
  createTransferInstruction,
  getAssociatedTokenAddressSync,
  transfer,
} from "@solana/spl-token";
import {
  buyFromPumpfun,
  fundPumpfunIfNeeded,
  getBondingCurveAddress,
  initPumpfunCurve,
  PUMPFUN_FEE,
  sellFromPumpfun,
} from "../utils/pumpfun";
chai.use(chaiAsPromised);

function getTransmuterAddress(fromMint: PublicKey, toMint: PublicKey) {
  const main_program = anchor.workspace.Moonzip as Program<Moonzip>;
  const [transmuterAddress, _] = PublicKey.findProgramAddressSync(
    [
      anchor.utils.bytes.utf8.encode("transmuter"),
      fromMint.toBytes(),
      toMint.toBytes(),
    ],
    main_program.programId
  );
  return transmuterAddress;
}

describe("transmuter", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());
  before(beforeAll);

  const main_program = anchor.workspace.Moonzip as Program<Moonzip>;
  const provider = main_program.provider as anchor.AnchorProvider;
  const connection = provider.connection;
  const authority = getAuthority();

  it("happy path for curved pool", async () => {
    const user = anchor.web3.Keypair.generate();
    const curveMint = anchor.web3.Keypair.generate();
    const fromMint = anchor.web3.Keypair.generate();
    const userFromBalance = new BN("100000");

    await airdrop(user.publicKey, new BN(LAMPORTS_PER_SOL));
    await airdrop(authority.publicKey, new BN(LAMPORTS_PER_SOL));

    await tokenInit(authority, fromMint);
    await mintToken(
      authority,
      fromMint.publicKey,
      user.publicKey,
      userFromBalance
    );

    let randomId = new BN(Math.floor(Math.random() * 100000).toString());
    await createProject(user, randomId, {
      useStaticPool: false,
      curvePool: {
        moonzip: {},
      },
      devPurchase: null,
    });

    const poolAddress = await createCurvedPool(randomId, curveMint);

    const transmuterAddress = getTransmuterAddress(
      fromMint.publicKey,
      curveMint.publicKey
    );

    let signature = await main_program.methods
      .buyFromCurvedPool({
        sols: new BN(LAMPORTS_PER_SOL),
        minTokenOutput: new BN(0),
        projectId: { 0: randomId },
      })
      .accounts({
        authority: authority.publicKey,
        user: authority.publicKey,
        mint: curveMint.publicKey,
        project: getProjectAddress(randomId),
      })
      .signers([authority])
      .rpc();
    await connection.confirmTransaction(signature);
    console.log(`bought from curved pool for authority`);

    signature = await main_program.methods
      .initTransmuterForCurve()
      .accounts({
        base: {
          authority: authority.publicKey,
          fromMint: fromMint.publicKey,
          toMint: curveMint.publicKey,
          donor: authority.publicKey,
        },
        curvedPool: poolAddress,
      })
      .signers([authority])
      .rpc();
    await connection.confirmTransaction(signature);
    console.log("initialized transmuter for curve");

    const transmuter = await main_program.account.transmuter.fetch(
      transmuterAddress
    );
    expect(transmuter.toMint.toBase58()).to.eql(curveMint.publicKey.toBase58());
    expect(transmuter.fromMint.toBase58()).to.eql(
      fromMint.publicKey.toBase58()
    );

    signature = await main_program.methods
      .transmute({
        tokens: userFromBalance,
      })
      .accounts({
        authority: authority.publicKey,
        user: user.publicKey,
        fromMint: fromMint.publicKey,
        toMint: curveMint.publicKey,
      })
      .signers([authority, user])
      .rpc();
    await connection.confirmTransaction(signature);
    console.log("transmuted tokens");

    const newUserFromBalance = await tokenBalance(
      fromMint.publicKey,
      user.publicKey
    );
    expect(newUserFromBalance).to.eql(0);

    const userToBalance = await tokenBalance(
      curveMint.publicKey,
      user.publicKey
    );
    expect(userToBalance).to.gt(0);

    const preSellBalance = await connection.getBalance(user.publicKey);
    let feeBalance = await connection.getBalance(feeAddress());
    signature = await main_program.methods
      .sellFromCurvedPool({
        tokens: new BN(userToBalance),
        minSolOutput: new BN(0),
        projectId: { 0: randomId },
      })
      .accounts({
        authority: authority.publicKey,
        mint: curveMint.publicKey,
        user: user.publicKey,
        project: getProjectAddress(randomId),
      })
      .signers([authority, user])
      .rpc();
    await connection.confirmTransaction(signature);
    console.log("sold from curved pool for user ", user.publicKey.toBase58());
    const postSellBalance = await connection.getBalance(user.publicKey);
    const addedSols = postSellBalance - preSellBalance;
    expect(addedSols).to.eql(
      userFromBalance.toNumber() -
        feeAmount(userFromBalance, MZIP_FEE).toNumber()
    );

    let addedFee = (await connection.getBalance(feeAddress())) - feeBalance;
    expect(addedFee).to.eql(feeAmount(userFromBalance, MZIP_FEE).toNumber());
  });

  it("happy path for pumpfun bonding curve", async () => {
    const user = anchor.web3.Keypair.generate();
    const curveMint = anchor.web3.Keypair.generate();
    const fromMint = anchor.web3.Keypair.generate();
    const transmuterBalance = new BN("10000000000");
    const userFromBalance = new BN("100000");

    await airdrop(user.publicKey, new BN(LAMPORTS_PER_SOL));
    await airdrop(authority.publicKey, new BN(LAMPORTS_PER_SOL));
    await fundPumpfunIfNeeded();

    await tokenInit(authority, fromMint);
    await mintToken(
      authority,
      fromMint.publicKey,
      user.publicKey,
      userFromBalance
    );

    const curveAddress = await initPumpfunCurve(authority, curveMint);
    console.log(
      "initialized pumpfun curve with mint",
      curveMint.publicKey.toBase58()
    );

    const transmuterAddress = getTransmuterAddress(
      fromMint.publicKey,
      curveMint.publicKey
    );
    await buyFromPumpfun(curveMint.publicKey, authority, transmuterBalance);
    console.log(
      "bought from bunding curve for user",
      authority.publicKey.toBase58()
    );

    let signature = await main_program.methods
      .initTransmuterForPumpfunCurve()
      .accounts({
        base: {
          authority: authority.publicKey,
          fromMint: fromMint.publicKey,
          toMint: curveMint.publicKey,
          donor: authority.publicKey,
        },
        bondingCurve: curveAddress,
      })
      .signers([authority])
      .rpc();
    await connection.confirmTransaction(signature);
    console.log("initialized transmuter for curve");

    const transmuter = await main_program.account.transmuter.fetch(
      transmuterAddress
    );
    expect(transmuter.toMint.toBase58()).to.eql(curveMint.publicKey.toBase58());
    expect(transmuter.fromMint.toBase58()).to.eql(
      fromMint.publicKey.toBase58()
    );

    signature = await main_program.methods
      .transmute({
        tokens: userFromBalance,
      })
      .accounts({
        authority: authority.publicKey,
        user: user.publicKey,
        fromMint: fromMint.publicKey,
        toMint: curveMint.publicKey,
      })
      .signers([authority, user])
      .rpc();
    await connection.confirmTransaction(signature);
    console.log("transmuted tokens");

    const newUserFromBalance = await tokenBalance(
      fromMint.publicKey,
      user.publicKey
    );
    expect(newUserFromBalance).to.eql(0);

    const userToBalance = await tokenBalance(
      curveMint.publicKey,
      user.publicKey
    );
    expect(userToBalance).to.gt(0);

    const preSellBalance = await connection.getBalance(user.publicKey);
    await sellFromPumpfun(curveMint.publicKey, user, new BN(userToBalance));
    console.log("sold from bonding curve for user ", user.publicKey.toBase58());
    const postSellBalance = await connection.getBalance(user.publicKey);
    const addedSols = postSellBalance - preSellBalance;
    expect(addedSols).to.eql(
      userFromBalance.sub(feeAmount(userFromBalance, PUMPFUN_FEE)).toNumber()
    );
  });
});
