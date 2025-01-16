import { getProvider, Program } from "@coral-xyz/anchor";
import * as anchor from "@coral-xyz/anchor";
import {
  createAssociatedTokenAccountInstruction,
  getAssociatedTokenAddressSync,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
  SYSVAR_RENT_PUBKEY,
} from "@solana/web3.js";
import { Pump } from "./external/pumpfun/idl.types";
import idl from "./external/pumpfun/idl.new.json";
import { BN } from "bn.js";
import { ASSOCIATED_PROGRAM_ID } from "@coral-xyz/anchor/dist/cjs/utils/token";
import { airdrop } from "./utils";

const program = new Program<Pump>(idl as Pump);
const MPL_METADATA_PROGRAM = new PublicKey(
  "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
);
const MPL_METADATA_PREFIX = anchor.utils.bytes.utf8.encode("metadata");
export const PUMPFUN_FEE = 100;

export function getBondingCurveAddress(pool_mint: PublicKey) {
  const [poolAddress, _] = PublicKey.findProgramAddressSync(
    [anchor.utils.bytes.utf8.encode("bonding-curve"), pool_mint.toBytes()],
    program.programId
  );
  return poolAddress;
}

export function getGlobalAccountKey() {
  const [address, _] = PublicKey.findProgramAddressSync(
    [anchor.utils.bytes.utf8.encode("global")],
    program.programId
  );
  return address;
}

export async function getGlobalAccount() {
  return program.account.global.fetch(getGlobalAccountKey());
}

export function getMintAuthority() {
  const [address, _] = PublicKey.findProgramAddressSync(
    [anchor.utils.bytes.utf8.encode("mint-authority")],
    program.programId
  );
  return address;
}

export function getMetadataAccount(mint: PublicKey) {
  const [address, _] = PublicKey.findProgramAddressSync(
    [MPL_METADATA_PREFIX, MPL_METADATA_PROGRAM.toBytes(), mint.toBytes()],
    MPL_METADATA_PROGRAM
  );
  return address;
}

export async function initPumpfunCurve(
  owner: Keypair,
  mint: Keypair
): Promise<PublicKey> {
  const provider = getProvider();

  const curveAddr = getBondingCurveAddress(mint.publicKey);
  const connection = provider.connection;
  let tx = await program.methods
    .create("name", "symbol", "uri")
    .accounts({
      mint: mint.publicKey,
      mintAuthority: getMintAuthority(),
      associatedBondingCurve: getAssociatedTokenAddressSync(
        mint.publicKey,
        curveAddr,
        true
      ),
      global: getGlobalAccountKey(),
      bondingCurve: curveAddr,
      mplTokenMetadata: MPL_METADATA_PROGRAM,
      metadata: getMetadataAccount(mint.publicKey),
      user: owner.publicKey,
      systemProgram: SystemProgram.programId,
      tokenProgram: TOKEN_PROGRAM_ID,
      associatedTokenProgram: ASSOCIATED_PROGRAM_ID,
      rent: SYSVAR_RENT_PUBKEY,
    })
    .transaction();
  const signature = await connection.sendTransaction(tx, [owner, mint]);
  await connection.confirmTransaction(signature);
  return curveAddr;
}

export async function buyFromPumpfun(
  mint: PublicKey,
  user: Keypair,
  tokens: anchor.BN
) {
  const connection = program.provider.connection;
  const curveAddr = getBondingCurveAddress(mint);
  const globalAccount = await getGlobalAccount();
  const tx = new anchor.web3.Transaction().add(
    createAssociatedTokenAccountInstruction(
      user.publicKey,
      getAssociatedTokenAddressSync(mint, user.publicKey),
      user.publicKey,
      mint
    )
  );
  let signature = await connection.sendTransaction(tx, [user]);
  await connection.confirmTransaction(signature);

  signature = await program.methods
    .buy(tokens, new BN(LAMPORTS_PER_SOL / 100))
    .accounts({
      mint: mint,
      user: user.publicKey,
      bondingCurve: curveAddr,
      global: getGlobalAccountKey(),
      feeRecipient: globalAccount.feeRecipient,
      associatedBondingCurve: getAssociatedTokenAddressSync(
        mint,
        curveAddr,
        true
      ),
      associatedUser: getAssociatedTokenAddressSync(mint, user.publicKey, true),
      systemProgram: SystemProgram.programId,
      tokenProgram: TOKEN_PROGRAM_ID,
      rent: SYSVAR_RENT_PUBKEY,
    })
    .signers([user])
    .rpc();
  await connection.confirmTransaction(signature);
}

export async function sellFromPumpfun(
  mint: PublicKey,
  user: Keypair,
  tokens: anchor.BN
) {
  const connection = program.provider.connection;
  const curveAddr = getBondingCurveAddress(mint);
  const globalAccount = await getGlobalAccount();
  let signature = await program.methods
    .sell(tokens, new BN("0"))
    .accounts({
      mint: mint,
      user: user.publicKey,
      bondingCurve: curveAddr,
      global: getGlobalAccountKey(),
      feeRecipient: globalAccount.feeRecipient,
      associatedBondingCurve: getAssociatedTokenAddressSync(
        mint,
        curveAddr,
        true
      ),
      associatedUser: getAssociatedTokenAddressSync(mint, user.publicKey, true),
      systemProgram: SystemProgram.programId,
      associatedTokenProgram: ASSOCIATED_PROGRAM_ID,
      tokenProgram: TOKEN_PROGRAM_ID,
    })
    .signers([user])
    .rpc();
  await connection.confirmTransaction(signature);
}

export async function fundPumpfunIfNeeded() {
  const globalAccount = await getGlobalAccount();
  const feeRecipient = globalAccount.feeRecipient;
  const connection = program.provider.connection;
  const balance = await connection.getBalance(feeRecipient);

  if (balance === 0) {
    await airdrop(feeRecipient, new BN(LAMPORTS_PER_SOL));
  }
}
