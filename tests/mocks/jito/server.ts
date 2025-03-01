import express, { Request, Response } from "express";
import bodyParser from "body-parser";
import cors from "cors";
import {
  Connection,
  Keypair,
  Transaction,
  SystemProgram,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import { Server } from "ws";

import { sendTransaction } from "../../utils/helpers";

const port = 13000;
const solana_rpc = process.env.SOLANA_RPC;

// Initialize Solana connection to local validator
const connection = new Connection(solana_rpc, "confirmed"); // Local Solana validator RPC endpoint

// Mock Jito Service
class MockJitoService {
  // Simulates sending a single transaction to the local Solana validator
  async sendTransaction(
    encodedTransaction: string,
    _encoding
  ): Promise<string> {
    // Decode the transaction
    const transaction = Transaction.from(
      Buffer.from(encodedTransaction, "base64")
    );
    try {
      // Send and confirm the transaction
      const signature = await sendTransaction(connection, transaction);
      return signature;
    } catch (error) {
      let keys = transaction.compileMessage().accountKeys;
      console.log(`Account keys of instruction(#${keys.length}): ${keys}`);
      console.error("Failed to send transaction:", error);
      throw new Error("Failed to send transaction");
    }
  }

  // Simulates sending a bundle of transactions to the local Solana validator
  async sendBundle(
    encodedTransactions: string[],
    encoding: string = "base64"
  ): Promise<string> {
    const bundleId = `bundle-${Math.random().toString(36).substring(7)}`; // Generate a random bundle ID
    let transactionIdx = 1;

    // Send and confirm each transaction in the bundle
    for (const encodedTx of encodedTransactions) {
      console.log(`Sending transaction #${transactionIdx}`);
      try {
        await this.sendTransaction(encodedTx, encoding);
      } catch (error) {
        console.error("Failed to send transaction in bundle:", error);
        throw new Error("Failed to send bundle");
      }
      transactionIdx += 1;
    }

    return bundleId;
  }

  // Simulates getting the status of multiple bundles from the local Solana validator
  async getBundleStatuses(bundleIds: string[]): Promise<any[]> {
    const statuses = await Promise.all(
      bundleIds.map(async (bundleId) => {
        // For simplicity, assume the bundle ID is the signature of the first transaction
        const signature = bundleId;

        // Get the transaction status from the local validator
        const status = await connection.getSignatureStatus(signature, {
          searchTransactionHistory: true,
        });
        if (!status || !status.value) {
          return null; // Transaction not found
        }

        return {
          bundle_id: bundleId,
          transactions: [signature],
          slot: status.value.slot,
          confirmation_status: status.value.confirmationStatus || "processed",
          err: status.value.err
            ? { Err: status.value.err.toString() }
            : { Ok: null },
        };
      })
    );

    return statuses.filter((status) => status !== null);
  }
}

// Create an instance of the mock service
const jitoService = new MockJitoService();

// Initialize Express app
const app = express();

// Middleware
app.use(bodyParser.json());
app.use(cors());

// Endpoint: Send Transaction (JSON-RPC)
app.post("/api/v1/transactions", async (req: Request, res: Response) => {
  const { id, jsonrpc, method, params } = req.body;
  if (method !== "sendTransaction") {
    return res.status(400).json({ error: "Invalid method" });
  }

  const [encodedTransaction, options] = params;
  const encoding = options?.encoding || "base64";

  try {
    const signature = await jitoService.sendTransaction(
      encodedTransaction,
      encoding
    );
    res.status(200).json({
      jsonrpc,
      result: signature,
      id,
    });
  } catch (error) {
    res.status(500).json({ error: "Failed to send transaction" });
  }
});

// Endpoint: Send Bundle (JSON-RPC)
app.post("/api/v1/bundles", async (req: Request, res: Response) => {
  const { id, jsonrpc, method, params } = req.body;
  if (method !== "sendBundle") {
    return res.status(400).json({ error: "Invalid method" });
  }

  const [encodedTransactions, options] = params;
  const encoding = options?.encoding || "base64";

  try {
    const bundleId = await jitoService.sendBundle(
      encodedTransactions,
      encoding
    );
    res.status(200).json({
      jsonrpc,
      result: bundleId,
      id,
    });
  } catch (error) {
    res.status(500).json({ error: "Failed to send bundle" });
  }
});

// Endpoint: Get Bundle Statuses (JSON-RPC)
app.post("/api/v1/getBundleStatuses", async (req: Request, res: Response) => {
  const { id, jsonrpc, method, params } = req.body;
  if (method !== "getBundleStatuses") {
    return res.status(400).json({ error: "Invalid method" });
  }

  const [bundleIds] = params;

  try {
    const bundleStatuses = await jitoService.getBundleStatuses(bundleIds);
    res.status(200).json({
      jsonrpc,
      result: {
        context: { slot: await connection.getSlot() }, // Current slot
        value: bundleStatuses,
      },
      id,
    });
  } catch (error) {
    res.status(500).json({ error: "Failed to get bundle statuses" });
  }
});

// Start the Express server
const server = app.listen(port, () => {
  console.log(`Mock Jito API running at http://localhost:${port}`);
});

// WebSocket Server for Tip Information
const wss = new Server({ server });

wss.on("connection", (ws) => {
  console.log("New WebSocket connection");

  // Function to generate tip data with the current time
  const generateTipData = () => {
    const now = new Date().toISOString(); // Get current time in ISO format
    return [
      {
        time: now, // Use current time
        landed_tips_25th_percentile: 1.0000000000000002e-6,
        landed_tips_50th_percentile: 0.000011000000000000001,
        landed_tips_75th_percentile: 0.0004,
        landed_tips_95th_percentile: 0.012221000000000001,
        landed_tips_99th_percentile: 0.03844800000000003,
        ema_landed_tips_50th_percentile: 0.00014160699403518529,
      },
    ];
  };

  // Send tip data immediately upon connection
  ws.send(JSON.stringify(generateTipData()));

  // Send tip data periodically (every 10 seconds)
  const interval = setInterval(() => {
    const tipData = generateTipData();
    ws.send(JSON.stringify(tipData));
  }, 10000); // 10 seconds

  // Handle client messages (if needed)
  ws.on("message", (message) => {
    console.log("Received message:", message.toString());
  });

  // Handle client disconnect
  ws.on("close", () => {
    console.log("WebSocket connection closed");
    clearInterval(interval); // Stop sending data when the client disconnects
  });
});
