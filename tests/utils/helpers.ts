import { web3 } from "@coral-xyz/anchor";
import {
  Connection,
  PublicKey,
  SendTransactionError,
  Signer,
  Transaction,
} from "@solana/web3.js";

export const withTimeout = <T>(millis, promise: Promise<T>): Promise<T> => {
  let timeoutPid;
  const timeout = new Promise(
    (resolve, reject) =>
      (timeoutPid = setTimeout(
        () => reject(`Timed out after ${millis} ms.`),
        millis
      ))
  );
  return Promise.race([promise, timeout]).finally(() => {
    if (timeoutPid) {
      clearTimeout(timeoutPid);
    }
  }) as Promise<T>;
};
export const withErrorHandling = async <T>(promise: Promise<T>) => {
  try {
    return await promise;
  } catch (error) {
    throw new Error(
      "Error occurred: " +
        JSON.stringify(error) +
        "\nCause: " +
        JSON.stringify(error.cause)
    );
  }
};
export const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

export function time() {
  const nanos = process.hrtime.bigint();
  return Number(nanos / BigInt(1_000_000));
}

export async function waitForOk<T>(f: () => Promise<T>): Promise<T> {
  let start = time();
  while (time() - start < 30000) {
    try {
      return await f();
    } catch (err) {
      console.log(`promise rejected with error: ${err}, would sleep`);
    }
    await delay(2000);
  }
  throw new Error("promise is not ok in 30s, failure");
}

export const sendTransaction = async (
  connection: Connection,
  transaction: Transaction
) => {
  let signature: string;
  try {
    let wired = transaction.serialize();
    console.log("Serialized transaction len: ", wired.length);
    signature = await connection.sendEncodedTransaction(
      wired.toString("base64")
    );
  } catch (error) {
    console.log(
      "Raw failed transaction: ",
      JSON.stringify(transaction, null, 4)
    );
    if (error instanceof SendTransactionError) {
      console.log(
        "Full logs of failed transaction: ",
        await error.getLogs(connection)
      );
    }
    throw error;
  }
  console.log(`sent transaction ${signature}`);

  for (let i = 0; i < 10; i++) {
    await delay(1000);
    const status = await connection.getSignatureStatus(signature);
    if (status.value == null) {
      console.log("transaction not yet found");
      continue;
    }
    if (status.value.err) {
      throw new Error(`transaction failed: ${JSON.stringify(status)}`);
    }
    if (
      status.value.confirmationStatus == "confirmed" ||
      status.value.confirmationStatus == "finalized"
    ) {
      return signature;
    }
  }
  throw new Error("transaction not confirmed in 10 seconds");
};

export async function signTransaction(
  connection: Connection,
  tx: Transaction,
  signers: Signer[],
  feePayer?: PublicKey
): Promise<Transaction> {
  tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
  tx.feePayer = feePayer ? feePayer : signers[0].publicKey;
  for (let signer of signers) {
    tx.partialSign(signer);
  }
  return tx;
}

export async function waitOnClusterTime(
  connection: web3.Connection,
  deadline: number
) {
  const getCurrentBlockTime = async () => {
    // TODO fetch clock account can help to reduce rpc call
    const currentSlot = await connection.getSlot();
    const currentBlockTime = await connection.getBlockTime(currentSlot);
    return currentBlockTime;
  };

  while (true) {
    const currentBlockTime = await getCurrentBlockTime();
    if (currentBlockTime > deadline) {
      break;
    } else {
      await delay(1000);
      console.log(
        `current block time is ${currentBlockTime}, waiting for ${deadline}`
      );
    }
  }
}

export function currentTS() {
  return Math.floor(new Date(new Date().toUTCString()).getTime() / 1000);
}
