import * as anchor from "@coral-xyz/anchor";
import { BN } from "@coral-xyz/anchor";
import { components, paths } from "../../clients/backend_client";

import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  Transaction,
} from "@solana/web3.js";
import { expect } from "chai";
import {
  airdrop,
  beforeAll,
  devLockEscrow,
  expectNoATA,
  getProvider,
  tokenBalance,
} from "../utils/utils";
import createClient from "openapi-fetch";
import {
  currentTS,
  delay,
  sendTransaction,
  time,
  waitForOk,
  waitOnClusterTime,
  withErrorHandling,
  withTimeout,
} from "../utils/helpers";
import { getAssociatedTokenAddressSync } from "@solana/spl-token";

const imagePath = "./tests/data/moon.png";
const apiHost = process.env.MOONZIP_API_HOST || "http://app-api:8080";
const client = createClient<paths>({ baseUrl: apiHost });

function onCurveStatus(
  curveVariant: components["schemas"]["CurveVariant"]
): components["schemas"]["PublicProjectStage"] {
  if (curveVariant == "pumpfun") {
    return "graduated";
  } else if (curveVariant == "moonzip") {
    return "curvePoolActive";
  }
}

function sampleCreateProjectMeta(
  owner: Keypair,
  deploySchema: components["schemas"]["DeploySchema"]
) {
  const meta: components["schemas"]["CreateTokenMeta"] = {
    symbol: "TEST",
    description: "some small token test description",
    name: "TEST",
  };

  const request_meta: components["schemas"]["CreateProjectRequest"] = {
    deploySchema,
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

async function waitForProject(
  projectId: string,
  stage?: components["schemas"]["PublicProjectStage"]
) {
  console.log(`started waiting for project at ${new Date().toUTCString()}`);
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
  throw new Error(
    `project is not ready in 30 seconds at: ${new Date().toUTCString()}`
  );
}

async function testBuySellStaticPool(
  curve: components["schemas"]["CurveVariant"]
) {
  let _provider = getProvider();
  let connection = _provider.connection;

  const schema: components["schemas"]["DeploySchema"] = {
    curvePool: curve,
    staticPool: {
      launchPeriod: 10,
    },
    devPurchase: {
      lock: { type: "disabled" },
      value: LAMPORTS_PER_SOL,
    },
  };

  const owner = anchor.web3.Keypair.generate();
  const user = anchor.web3.Keypair.generate();

  console.log("for owner", owner.publicKey.toBase58());
  await airdrop(owner.publicKey, new BN(LAMPORTS_PER_SOL * 1.2));
  await airdrop(user.publicKey, new BN(LAMPORTS_PER_SOL));
  let projectMeta = sampleCreateProjectMeta(owner, schema);
  let projectResult = await createProject(owner, projectMeta);

  console.log("Sent transaction for project creation, waiting for status");

  let project = await waitForProject(
    projectResult.projectId,
    "staticPoolActive"
  );

  console.log("Static pool is active, begin to buy");

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
}

async function testBuySellCurvePool(
  curve: components["schemas"]["CurveVariant"]
) {
  let _provider = getProvider();
  let connection = _provider.connection;

  const owner = anchor.web3.Keypair.generate();
  const user = anchor.web3.Keypair.generate();
  await airdrop(owner.publicKey, new BN(LAMPORTS_PER_SOL * 1.2));
  await airdrop(user.publicKey, new BN(LAMPORTS_PER_SOL));

  const schema: components["schemas"]["DeploySchema"] = {
    curvePool: curve,
    staticPool: {
      launchPeriod: 10,
    },
    devPurchase: {
      lock: { type: "disabled" },
      value: LAMPORTS_PER_SOL,
    },
  };

  let projectMeta = sampleCreateProjectMeta(owner, schema);
  let projectResult = await createProject(owner, projectMeta);
  let project = await waitForProject(
    projectResult.projectId,
    onCurveStatus(curve)
  );

  let curvePoolMint = new PublicKey(project.curvePoolMint!);

  console.log("project is created, verifying creator's balance");
  let balance = await waitForOk(
    async () => await tokenBalance(curvePoolMint, owner.publicKey)
  );
  expect(balance).to.be.gt(100_000_000);

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
}

async function claimDevLock(owner: Keypair, projectId: string) {
  let _provider = getProvider();
  let connection = _provider.connection;

  let result = await withErrorHandling(
    withTimeout(
      2000,
      client.POST("/api/project/claim_dev_lock", {
        body: {
          projectId: projectId,
        },
      })
    )
  );
  if (result.error) {
    throw new Error(JSON.stringify(result.error));
  }
  let transaction = Transaction.from(
    Buffer.from(result.data.transaction, "base64")
  );
  transaction.recentBlockhash = (
    await connection.getLatestBlockhash()
  ).blockhash;
  transaction.partialSign(owner);
  transaction.verifySignatures(true);
  await sendTransaction(getProvider().connection, transaction);
  return result;
}

async function testDevLockWorks(curve: components["schemas"]["CurveVariant"]) {
  let _provider = getProvider();
  let connection = _provider.connection;

  const owner = anchor.web3.Keypair.generate();
  await airdrop(owner.publicKey, new BN(LAMPORTS_PER_SOL * 1.2));

  const schema: components["schemas"]["DeploySchema"] = {
    curvePool: curve,
    staticPool: {
      launchPeriod: 10,
    },
    devPurchase: {
      lock: { type: "interval", interval: 10 },
      value: LAMPORTS_PER_SOL,
    },
  };
  let projectMeta = sampleCreateProjectMeta(owner, schema);
  let projectResult = await createProject(owner, projectMeta);
  let project = await waitForProject(
    projectResult.projectId,
    onCurveStatus(curve)
  );

  let curvePoolMint = new PublicKey(project.curvePoolMint!);
  await expectNoATA(curvePoolMint, owner.publicKey);
  let escrow = devLockEscrow(new PublicKey(project.devLockBase));
  let escrowATA = getAssociatedTokenAddressSync(curvePoolMint, escrow, true);
  console.log(`would check escrow token balance: ${escrow} ${escrowATA}`);

  let escrowBalance = await waitForOk(
    async () => await tokenBalance(curvePoolMint, escrow)
  );
  expect(escrowBalance).to.gt(100_000_000);
  await claimDevLock(owner, project.id);
  let balanceBeforeUnlock = await tokenBalance(curvePoolMint, owner.publicKey);
  expect(balanceBeforeUnlock).to.eq(0);

  await waitOnClusterTime(connection, currentTS() + 10);
  await claimDevLock(owner, project.id);

  let ownerBalance = await waitForOk(
    async () => await tokenBalance(curvePoolMint, owner.publicKey)
  );
  expect(ownerBalance).to.eq(escrowBalance);
}

async function testGraduateRaydium() {
  let _provider = getProvider();
  let connection = _provider.connection;

  const owner = anchor.web3.Keypair.generate();
  await airdrop(owner.publicKey, new BN(LAMPORTS_PER_SOL * 1.2));

  const schema: components["schemas"]["DeploySchema"] = {
    curvePool: "moonzip",
    staticPool: {
      launchPeriod: 10,
    },
    devPurchase: {
      lock: { type: "disabled" },
      value: LAMPORTS_PER_SOL,
    },
  };
  let projectMeta = sampleCreateProjectMeta(owner, schema);
  let projectResult = await createProject(owner, projectMeta);
  let project = await waitForProject(
    projectResult.projectId,
    onCurveStatus("moonzip")
  );
  for (let i = 0; i < 10; i++) {
    let buyer = Keypair.generate();
    let sols = new BN(LAMPORTS_PER_SOL).mul(new BN(8));
    await airdrop(buyer.publicKey, sols.add(new BN(0.2 * LAMPORTS_PER_SOL)));
    await buyFromProject(project.id, buyer, sols, 0);
  }

  project = await waitForProject(projectResult.projectId, "graduated");
}

describe("projects operations", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  before(beforeAll);

  it("moonzip: create buy sell from static pool project", async () => {
    await testBuySellStaticPool("moonzip");
  });

  it("pumpfun: create buy sell from static pool project", async () => {
    await testBuySellStaticPool("pumpfun");
  });

  it("moonzip: create and wait for static pool graduate, buy from curve pool", async () => {
    await testBuySellCurvePool("moonzip");
  });

  it("pumpfun: create and wait for static pool graduate, buy from curve pool", async () => {
    await testBuySellCurvePool("pumpfun");
  });

  it("moonzip: create, wait for curve pool, ensure dev lock works", async () => {
    await testDevLockWorks("moonzip");
  });

  it("pumpfun: create, wait for curve pool, ensure dev lock works", async () => {
    await testDevLockWorks("pumpfun");
  });

  // TODO: finish with raydium
  // it("buy to graduate to raydium, ensure correct graduation", async () => {
  //   await testGraduateRaydium();
  // });
});
