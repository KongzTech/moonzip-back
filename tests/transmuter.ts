import { Program } from "@coral-xyz/anchor";
import {
  airdrop,
  getAuthority as getAuthority,
  mintToken,
  tokenBalance,
  tokenInit,
} from "./utils";
import { Moonzip } from "../target/types/moonzip";
import * as anchor from "@coral-xyz/anchor";
import { createCurvedPool } from "./curved_pool";
import { BN } from "bn.js";
import * as chai from "chai";
import chaiAsPromised from "chai-as-promised";
import { LAMPORTS_PER_SOL, PublicKey } from "@solana/web3.js";
import { expect } from "chai";
import {
  createTransferInstruction,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
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

  const main_program = anchor.workspace.Moonzip as Program<Moonzip>;
  const provider = main_program.provider as anchor.AnchorProvider;
  const connection = provider.connection;
  const authority = getAuthority();

  it("happy path", async () => {
    const user = anchor.web3.Keypair.generate();
    const curveMint = anchor.web3.Keypair.generate();
    const fromMint = anchor.web3.Keypair.generate();
    const transmuterBalance = new BN("10000000000");
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

    await createCurvedPool(curveMint);

    const transmuterAddress = getTransmuterAddress(
      fromMint.publicKey,
      curveMint.publicKey
    );
    let signature = await main_program.methods
      .createTransmuter()
      .accounts({
        authority: authority.publicKey,
        fromMint: fromMint.publicKey,
        toMint: curveMint.publicKey,
      })
      .signers([authority])
      .rpc();
    await connection.confirmTransaction(signature);
    console.log("created transmuter with key", transmuterAddress.toBase58());

    signature = await main_program.methods
      .buyFromCurvedPool({
        tokens: transmuterBalance,
        maxSolCost: new BN(1000000),
      })
      .accounts({
        authority: authority.publicKey,
        user: authority.publicKey,
        mint: curveMint.publicKey,
      })
      .signers([authority])
      .rpc();
    await connection.confirmTransaction(signature);
    console.log("bought from curved pool for user", user.publicKey.toBase58());

    signature = await main_program.methods
      .initTransmuterForCurve()
      .accounts({
        authority: authority.publicKey,
        fromMint: fromMint.publicKey,
        toMint: curveMint.publicKey,
      })
      .signers([authority])
      .rpc();
    await connection.confirmTransaction(signature);
    console.log("initialized transmuter for curve");

    const tx = new anchor.web3.Transaction().add(
      createTransferInstruction(
        getAssociatedTokenAddressSync(curveMint.publicKey, authority.publicKey),
        getAssociatedTokenAddressSync(
          curveMint.publicKey,
          transmuterAddress,
          true
        ),
        authority.publicKey,
        transmuterBalance.toNumber()
      )
    );

    await provider.sendAndConfirm(tx, [authority]);

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
    signature = await main_program.methods
      .sellFromCurvedPool({
        tokens: new BN(userToBalance),
        minSolOutput: new BN(0),
      })
      .accounts({
        authority: authority.publicKey,
        mint: curveMint.publicKey,
        user: user.publicKey,
      })
      .signers([authority, user])
      .rpc();
    await connection.confirmTransaction(signature);
    console.log("sold from curved pool for user ", user.publicKey.toBase58());
    const postSellBalance = await connection.getBalance(user.publicKey);
    const addedSols = postSellBalance - preSellBalance;
    expect(addedSols).to.eql(userFromBalance.toNumber());
  });
});
