/**
 * Program IDL in camelCase format in order to be used in JS/TS.
 *
 * Note that this is only a type helper and is not the actual IDL. The original
 * IDL can be found at `target/idl/pump.json`.
 */
export type Pump = {
  address: "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
  metadata: {
    name: "pump";
    version: "0.1.0";
    spec: "0.1.0";
  };
  instructions: [
    {
      name: "initialize";
      discriminator: [175, 175, 109, 31, 13, 152, 155, 237];
      accounts: [
        {
          name: "global";
          writable: true;
        },
        {
          name: "user";
          writable: true;
          signer: true;
        },
        {
          name: "systemProgram";
        }
      ];
      args: [];
    },
    {
      name: "setParams";
      discriminator: [27, 234, 178, 52, 147, 2, 187, 141];
      accounts: [
        {
          name: "global";
          writable: true;
        },
        {
          name: "user";
          writable: true;
          signer: true;
        },
        {
          name: "systemProgram";
        },
        {
          name: "eventAuthority";
        },
        {
          name: "program";
        }
      ];
      args: [
        {
          name: "feeRecipient";
          type: "pubkey";
        },
        {
          name: "initialVirtualTokenReserves";
          type: "u64";
        },
        {
          name: "initialVirtualSolReserves";
          type: "u64";
        },
        {
          name: "initialRealTokenReserves";
          type: "u64";
        },
        {
          name: "tokenTotalSupply";
          type: "u64";
        },
        {
          name: "feeBasisPoints";
          type: "u64";
        }
      ];
    },
    {
      name: "create";
      discriminator: [24, 30, 200, 40, 5, 28, 7, 119];
      accounts: [
        {
          name: "mint";
          writable: true;
          signer: true;
        },
        {
          name: "mintAuthority";
        },
        {
          name: "bondingCurve";
          writable: true;
        },
        {
          name: "associatedBondingCurve";
          writable: true;
        },
        {
          name: "global";
        },
        {
          name: "mplTokenMetadata";
        },
        {
          name: "metadata";
          writable: true;
        },
        {
          name: "user";
        },
        {
          name: "systemProgram";
        },
        {
          name: "tokenProgram";
        },
        {
          name: "associatedTokenProgram";
        },
        {
          name: "rent";
        },
        {
          name: "eventAuthority";
        },
        {
          name: "program";
        }
      ];
      args: [
        {
          name: "name";
          type: "string";
        },
        {
          name: "symbol";
          type: "string";
        },
        {
          name: "uri";
          type: "string";
        }
      ];
    },
    {
      name: "buy";
      discriminator: [102, 6, 61, 18, 1, 218, 235, 234];
      accounts: [
        {
          name: "global";
        },
        {
          name: "feeRecipient";
          writable: true;
        },
        {
          name: "mint";
        },
        {
          name: "bondingCurve";
          writable: true;
        },
        {
          name: "associatedBondingCurve";
          writable: true;
        },
        {
          name: "associatedUser";
          writable: true;
        },
        {
          name: "user";
          writable: true;
          signer: true;
        },
        {
          name: "systemProgram";
        },
        {
          name: "tokenProgram";
        },
        {
          name: "rent";
        },
        {
          name: "eventAuthority";
        },
        {
          name: "program";
        }
      ];
      args: [
        {
          name: "amount";
          type: "u64";
        },
        {
          name: "maxSolCost";
          type: "u64";
        }
      ];
    },
    {
      name: "sell";
      discriminator: [51, 230, 133, 164, 1, 127, 131, 173];
      accounts: [
        {
          name: "global";
        },
        {
          name: "feeRecipient";
          writable: true;
        },
        {
          name: "mint";
        },
        {
          name: "bondingCurve";
          writable: true;
        },
        {
          name: "associatedBondingCurve";
          writable: true;
        },
        {
          name: "associatedUser";
          writable: true;
        },
        {
          name: "user";
          writable: true;
          signer: true;
        },
        {
          name: "systemProgram";
        },
        {
          name: "associatedTokenProgram";
        },
        {
          name: "tokenProgram";
        },
        {
          name: "eventAuthority";
        },
        {
          name: "program";
        }
      ];
      args: [
        {
          name: "amount";
          type: "u64";
        },
        {
          name: "minSolOutput";
          type: "u64";
        }
      ];
    },
    {
      name: "withdraw";
      discriminator: [183, 18, 70, 156, 148, 109, 161, 34];
      accounts: [
        {
          name: "global";
        },
        {
          name: "lastWithdraw";
          writable: true;
        },
        {
          name: "mint";
        },
        {
          name: "bondingCurve";
          writable: true;
        },
        {
          name: "associatedBondingCurve";
          writable: true;
        },
        {
          name: "associatedUser";
          writable: true;
        },
        {
          name: "user";
          writable: true;
          signer: true;
        },
        {
          name: "systemProgram";
        },
        {
          name: "tokenProgram";
        },
        {
          name: "rent";
        },
        {
          name: "eventAuthority";
        },
        {
          name: "program";
        }
      ];
      args: [];
    }
  ];
  accounts: [
    {
      name: "bondingCurve";
      discriminator: [23, 183, 248, 55, 96, 216, 172, 96];
    },
    {
      name: "global";
      discriminator: [167, 232, 232, 177, 200, 108, 114, 127];
    }
  ];
  events: [
    {
      name: "createEvent";
      discriminator: [27, 114, 169, 77, 222, 235, 99, 118];
    },
    {
      name: "tradeEvent";
      discriminator: [189, 219, 127, 211, 78, 230, 97, 238];
    },
    {
      name: "completeEvent";
      discriminator: [95, 114, 97, 156, 212, 46, 152, 8];
    },
    {
      name: "setParamsEvent";
      discriminator: [223, 195, 159, 246, 62, 48, 143, 131];
    }
  ];
  errors: [
    {
      code: 6000;
      name: "notAuthorized";
      msg: "The given account is not authorized to execute this instruction.";
    },
    {
      code: 6001;
      name: "alreadyInitialized";
      msg: "The program is already initialized.";
    },
    {
      code: 6002;
      name: "tooMuchSolRequired";
      msg: "slippage: Too much SOL required to buy the given amount of tokens.";
    },
    {
      code: 6003;
      name: "tooLittleSolReceived";
      msg: "slippage: Too little SOL received to sell the given amount of tokens.";
    },
    {
      code: 6004;
      name: "mintDoesNotMatchBondingCurve";
      msg: "The mint does not match the bonding curve.";
    },
    {
      code: 6005;
      name: "bondingCurveComplete";
      msg: "The bonding curve has completed and liquidity migrated to raydium.";
    },
    {
      code: 6006;
      name: "bondingCurveNotComplete";
      msg: "The bonding curve has not completed.";
    },
    {
      code: 6007;
      name: "notInitialized";
      msg: "The program is not initialized.";
    },
    {
      code: 6008;
      name: "withdrawTooFrequent";
      msg: "Withdraw too frequent";
    }
  ];
  types: [
    {
      name: "global";
      type: {
        kind: "struct";
        fields: [
          {
            name: "initialized";
            type: "bool";
          },
          {
            name: "authority";
            type: "pubkey";
          },
          {
            name: "feeRecipient";
            type: "pubkey";
          },
          {
            name: "initialVirtualTokenReserves";
            type: "u64";
          },
          {
            name: "initialVirtualSolReserves";
            type: "u64";
          },
          {
            name: "initialRealTokenReserves";
            type: "u64";
          },
          {
            name: "tokenTotalSupply";
            type: "u64";
          },
          {
            name: "feeBasisPoints";
            type: "u64";
          }
        ];
      };
    },
    {
      name: "lastWithdraw";
      type: {
        kind: "struct";
        fields: [
          {
            name: "lastWithdrawTimestamp";
            type: "i64";
          }
        ];
      };
    },
    {
      name: "bondingCurve";
      type: {
        kind: "struct";
        fields: [
          {
            name: "virtualTokenReserves";
            type: "u64";
          },
          {
            name: "virtualSolReserves";
            type: "u64";
          },
          {
            name: "realTokenReserves";
            type: "u64";
          },
          {
            name: "realSolReserves";
            type: "u64";
          },
          {
            name: "tokenTotalSupply";
            type: "u64";
          },
          {
            name: "complete";
            type: "bool";
          }
        ];
      };
    },
    {
      name: "createEvent";
      type: {
        kind: "struct";
        fields: [
          {
            name: "name";
            type: "string";
          },
          {
            name: "symbol";
            type: "string";
          },
          {
            name: "uri";
            type: "string";
          },
          {
            name: "mint";
            type: "pubkey";
          },
          {
            name: "bondingCurve";
            type: "pubkey";
          },
          {
            name: "user";
            type: "pubkey";
          }
        ];
      };
    },
    {
      name: "tradeEvent";
      type: {
        kind: "struct";
        fields: [
          {
            name: "mint";
            type: "pubkey";
          },
          {
            name: "solAmount";
            type: "u64";
          },
          {
            name: "tokenAmount";
            type: "u64";
          },
          {
            name: "isBuy";
            type: "bool";
          },
          {
            name: "user";
            type: "pubkey";
          },
          {
            name: "timestamp";
            type: "i64";
          },
          {
            name: "virtualSolReserves";
            type: "u64";
          },
          {
            name: "virtualTokenReserves";
            type: "u64";
          },
          {
            name: "realSolReserves";
            type: "u64";
          },
          {
            name: "realTokenReserves";
            type: "u64";
          }
        ];
      };
    },
    {
      name: "completeEvent";
      type: {
        kind: "struct";
        fields: [
          {
            name: "user";
            type: "pubkey";
          },
          {
            name: "mint";
            type: "pubkey";
          },
          {
            name: "bondingCurve";
            type: "pubkey";
          },
          {
            name: "timestamp";
            type: "i64";
          }
        ];
      };
    },
    {
      name: "setParamsEvent";
      type: {
        kind: "struct";
        fields: [
          {
            name: "feeRecipient";
            type: "pubkey";
          },
          {
            name: "initialVirtualTokenReserves";
            type: "u64";
          },
          {
            name: "initialVirtualSolReserves";
            type: "u64";
          },
          {
            name: "initialRealTokenReserves";
            type: "u64";
          },
          {
            name: "tokenTotalSupply";
            type: "u64";
          },
          {
            name: "feeBasisPoints";
            type: "u64";
          }
        ];
      };
    },
    {
      name: "bondingCurve";
      type: {
        kind: "struct";
        fields: [
          {
            name: "virtualTokenReserves";
            type: "u64";
          },
          {
            name: "virtualSolReserves";
            type: "u64";
          },
          {
            name: "realTokenReserves";
            type: "u64";
          },
          {
            name: "realSolReserves";
            type: "u64";
          },
          {
            name: "tokenTotalSupply";
            type: "u64";
          },
          {
            name: "complete";
            type: "bool";
          }
        ];
      };
    },
    {
      name: "global";
      type: {
        kind: "struct";
        fields: [
          {
            name: "initialized";
            type: "bool";
          },
          {
            name: "authority";
            type: "pubkey";
          },
          {
            name: "feeRecipient";
            type: "pubkey";
          },
          {
            name: "initialVirtualTokenReserves";
            type: "u64";
          },
          {
            name: "initialVirtualSolReserves";
            type: "u64";
          },
          {
            name: "initialRealTokenReserves";
            type: "u64";
          },
          {
            name: "tokenTotalSupply";
            type: "u64";
          },
          {
            name: "feeBasisPoints";
            type: "u64";
          }
        ];
      };
    },
    {
      name: "createEvent";
      type: {
        kind: "struct";
        fields: [
          {
            name: "name";
            type: "string";
          },
          {
            name: "symbol";
            type: "string";
          },
          {
            name: "uri";
            type: "string";
          },
          {
            name: "mint";
            type: "pubkey";
          },
          {
            name: "bondingCurve";
            type: "pubkey";
          },
          {
            name: "user";
            type: "pubkey";
          }
        ];
      };
    },
    {
      name: "tradeEvent";
      type: {
        kind: "struct";
        fields: [
          {
            name: "mint";
            type: "pubkey";
          },
          {
            name: "solAmount";
            type: "u64";
          },
          {
            name: "tokenAmount";
            type: "u64";
          },
          {
            name: "isBuy";
            type: "bool";
          },
          {
            name: "user";
            type: "pubkey";
          },
          {
            name: "timestamp";
            type: "i64";
          },
          {
            name: "virtualSolReserves";
            type: "u64";
          },
          {
            name: "virtualTokenReserves";
            type: "u64";
          },
          {
            name: "realSolReserves";
            type: "u64";
          },
          {
            name: "realTokenReserves";
            type: "u64";
          }
        ];
      };
    },
    {
      name: "completeEvent";
      type: {
        kind: "struct";
        fields: [
          {
            name: "user";
            type: "pubkey";
          },
          {
            name: "mint";
            type: "pubkey";
          },
          {
            name: "bondingCurve";
            type: "pubkey";
          },
          {
            name: "timestamp";
            type: "i64";
          }
        ];
      };
    },
    {
      name: "setParamsEvent";
      type: {
        kind: "struct";
        fields: [
          {
            name: "feeRecipient";
            type: "pubkey";
          },
          {
            name: "initialVirtualTokenReserves";
            type: "u64";
          },
          {
            name: "initialVirtualSolReserves";
            type: "u64";
          },
          {
            name: "initialRealTokenReserves";
            type: "u64";
          },
          {
            name: "tokenTotalSupply";
            type: "u64";
          },
          {
            name: "feeBasisPoints";
            type: "u64";
          }
        ];
      };
    }
  ];
};
