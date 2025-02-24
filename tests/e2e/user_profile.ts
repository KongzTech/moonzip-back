import { withErrorHandling } from "../utils/helpers";
import createClient from "openapi-fetch";
import { paths } from "../../clients/backend_client";
import { Keypair } from "@solana/web3.js";
import { expect } from "chai";

const apiHost = process.env.MOONZIP_API_HOST || "http://localhost:8080";
const client = createClient<paths>({ baseUrl: apiHost });

interface ErrorResponse {
  message: string;
  code: number;
}

export enum UserInfoApiErrorCode {
  InvalidParameter = 3,
  EmptyUsername = 10,
  ExistedUsername = 12, // "username existed"
  NotFoundUser = 13, // "Not found user"
  InvalidUsernameFormat = 14, // "Invalid username format"
}

export enum ExpectedResult {
  UserUpserted,
  UserFound,
  UserNotFound,
  ClientError,
  ServerError,
  InvalidParameter,
  InvalidUsernameFormat,
  ExistedUsername,
  RetrievedPageData,
}

async function createUserInformation(
  walletAddress: string,
  username: string,
  nftAddress: string
): Promise<ExpectedResult> {
  let create = await withErrorHandling(
    client.POST("/api/user/upsert", {
      params: {
        query: {},
      },
      body: {
        walletAddress: walletAddress,
        username: username,
        displayValue: username,
        nftAddress: nftAddress,
      },
    })
  );
  if (create.response.status == 200 && create.data) {
    console.log("Upsert successfully");
    console.log(create.data);
    return ExpectedResult.UserUpserted;
  }
  if (create.response.status >= 400 && create.response.status < 500) {
    const errorData: ErrorResponse = create.error;
    if (errorData.code === UserInfoApiErrorCode.InvalidUsernameFormat) {
      console.log("Invalid username format");
      return ExpectedResult.InvalidUsernameFormat;
    }
    if (errorData.code === UserInfoApiErrorCode.EmptyUsername) {
      console.log("Username is empty");
      return ExpectedResult.InvalidUsernameFormat;
    }
    if (errorData.code === UserInfoApiErrorCode.ExistedUsername) {
      console.log("Username is existed");
      return ExpectedResult.ExistedUsername;
    }
    if (errorData.code === UserInfoApiErrorCode.InvalidParameter) {
      console.log("Invalid Parameter");
      return ExpectedResult.InvalidParameter;
    }
    return ExpectedResult.ServerError;
  }
  if (create.response.status == 500) {
    return ExpectedResult.ServerError;
  }
}

async function getUserInformation(
  walletAddress: string
): Promise<ExpectedResult> {
  const getResponse = await client.GET("/api/user/get", {
    params: {
      query: {
        walletAddress: walletAddress,
      },
    },
  });
  if (getResponse && getResponse.data) {
    console.log(getResponse.data);
  }
  if (getResponse.response.status >= 400 && getResponse.response.status < 500) {
    const errorData: ErrorResponse = getResponse.error;
    if (errorData.code === UserInfoApiErrorCode.NotFoundUser) {
      return ExpectedResult.UserNotFound;
    }
  }
  if (getResponse.error) {
    return ExpectedResult.ServerError;
  }
  return ExpectedResult.UserFound;
}

async function fetchPageNFTs(
  walletAddress: string,
  page: number,
  limit: number
): Promise<ExpectedResult> {
  console.log(`Fetching page ${page}...`);
  const response = await client.GET("/api/user/owned-nfts", {
    params: {
      query: {
        ownerAddress: walletAddress,
        page,
        limit,
      },
    },
  });

  if (response.error) {
    const errorData: ErrorResponse = response.error;
    console.log(errorData);
    if (errorData.code === 3 && page === 0) {
      console.log("Validated page 0");
      return ExpectedResult.RetrievedPageData;
    } else {
      return ExpectedResult.ServerError;
    }
  }

  if (!response.data || !response.data.result) {
    return ExpectedResult.ServerError;
  }
  return ExpectedResult.RetrievedPageData;
}

async function fetchUserOwnedNFTs(
  walletAddress: string,
  totalPages: number,
  limit: number
): Promise<ExpectedResult> {
  for (let page = 0; page <= totalPages; page++) {
    let result = await fetchPageNFTs(walletAddress, page, limit);
    expect(result).to.equal(ExpectedResult.RetrievedPageData);
  }
  console.log("All pages fetched successfully.");
  return ExpectedResult.RetrievedPageData;
}

describe("User info tests", () => {
  it("Should create, update, and get user information successfully", async () => {
    const keypair1 = Keypair.generate();
    const walletAddress1 = keypair1.publicKey.toBase58();
    const walletAddressAppend = walletAddress1 + "-updated";
    const nftAddress1 = "8sEzBWHU6JuPmpkSUoT7tbiCKr6hy2pXgTBwFryJJQQa";
    const nftAddress2 = "JwTBZc915F6pqQ754YR5ot5BG76Sfy7CHQTp5BGhkWs";

    const createResult1 = await createUserInformation(
      walletAddress1,
      walletAddress1,
      nftAddress1
    );
    expect(createResult1).to.equal(ExpectedResult.UserUpserted);
    const getResult1 = await getUserInformation(walletAddress1);
    expect(getResult1).to.equal(ExpectedResult.UserFound);

    const updateResult1 = await createUserInformation(
      walletAddress1,
      walletAddressAppend,
      nftAddress2
    );
    expect(updateResult1).to.equal(ExpectedResult.UserUpserted);
    const getResult2 = await getUserInformation(walletAddress1);
    expect(getResult2).to.equal(ExpectedResult.UserFound);

    const createResult2 = await createUserInformation(
      walletAddress1,
      walletAddress1,
      undefined
    );
    expect(createResult2).to.equal(ExpectedResult.UserUpserted);
    const getResult3 = await getUserInformation(walletAddress1);
    expect(getResult3).to.equal(ExpectedResult.UserFound);

    const updateResult2 = await createUserInformation(
      walletAddress1,
      walletAddressAppend,
      undefined
    );
    expect(updateResult2).to.equal(ExpectedResult.UserUpserted);
    const getResult4 = await getUserInformation(walletAddress1);
    expect(getResult4).to.equal(ExpectedResult.UserFound);
  });

  it("Try to create or update invalid username (contain bad words or empty)", async () => {
    const keypair = Keypair.generate();
    const walletAddress = keypair.publicKey.toBase58();

    const createResult = await createUserInformation(
      walletAddress,
      "fu*k",
      undefined
    );
    expect(createResult).to.equal(ExpectedResult.InvalidUsernameFormat);

    const createResult2 = await createUserInformation(
      walletAddress,
      "d*ck",
      undefined
    );
    expect(createResult2).to.equal(ExpectedResult.InvalidUsernameFormat);

    const keypair2 = Keypair.generate();
    const walletAddress2 = keypair2.publicKey.toBase58();

    const createResult3 = await createUserInformation(
      walletAddress2,
      walletAddress2,
      undefined
    );
    expect(createResult3).to.equal(ExpectedResult.UserUpserted);

    const updatedResult3 = await createUserInformation(
      walletAddress2,
      "nig*a",
      undefined
    );
    expect(updatedResult3).to.equal(ExpectedResult.InvalidUsernameFormat);

    const updatedResult4 = await createUserInformation(
      walletAddress2,
      "",
      undefined
    );
    expect(updatedResult4).to.equal(ExpectedResult.InvalidParameter);
  });

  it("Try to create existing username", async () => {
    const keypair = Keypair.generate();
    const walletAddress = keypair.publicKey.toBase58();
    const createResult = await createUserInformation(
      walletAddress,
      walletAddress,
      undefined
    );
    expect(createResult).to.equal(ExpectedResult.UserUpserted);

    const keypair2 = Keypair.generate();
    const walletAddress2 = keypair2.publicKey.toBase58();
    const createResult2 = await createUserInformation(
      walletAddress2,
      walletAddress,
      undefined
    );
    expect(createResult2).to.equal(ExpectedResult.ExistedUsername);
  });

  it("Try to get not exist user", async () => {
    const keypair4 = Keypair.generate();
    const walletAddress4 = keypair4.publicKey.toBase58();
    let createResult = await getUserInformation(walletAddress4);
    expect(createResult).to.equal(ExpectedResult.UserNotFound);
  });
});

describe("GET /api/user/owned-nfts", () => {
  const keypair = Keypair.generate();
  const walletAddress = keypair.publicKey.toBase58();
  const totalPages = 120;
  const limit = 10;

  it("should return NFTs for each page in a loop", async () => {
    const result = await fetchUserOwnedNFTs(walletAddress, totalPages, limit);
    expect(result).to.equal(ExpectedResult.RetrievedPageData);
  });
});
