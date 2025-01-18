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
  createProject,
  getAuthority,
  keypairFromFile,
  mintToken,
  tokenBalance,
  tokenInit,
} from "./utils";
import { BN } from "bn.js";
import { LAMPORTS_PER_SOL, PublicKey } from "@solana/web3.js";
import { expect } from "chai";
import * as chai from "chai";
import chaiAsPromised from "chai-as-promised";
chai.use(chaiAsPromised);

function getPoolAddress(mint: PublicKey) {
  const main_program = anchor.workspace.Moonzip as Program<Moonzip>;
  const [poolAddress, _] = PublicKey.findProgramAddressSync(
    [anchor.utils.bytes.utf8.encode("static-pool"), mint.toBytes()],
    main_program.programId
  );
  return poolAddress;
}

describe("static pool", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());

  const main_program = anchor.workspace.Moonzip as Program<Moonzip>;
  const provider = main_program.provider as anchor.AnchorProvider;
  const connection = provider.connection;
  const authority = getAuthority();

  it("happy path", async () => {
    const creator = anchor.web3.Keypair.generate();

    const firstBuyer = anchor.web3.Keypair.generate();
    const secondBuyer = anchor.web3.Keypair.generate();

    const config = {
      closeConditions: {
        finishTs: null,
        maxLamports: new BN(500),
      },
      minPurchaseLamports: new BN(10),
    };

    const firstAmount = new BN(200);
    const secondAmount = new BN(300);
    const totalAmount = firstAmount.toNumber() + secondAmount.toNumber();

    const mint = anchor.web3.Keypair.generate();
    const poolAddress = getPoolAddress(mint.publicKey);

    await airdrop(creator.publicKey, new BN(LAMPORTS_PER_SOL));
    await airdrop(authority.publicKey, new BN(LAMPORTS_PER_SOL));
    let randomId = new BN(Math.floor(Math.random() * 100000).toString());
    await createProject(creator, randomId, {
      useStaticPool: true,
      curvePool: {
        moonzip: {},
      },
    });

    let signature = await main_program.methods
      .createStaticPool({ config: config, projectId: { 0: randomId } })
      .accounts({
        authority: authority.publicKey,
        mint: mint.publicKey,
      })
      .signers([authority, mint])
      .rpc();
    await connection.confirmTransaction(signature);
    let state = await main_program.account.staticPool.fetch(
      getPoolAddress(mint.publicKey)
    );

    expect(state.collectedLamports.toNumber()).to.eql(0);
    expect(state.state).to.eql({ active: {} });
    expect(state.mint).to.eql(mint.publicKey);
    expect(state.config.closeConditions.maxLamports.toNumber()).to.eql(
      config.closeConditions.maxLamports.toNumber()
    );
    expect(state.config.minPurchaseLamports.toNumber()).to.eql(
      config.minPurchaseLamports.toNumber()
    );

    const tokenRent = await getMinimumBalanceForRentExemptAccount(connection);

    await airdrop(
      firstBuyer.publicKey,
      new BN(firstAmount.toNumber() + tokenRent)
    );
    await airdrop(
      secondBuyer.publicKey,
      new BN(secondAmount.toNumber() + tokenRent)
    );

    console.log("starting to purchasing from static pool");

    signature = await main_program.methods
      .buyFromStaticPool({ amount: firstAmount })
      .accounts({
        authority: authority.publicKey,
        mint: mint.publicKey,
        user: firstBuyer.publicKey,
      })
      .signers([authority, firstBuyer])
      .rpc();
    await connection.confirmTransaction(signature);

    state = await main_program.account.staticPool.fetch(
      getPoolAddress(mint.publicKey)
    );

    expect(state.collectedLamports.toNumber()).to.eql(firstAmount.toNumber());
    expect(state.state).to.eql({ active: {} });

    let token_account = await getAccount(
      connection,
      getAssociatedTokenAddressSync(mint.publicKey, firstBuyer.publicKey)
    );
    expect(Number(token_account.amount)).to.eql(firstAmount.toNumber());

    signature = await main_program.methods
      .buyFromStaticPool({ amount: secondAmount })
      .accounts({
        authority: authority.publicKey,
        mint: mint.publicKey,
        user: secondBuyer.publicKey,
      })
      .signers([authority, secondBuyer])
      .rpc();
    await connection.confirmTransaction(signature);

    state = await main_program.account.staticPool.fetch(poolAddress);

    expect(state.collectedLamports.toNumber()).to.eql(
      firstAmount.toNumber() + secondAmount.toNumber()
    );
    expect(state.state).to.eql({ closed: {} });

    const tokenOwner = anchor.web3.Keypair.generate();
    await airdrop(tokenOwner.publicKey, new BN(LAMPORTS_PER_SOL));

    console.log("starting to graduate static pool");
    let fundsReceiver = anchor.web3.Keypair.generate();
    await airdrop(fundsReceiver.publicKey, new BN(LAMPORTS_PER_SOL));

    signature = await main_program.methods
      .graduateStaticPool()
      .accounts({
        authority: authority.publicKey,
        fundsReceiver: fundsReceiver.publicKey,
        pool: poolAddress,
      })
      .signers([authority])
      .rpc();
    await connection.confirmTransaction(signature);

    expect(
      main_program.account.staticPool.fetch(poolAddress)
    ).to.eventually.be.rejectedWith("shit");

    let balance = await connection.getBalance(fundsReceiver.publicKey);
    expect(balance).to.eql(LAMPORTS_PER_SOL + totalAmount);
  });
});
