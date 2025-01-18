import { Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import * as anchor from "@coral-xyz/anchor";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createAssociatedTokenAccountIdempotentInstruction,
  createInitializeMintInstruction,
  createMintToInstruction,
  getAssociatedTokenAddress,
  getAssociatedTokenAddressSync,
  getMinimumBalanceForRentExemptMint,
  MINT_SIZE,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { BN, Program } from "@coral-xyz/anchor";
import { Moonzip } from "../target/types/moonzip";

const fs = require("node:fs");

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

export function takeFee(amount: BN, fee: number) {
  const feeAmount = amount.mul(new BN(fee)).div(new BN(10000));
  return amount.sub(feeAmount);
}

export function getProjectAddress(projectId: BN) {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("project"), projectId.toArrayLike(Buffer, "le", 16)],
    anchor.workspace.Moonzip.programId
  )[0];
}

export async function createProject(owner: Keypair, projectId: BN, schema) {
  const provider = getProvider();
  const authority = getAuthority();
  const address = getProjectAddress(projectId);
  const main_program = anchor.workspace.Moonzip as Program<Moonzip>;
  const tx = await main_program.methods
    .createProject({ id: { 0: projectId }, schema: schema })
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
