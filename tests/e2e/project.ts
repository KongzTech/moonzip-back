import { BN, Program } from "@coral-xyz/anchor";
import { components, paths } from "../../clients/backend_client";

import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  Transaction,
  VersionedTransaction,
} from "@solana/web3.js";
import { expect } from "chai";
import {
  airdrop,
  beforeAll,
  getProvider,
  provideGlobalConfig,
  tokenBalance,
} from "../utils/utils";
import * as anchor from "@coral-xyz/anchor";
import fs from "fs";
import createClient from "openapi-fetch";
import { bs58 } from "@coral-xyz/anchor/dist/cjs/utils/bytes";
import {
  delay,
  sendTransaction,
  withErrorHandling,
  withTimeout,
} from "../utils/helpers";
import { getMinimumBalanceForRentExemptAccount } from "@solana/spl-token";

const imagePath = "./tests/data/moon.png";
const apiHost = process.env.MOONZIP_API_HOST || "http://app-api:8080";
const client = createClient<paths>({ baseUrl: apiHost });

function sampleCreateProjectMeta(owner: Keypair, devPurchase: number) {
  const meta: components["schemas"]["CreateTokenMeta"] = {
    symbol: "TEST",
    description: "some small token test description",
    name: "TEST",
  };
  const schema: components["schemas"]["DeploySchema"] = {
    curvePool: "moonzip",
    staticPool: {
      launchPeriod: 10,
    },
    devPurchase: devPurchase,
  };

  const request_meta: components["schemas"]["CreateProjectRequest"] = {
    deploySchema: schema,
    meta: meta,
    owner: owner.publicKey.toBase58(),
  };
  return request_meta;
}

async function createProject(
  owner: Keypair,
  request_meta: components["schemas"]["CreateProjectRequest"]
) {
  const formData = new FormData();
  formData.append("project_request", JSON.stringify(request_meta));
  formData.append("image_content", new File([], imagePath));
  let result = await withErrorHandling(
    withTimeout(
      2000,
      client.POST("/api/project/create", {
        body: formData,
      })
    )
  );
  if (result.error) {
    throw new Error(JSON.stringify(result.error));
  }

  console.log("Received project id", result.data.projectId);
  let transaction = Transaction.from(
    Buffer.from(result.data.transaction, "base64")
  );
  transaction.partialSign(owner);
  transaction.verifySignatures(true);
  await sendTransaction(getProvider().connection, transaction);
  return result.data!;
}

type OperationMeta = {
  feeEstimate: number;
};

async function buyFromProject(
  projectId: string,
  user: Keypair,
  sols: number,
  minTokenOutput: number
): Promise<OperationMeta> {
  let buyResponse = await withErrorHandling(
    client.POST("/api/project/buy", {
      body: {
        projectId: projectId,
        user: user.publicKey.toBase58(),
        sols,
        minTokenOutput,
      },
    })
  );
  if (buyResponse.error) {
    throw new Error(JSON.stringify(buyResponse.error));
  }
  let transaction = Transaction.from(
    Buffer.from(buyResponse.data.transaction, "base64")
  );
  transaction.partialSign(user);
  transaction.verifySignatures(true);
  let fee = await transaction.getEstimatedFee(getProvider().connection);
  await sendTransaction(getProvider().connection, transaction);
  return {
    feeEstimate: fee,
  };
}

async function sellToProject(
  projectId: string,
  user: Keypair,
  tokens: number,
  minSolOutput: number
): Promise<OperationMeta> {
  let sellResponse = await withErrorHandling(
    client.POST("/api/project/sell", {
      body: {
        projectId: projectId,
        user: user.publicKey.toBase58(),
        tokens: tokens,
        minSolOutput: minSolOutput,
      },
    })
  );
  if (sellResponse.error) {
    throw new Error(JSON.stringify(sellResponse.error));
  }
  let transaction = Transaction.from(
    Buffer.from(sellResponse.data.transaction, "base64")
  );
  let fee = await transaction.getEstimatedFee(getProvider().connection);
  transaction.partialSign(user);
  transaction.verifySignatures(true);
  await sendTransaction(getProvider().connection, transaction);
  return {
    feeEstimate: fee,
  };
}

function time() {
  const nanos = process.hrtime.bigint();
  return Number(nanos / 1_000_000n);
}

async function waitForProject(
  projectId: string,
  stage?: components["schemas"]["PublicProjectStage"]
) {
  let start = time();
  while (time() - start < 30000) {
    let projectResponse = await client.GET("/api/project/get", {
      params: {
        query: {
          projectId: projectId,
        },
      },
    });
    if (projectResponse.error) {
      throw new Error(JSON.stringify(projectResponse.error));
    }
    if (projectResponse.data.project) {
      if (stage) {
        if (projectResponse.data.project.stage == stage) {
          return projectResponse.data.project;
        }
        console.log(
          `project is received, but stage mismatch, received: ${projectResponse.data.project.stage}, expected: ${stage}`
        );
      } else {
        return projectResponse.data.project;
      }
    }
    await delay(2000);
  }
  throw new Error("project is not ready in 30 seconds");
}

describe("projects operations", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  before(beforeAll);
  let _provider = getProvider();
  let connection = _provider.connection;

  it("create buy sell from static pool project", async () => {
    const owner = anchor.web3.Keypair.generate();
    const user = anchor.web3.Keypair.generate();

    console.log("for owner", owner.publicKey.toBase58());
    await airdrop(owner.publicKey, new BN(LAMPORTS_PER_SOL * 1.2));
    await airdrop(user.publicKey, new BN(LAMPORTS_PER_SOL));
    let projectMeta = sampleCreateProjectMeta(owner, LAMPORTS_PER_SOL);
    let projectResult = await createProject(owner, projectMeta);

    let project = await waitForProject(
      projectResult.projectId,
      "staticPoolActive"
    );

    expect(project.name).to.equal(projectMeta.meta.name);
    expect(project.description).to.equal(projectMeta.meta.description);
    expect(project.owner).to.equal(owner.publicKey.toBase58());
    expect(project.stage).to.equal("staticPoolActive");
    expect(project.staticPoolMint).to.not.be.null;

    let solToSpend = LAMPORTS_PER_SOL / 100;
    let minTokenOutput = 1_000_000;
    await buyFromProject(
      projectResult.projectId,
      user,
      solToSpend,
      minTokenOutput
    );

    let tokenBalanceAfterBuy = await tokenBalance(
      new PublicKey(project.staticPoolMint),
      user.publicKey
    );
    expect(tokenBalanceAfterBuy).to.be.gte(minTokenOutput);
    let solBalanceAfterBuy = await connection.getBalance(user.publicKey);

    let tokensToSell = Math.floor(minTokenOutput / 2);
    let sellMeta = await sellToProject(
      projectResult.projectId,
      user,
      tokensToSell,
      Math.floor(solToSpend / 3)
    );

    let tokenBalanceAfterSell = await tokenBalance(
      new PublicKey(project.staticPoolMint),
      user.publicKey
    );
    expect(tokenBalanceAfterSell).to.be.equal(
      tokenBalanceAfterBuy - tokensToSell
    );
    let solBalanceAfterSell = await connection.getBalance(user.publicKey);
    expect(solBalanceAfterSell).to.be.greaterThan(
      solBalanceAfterBuy - sellMeta.feeEstimate
    );
  });

  it("create and wait for static pool graduate, buy from curve pool", async () => {
    const owner = anchor.web3.Keypair.generate();
    const user = anchor.web3.Keypair.generate();
    await airdrop(owner.publicKey, new BN(LAMPORTS_PER_SOL * 1.2));
    await airdrop(user.publicKey, new BN(LAMPORTS_PER_SOL));
    let projectMeta = sampleCreateProjectMeta(owner, LAMPORTS_PER_SOL);
    let projectResult = await createProject(owner, projectMeta);
    let project = await waitForProject(
      projectResult.projectId,
      "curvePoolActive"
    );
    let curvePoolMint = new PublicKey(project.curvePoolMint!);
    let solsToSpend = Math.floor(LAMPORTS_PER_SOL / 3);
    let minTokenOutput = 1_000_000;
    console.log("buying from curve pool in sols", solsToSpend);
    await buyFromProject(
      projectResult.projectId,
      user,
      solsToSpend,
      minTokenOutput
    );
    let balanceAfterBuy = await connection.getBalance(user.publicKey);
    let tokensAfterBuy = await tokenBalance(curvePoolMint, user.publicKey);
    expect(tokensAfterBuy).to.be.gte(minTokenOutput);

    let tokensToSell = Math.floor(tokensAfterBuy / 2);
    let sellMeta = await sellToProject(
      projectResult.projectId,
      user,
      tokensToSell,
      0
    );
    let tokensAfterSell = await tokenBalance(curvePoolMint, user.publicKey);
    expect(tokensAfterSell).to.be.equal(tokensAfterBuy - tokensToSell);
    let solBalanceAfterSell = await connection.getBalance(user.publicKey);
    expect(solBalanceAfterSell).to.be.greaterThan(
      solBalanceAfterSell - sellMeta.feeEstimate
    );
    expect(solBalanceAfterSell).to.be.greaterThan(
      balanceAfterBuy - sellMeta.feeEstimate
    );
  });
});
