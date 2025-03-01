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
export const MAX_FEE_BPS = 10000;
export const LOCKER_PROGRAM_ID = new PublicKey(
  "LocpQgucEQHbqNABEYvBvwoxCPsSbG91A1QaQhQQqjn"
);
export const RAYDIUM_CREATE_FEE_ACCOUNT = new PublicKey(
  "7YttLkHDoNj9wyDur5pM1ejNaAvT9X4eqaYcHQqtj2G5"
);

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

export async function expectNoATA(mint: PublicKey, owner: PublicKey) {
  const provider = getProvider();
  try {
    const response = (
      await provider.connection.getTokenAccountBalance(
        getAssociatedTokenAddressSync(mint, owner)
      )
    ).value;
    throw Error(
      `token account exists for owner ${owner}, mint ${mint}, balance: ${response}`
    );
  } catch (err) {
    return true;
  }
}

export async function tokenBalance(mint: PublicKey, owner: PublicKey) {
  const provider = getProvider();
  const response = (
    await provider.connection.getTokenAccountBalance(
      getAssociatedTokenAddressSync(mint, owner, true)
    )
  ).value;
  return parseInt(response.amount);
}

export function feeAmount(amount: BN, fee: number) {
  const feeAmount = amount.mul(new BN(fee)).div(new BN(MAX_FEE_BPS));
  return feeAmount;
}

export function restoreFullAmount(withAppliedFee: BN) {
  return withAppliedFee
    .mul(new BN(MAX_FEE_BPS))
    .div(new BN(MAX_FEE_BPS - MZIP_FEE));
}

export function removeFeePart(amount: BN) {
  return amount.mul(new BN(MAX_FEE_BPS - MZIP_FEE)).div(new BN(MAX_FEE_BPS));
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
    tokenDecimals: 6,
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

export function devLockEscrow(base: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("escrow"), base.toBuffer()],
    LOCKER_PROGRAM_ID
  )[0];
}

export function calculateFixedTokensPurchase(curve, tokens: BN): BN {
  let constant = curve.virtualTokenReserves.mul(curve.virtualSolReserves);
  let newVirtualTokenReserves = curve.virtualTokenReserves.sub(tokens);
  let newVirtualSolReserves = constant.div(newVirtualTokenReserves);
  let diff = newVirtualSolReserves.sub(curve.virtualSolReserves);
  console.log(`to buy ${tokens} would need ${diff} sols`);
  return restoreFullAmount(diff);
}

const JITO_TIP_KEYS = [
  "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
  "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
  "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
  "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
  "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
  "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt",
  "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
  "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
];

export async function beforeAll() {
  const main_program = anchor.workspace.Moonzip as Program<Moonzip>;

  for (let key of JITO_TIP_KEYS) {
    await airdrop(new PublicKey(key), new BN(LAMPORTS_PER_SOL));
  }

  await airdrop(main_program.provider.publicKey, new BN(LAMPORTS_PER_SOL));
  await airdrop(getAuthority().publicKey, new BN(LAMPORTS_PER_SOL));
  await provideGlobalConfig();
}
