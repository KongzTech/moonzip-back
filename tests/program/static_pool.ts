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
  beforeAll,
  createProject,
  getAuthority,
  getProjectAddress,
  keypairFromFile,
  mintToken,
  removeFeePart,
  restoreFullAmount,
  tokenBalance,
  tokenInit,
} from "../utils/utils";
import { BN } from "bn.js";
import { LAMPORTS_PER_SOL, PublicKey } from "@solana/web3.js";
import { expect } from "chai";
import * as chai from "chai";
import chaiAsPromised from "chai-as-promised";
import { sendTransaction, signTransaction } from "../utils/helpers";
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
  before(beforeAll);

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

    const firstAmountBare = new BN(200);
    const secondAmountBare = new BN(300);
    const firstAmount = restoreFullAmount(firstAmountBare);
    const secondAmount = restoreFullAmount(secondAmountBare);
    const totalAmount = firstAmount.toNumber() + secondAmount.toNumber();

    const mint = anchor.web3.Keypair.generate();
    const poolAddress = getPoolAddress(mint.publicKey);

    await airdrop(creator.publicKey, new BN(LAMPORTS_PER_SOL));
    await airdrop(authority.publicKey, new BN(LAMPORTS_PER_SOL));
    console.log("airdropped to authority and creator");

    let randomId = new BN(Math.floor(Math.random() * 100000).toString());
    await createProject(creator, randomId, {
      useStaticPool: true,
      curvePool: {
        moonzip: {},
      },
      devPurchase: null,
    });
    console.log("project created");

    let signature = await main_program.methods
      .createStaticPool({ config: config, projectId: { 0: randomId } })
      .accounts({
        authority: authority.publicKey,
        mint: mint.publicKey,
        project: getProjectAddress(randomId),
      })
      .signers([authority, mint])
      .rpc();
    await connection.confirmTransaction(signature);
    console.log("static pool created");
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

    let transaction = await main_program.methods
      .buyFromStaticPool({ sols: firstAmount, projectId: { 0: randomId } })
      .accounts({
        authority: authority.publicKey,
        mint: mint.publicKey,
        user: firstBuyer.publicKey,
        project: getProjectAddress(randomId),
      })
      .transaction();
    await signTransaction(connection, transaction, [authority, firstBuyer]);
    await sendTransaction(connection, transaction);
    console.log("first buyer bought");
    state = await main_program.account.staticPool.fetch(
      getPoolAddress(mint.publicKey)
    );

    expect(state.collectedLamports.toNumber()).to.eql(
      firstAmountBare.toNumber()
    );
    expect(state.state).to.eql({ active: {} });

    let token_account = await getAccount(
      connection,
      getAssociatedTokenAddressSync(mint.publicKey, firstBuyer.publicKey)
    );
    expect(Number(token_account.amount)).to.eql(firstAmountBare.toNumber());

    console.log(`second buyer: would buy for ${secondAmount.toNumber()} sols`);

    transaction = await main_program.methods
      .buyFromStaticPool({ sols: secondAmount, projectId: { 0: randomId } })
      .accounts({
        authority: authority.publicKey,
        mint: mint.publicKey,
        user: secondBuyer.publicKey,
        project: getProjectAddress(randomId),
      })
      .transaction();

    await signTransaction(connection, transaction, [authority, secondBuyer]);
    await sendTransaction(connection, transaction);
    console.log("second buyer bought");
    state = await main_program.account.staticPool.fetch(poolAddress);

    expect(state.collectedLamports.toNumber()).to.eql(
      firstAmountBare.add(secondAmountBare).toNumber()
    );
    expect(state.state).to.eql({ closed: {} });

    console.log("starting to graduate static pool");
    let fundsReceiver = anchor.web3.Keypair.generate();
    await airdrop(fundsReceiver.publicKey, new BN(LAMPORTS_PER_SOL));

    transaction = await main_program.methods
      .graduateStaticPool()
      .accounts({
        authority: authority.publicKey,
        fundsReceiver: fundsReceiver.publicKey,
        pool: poolAddress,
        project: getProjectAddress(randomId),
      })
      .signers([authority])
      .transaction();
    await signTransaction(connection, transaction, [authority]);
    await sendTransaction(connection, transaction);
    console.log("static pool graduated");
    expect(
      main_program.account.staticPool.fetch(poolAddress)
    ).to.eventually.be.rejectedWith("shit");

    let balance = await connection.getBalance(fundsReceiver.publicKey);
    expect(balance).to.eql(
      new BN(LAMPORTS_PER_SOL)
        .add(firstAmountBare)
        .add(secondAmountBare)
        .toNumber()
    );
  });
});
