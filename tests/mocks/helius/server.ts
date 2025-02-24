import express, { Request, Response } from "express";
import bodyParser from "body-parser";
import cors from "cors";

const app = express();
const PORT = 4000;

app.use(cors());
app.use(bodyParser.json());

const TOTAL_MOCK_DATA = 100;

const generateMockNFTPaginate = (
  total: number,
  page: number = 1,
  limit: number = 10
) => {
  return {
    jsonrpc: "2.0",
    result: {
      total,
      limit,
      page,
      items: Array.from({ length: limit }, (_, index) => {
        const id = index + 1 + (page - 1) * limit;
        return {
          interface: "ProgrammableNFT",
          id: `MockNFTId_${id}`,
          content: {
            $schema: "https://schema.metaplex.com/nft1.0.json",
            json_uri: `https://example.com/mock-nft/${id}.json`,
            files: [
              {
                uri: `https://picsum.photos/id/${id}/200/300`,
                cdn_uri: `https://cdn.example.com/image/${id}.png`,
                mime: "image/png",
              },
            ],
            metadata: {
              attributes: [
                { value: "MockType", trait_type: "Type" },
                { value: "MockEyewear", trait_type: "Eyewear" },
                { value: "MockHeadgear", trait_type: "Headgear" },
                { value: "MockMouth", trait_type: "Mouth" },
                { value: "MockClothes", trait_type: "Clothes" },
                { value: "MockAccessory", trait_type: "Accessory" },
              ],
              description: `This is a mock NFT #${id}`,
              name: `Mock NFT ${id}`,
              symbol: `MOCK${id}`,
              token_standard: "ProgrammableNonFungible",
            },
            links: {
              image: `https://picsum.photos/id/${id}/200/300`,
              external_url: "https://www.example.com/",
            },
          },
          authorities: [
            {
              address: "MockAuthorityAddress",
              scopes: ["full"],
            },
          ],
          compression: {
            eligible: false,
            compressed: false,
            data_hash: "",
            creator_hash: "",
            asset_hash: "",
            tree: "",
            seq: 0,
            leaf_id: 0,
          },
          grouping: [
            {
              group_key: "collection",
              group_value: "MockCollectionID",
            },
          ],
          royalty: {
            royalty_model: "creators",
            target: null,
            percent: 0.05,
            basis_points: 500,
            primary_sale_happened: true,
            locked: false,
          },
          creators: [
            {
              address: "MockCreatorAddress1",
              share: 0,
              verified: true,
            },
            {
              address: "MockCreatorAddress2",
              share: 100,
              verified: false,
            },
          ],
          ownership: {
            frozen: false,
            delegated: false,
            delegate: null,
            ownership_model: "single",
            owner: "MockOwnerAddress",
          },
          supply: {
            print_max_supply: 0,
            print_current_supply: 0,
            edition_nonce: 255,
          },
          mutable: true,
          burnt: false,
          token_info: {
            supply: 1,
            decimals: 0,
            token_program: "MockTokenProgramAddress",
            associated_token_address: "MockAssociatedTokenAddress",
          },
        };
      }),
    },
  };
};
const mockData = generateMockNFTPaginate(TOTAL_MOCK_DATA);

app.post("", (req: Request, res: Response) => {
  const apiKey = req.query["api-key"] as string;
  const ownerAddress = req.query["owner-address"] as string;
  if (!apiKey) {
    return res.status(401).json({
      jsonrpc: "2.0",
      error: {
        code: -32401,
        message: "missing api key",
      },
      id: req.body.id || null,
    });
  }

  const { method, params } = req.body;

  if (!method) {
    return res.status(400).json({
      jsonrpc: "2.0",
      error: {
        code: -32603,
        message: "Method not found",
      },
      id: req.body.id || null,
    });
  }

  if (method === "getAsset") {
    const { id } = params;
    if (!id) {
      return res.status(400).json({
        jsonrpc: "2.0",
        error: {
          code: -32000,
          message: "Pubkey Validation Err:  is invalid",
        },
        id: req.body.id || null,
      });
    }

    const mockResponse = {
      jsonrpc: "2.0",
      result: {
        interface: "ProgrammableNFT",
        id: id,
        content: {
          $schema: "https://schema.metaplex.com/nft1.0.json",
          json_uri: "https://example.com/nft.json",
          files: [
            {
              uri: "https://example.com/nft.png",
              cdn_uri: "https://cdn.example.com/nft.png",
              mime: "image/png",
            },
          ],
          metadata: {
            attributes: [
              { value: "Zombie", trait_type: "Type" },
              { value: "None", trait_type: "Eyewear" },
            ],
            description: "Mock NFT description",
            name: "#5477",
            symbol: "UNDEAD",
          },
          links: {
            external_url: "https://www.example.com/",
            image: "https://example.com/nft.png",
          },
        },
        authorities: [
          {
            address: "und8WV2QGXCaKDNnPvJbW4oixCyqhEQeje8J7xiY1uT",
            scopes: ["full"],
          },
        ],
        compression: {
          eligible: false,
          compressed: false,
          data_hash: "",
          creator_hash: "",
          asset_hash: "",
          tree: "",
          seq: 0,
          leaf_id: 0,
        },
        grouping: [
          {
            group_key: "collection",
            group_value: "undCCwvNJueKKgeYNJfqPSLqBwax4wMPPF6vNAJFwrb",
          },
        ],
        royalty: {
          royalty_model: "creators",
          target: null,
          percent: 0.05,
          basis_points: 500,
          primary_sale_happened: true,
          locked: false,
        },
        creators: [
          {
            address: "72ZGzDVoxLWnk89Pg3jPm9Nohd6FHrMMAKiazCp2jjgA",
            share: 0,
            verified: true,
          },
          {
            address: "D2GnBfYjme59GqY2mrTivw3atB2VVwACrXit5zvL2NK6",
            share: 100,
            verified: false,
          },
        ],
        ownership: {
          frozen: true,
          delegated: true,
          delegate: "4Gv5bLhtiCHoN7XJrR1cFqzqvi5BQyw8fa4zZJFvHjoz",
          ownership_model: "single",
          owner: ownerAddress,
        },
        supply: {
          print_max_supply: 0,
          print_current_supply: 0,
          edition_nonce: 254,
        },
        mutable: true,
        burnt: false,
        token_info: {
          supply: 1,
          decimals: 0,
          token_program: "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
          mint_authority: "5NnKHu1aYxakxxdsmgx4KTaqoZnzeKPUNSNNa35FRf8J",
          freeze_authority: "5NnKHu1aYxakxxdsmgx4KTaqoZnzeKPUNSNNa35FRf8J",
        },
      },
      id: req.body.id || "test",
    };

    return res.json(mockResponse);
  }

  if (method === "getAssetsByOwner") {
    return res.json(mockData);
  }

  res.status(400).json({
    jsonrpc: "2.0",
    error: {
      code: -32601,
      message: "Method not found.",
    },
    id: req.body.id || null,
  });
});

app.listen(PORT, () => {
  console.log(`Mock helius server running at http://localhost:${PORT}`);
});
