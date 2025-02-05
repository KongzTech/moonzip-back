import express from "express";
import multer from "multer";
import { v4 as uuidv4 } from "uuid";
import { z } from "zod";

const port = 14002;
const app = express();
const upload = multer();

// Define the schema for validation using zod
const CreateTokenMetadataSchema = z.object({
  name: z.string().min(1, { message: "Name is required" }),
  symbol: z.string().min(1, { message: "Symbol is required" }),
  description: z.string().min(1, { message: "Description is required" }),
  twitter: z.string().optional(),
  telegram: z.string().optional(),
  website: z.string().optional(),
});

// Define the response structure
interface TokenMetadataResponse {
  metadataUri: string;
}

app.post("/api/ipfs", upload.single("file"), (req, res) => {
  // Extract text fields from the form data
  const metadata = {
    name: req.body.name,
    symbol: req.body.symbol,
    description: req.body.description,
    twitter: req.body.twitter,
    telegram: req.body.telegram,
    website: req.body.website,
  };

  // Validate the metadata against the schema
  const validationResult = CreateTokenMetadataSchema.safeParse(metadata);

  // If validation fails, return a 400 Bad Request with error details
  if (!validationResult.success) {
    return res.status(400).json({
      message: "Validation failed",
      errors: validationResult.error.errors,
    });
  }

  // Generate a random IPFS URI
  const metadataUri = `ipfs://${uuidv4()}`;

  // Prepare the response
  const response: TokenMetadataResponse = {
    metadataUri: metadataUri,
  };

  // Send the response
  res.json(response);
});

app.listen(port, () => {
  console.log(`Server is running on port ${port}`);
});
