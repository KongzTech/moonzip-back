import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
} from "@solana/web3.js";
import * as anchor from "@coral-xyz/anchor";
import { BN, Program } from "@coral-xyz/anchor";
import {
  createAssociatedTokenAccountIdempotentInstruction,
  createInitializeMintInstruction,
  createMintToInstruction,
  getAssociatedTokenAddressSync,
  getMinimumBalanceForRentExemptMint,
  MINT_SIZE,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { Moonzip } from "../../target/types/moonzip";

const fs = require("node:fs");
export const MZIP_FEE = 100;

export function keypairFromFile(path: string): Keypair {
  const data = fs.readFileSync(path, "utf8");
  return anchor.web3.Keypair.fromSecretKey(Uint8Array.from(JSON.parse(data)));
}

export function getAuthority(): Keypair {
  return keypairFromFile(`./keys/test/${process.env.MOONZIP_AUTHORITY}.json`);
}

export function getProvider(): anchor.AnchorProvider {
  const main_program = anchor.workspace.Moonzip as Program<Moonzip>;
  return main_program.provider as anchor.AnchorProvider;
}

export async function airdrop(to: PublicKey, amount: anchor.BN) {
  const _provider = getProvider();
  const signature = await _provider.connection.requestAirdrop(
    to,
    amount.toNumber()
  );
  await _provider.connection.confirmTransaction(signature);
}

export async function tokenInit(owner: Keypair, mintKeypair: Keypair) {
  const provider = getProvider();

  const lamports = await getMinimumBalanceForRentExemptMint(
    provider.connection
  );

  const decimals = 6;
  const freezeAuthority = null;

  const tx = new anchor.web3.Transaction().add(
    SystemProgram.createAccount({
      fromPubkey: provider.wallet.publicKey,
      newAccountPubkey: mintKeypair.publicKey,
      space: MINT_SIZE,
      lamports,
      programId: TOKEN_PROGRAM_ID,
    }),
    createInitializeMintInstruction(
      mintKeypair.publicKey,
      decimals,
      owner.publicKey,
      freezeAuthority,
      TOKEN_PROGRAM_ID
    )
  );

  await provider.sendAndConfirm(tx, [mintKeypair]);
}

export async function mintToken(
  owner: Keypair,
  token: PublicKey,
  to: PublicKey,
  amount: anchor.BN
) {
  const provider = getProvider();
  const ata = getAssociatedTokenAddressSync(token, to, true);

  const tx = new anchor.web3.Transaction()
    .add(
      createAssociatedTokenAccountIdempotentInstruction(
        owner.publicKey,
        ata,
        to,
        token,
        TOKEN_PROGRAM_ID
      )
    )
    .add(
      createMintToInstruction(token, ata, owner.publicKey, amount.toNumber())
    );

  await provider.sendAndConfirm(tx, [owner]);
  console.log(`minted ${amount.toString()} tokens to ${to.toBase58()}`);
}

export async function tokenBalance(mint: PublicKey, owner: PublicKey) {
  const provider = getProvider();
  const response = (
    await provider.connection.getTokenAccountBalance(
      getAssociatedTokenAddressSync(mint, owner)
    )
  ).value;
  return parseInt(response.amount);
}

export function feeAmount(amount: BN, fee: number) {
  const feeAmount = amount.mul(new BN(fee)).div(new BN(10000));
  return feeAmount;
}

export function restoreFullAmount(withAppliedFee: BN) {
  return withAppliedFee.mul(new BN(10000)).div(new BN(10000 - MZIP_FEE));
}

export function removeFeePart(amount: BN) {
  return amount.mul(new BN(10000)).div(new BN(10000 + MZIP_FEE));
}

export function approxEquals(a: BN, b: BN, tolerance: BN = new BN(1)) {
  if (a.sub(b).abs().lte(tolerance)) {
    return true;
  }
  throw new Error(
    `${a.toString()} is different from ${b.toString()}, bypassing tolerance ${tolerance}`
  );
}

export function getProjectAddress(projectId: BN) {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("project"), projectId.toArrayLike(Buffer, "le", 16)],
    anchor.workspace.Moonzip.programId
  )[0];
}

export function feeAddress() {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("fee")],
    anchor.workspace.Moonzip.programId
  )[0];
}

export async function createProject(owner: Keypair, projectId: BN, schema) {
  const provider = getProvider();
  const authority = getAuthority();
  const address = getProjectAddress(projectId);
  const main_program = anchor.workspace.Moonzip as Program<Moonzip>;
  const tx = await main_program.methods
    .createProject({
      id: { 0: projectId },
      schema: schema,
      creatorDeposit: new BN(0),
    })
    .accounts({
      authority: authority.publicKey,
      creator: owner.publicKey,
      project: address,
    })
    .signers([authority, owner])
    .rpc();
  await provider.connection.confirmTransaction(tx);
  console.log(`created project ${address.toBase58()}`);
  return address;
}

export function pumpfunLikeConfig() {
  return {
    curve: {
      initialVirtualTokenReserves: new BN("1073000000000000"),
      initialVirtualSolReserves: new BN("30000000000"),
      initialRealTokenReserves: new BN("793100000000000"),
      totalTokenSupply: new BN("1000000000000000"),
    },
    tokenDecimals: 9,
    pool: {
      minTradeableSol: new BN(1000),
      minSolToClose: new BN(LAMPORTS_PER_SOL * 1e-5),
    },
  };
}

let CONFIG_INIT = false;

export async function provideGlobalConfig() {
  // we upload same config, no sense for re-uploading
  if (CONFIG_INIT) {
    return;
  }
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

  signature = await main_program.methods
    .setFeeConfig({
      onBuy: { 0: MZIP_FEE },
      onSell: { 0: MZIP_FEE },
    })
    .accounts({
      authority: authority.publicKey,
    })
    .signers([authority])
    .rpc();
  await connection.confirmTransaction(signature);
  console.log("fee config provided");
}

export async function beforeAll() {
  const main_program = anchor.workspace.Moonzip as Program<Moonzip>;
  await airdrop(main_program.provider.publicKey, new BN(LAMPORTS_PER_SOL));
  await airdrop(getAuthority().publicKey, new BN(LAMPORTS_PER_SOL));
  await provideGlobalConfig();
}
