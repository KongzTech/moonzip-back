import express, { Request, Response } from "express";
import * as crypto from "crypto";

const app = express();
const port = 14001;

// Middleware to parse JSON bodies
app.use(express.json());

// Mock API Key for authentication
const MOCK_API_KEY = "mock-api-key";

// Helper function to generate random CID (Content Identifier)
const generateRandomCID = (): string => {
  return `Qm${crypto.randomBytes(20).toString("hex")}`;
};

function validApiKey(req: Request, res: Response) {
  const apiKey = req.headers["authorization"];
  if (apiKey !== `Bearer ${MOCK_API_KEY}`) {
    res.status(403).json({ error: "Invalid API Key" });
    return false;
  }
  return true;
}

// Test Authentication Endpoint
app.get("/data/testAuthentication", (req: Request, res: Response) => {
  if (!validApiKey(req, res)) {
    return;
  }
  res.status(200).json({ message: "Congratulations! You are authenticated." });
});

// Pin File to IPFS Endpoint
app.post("/pinning/pinFileToIPFS", (req: Request, res: Response) => {
  if (!validApiKey(req, res)) {
    return;
  }

  // Generate a random CID and IPFS URL
  const cid = generateRandomCID();

  // Mock response
  res.status(200).json({
    IpfsHash: cid,
    PinSize: Math.floor(Math.random() * 1000),
    Timestamp: new Date().toISOString(),
    isDuplicate: false,
  });
});

// Start the server
app.listen(port, () => {
  console.log(`Mock Pinata server is running on http://localhost:${port}`);
});
