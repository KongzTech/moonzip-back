use base64::{
    alphabet::STANDARD,
    engine::{general_purpose::NO_PAD, GeneralPurpose},
    Engine,
};
use delegate::delegate;
use serde::Serialize;
use solana_client::rpc_client::SerializableTransaction;
use solana_sdk::{
    address_lookup_table::AddressLookupTableAccount,
    hash::Hash,
    instruction::Instruction,
    message::{v0, VersionedMessage},
    signature::{Keypair, Signature},
    signer::Signer,
    signers::Signers,
    transaction::{Transaction, VersionedTransaction},
};

#[derive(Debug, Clone, derive_more::From)]
pub enum AnyTxPrepare {
    Legacy(LegacyTxPrepare),
    Versioned(VersionedTxPrepare),
}

impl AnyTxPrepare {
    pub fn ixs_mut(&mut self) -> &mut Vec<Instruction> {
        match self {
            AnyTxPrepare::Legacy(legacy_tx_result) => legacy_tx_result.instructions.as_mut(),
            AnyTxPrepare::Versioned(versioned_tx_result) => {
                versioned_tx_result.instructions.as_mut()
            }
        }
    }

    pub fn as_blank(&self, payer: &Keypair) -> anyhow::Result<AnyTx> {
        match self {
            AnyTxPrepare::Legacy(tx_result) => {
                let tx =
                    Transaction::new_with_payer(&tx_result.instructions, Some(&payer.pubkey()));
                Ok(AnyTx::Legacy(tx))
            }
            AnyTxPrepare::Versioned(versioned_tx_result) => {
                let message = v0::Message::try_compile(
                    &payer.pubkey(),
                    &versioned_tx_result.instructions,
                    &versioned_tx_result.alt_accounts,
                    Default::default(),
                )?;
                Ok(VersionedTransaction::try_new(VersionedMessage::V0(message), &[payer])?.into())
            }
        }
    }

    pub fn sign(
        self,
        keypairs: &impl Signers,
        payer: &impl Signer,
        blockhash: Hash,
    ) -> anyhow::Result<AnyTx> {
        match self {
            AnyTxPrepare::Legacy(tx_result) => {
                let mut tx =
                    Transaction::new_with_payer(&tx_result.instructions, Some(&payer.pubkey()));
                tx.sign(keypairs, blockhash);
                Ok(AnyTx::Legacy(tx))
            }
            AnyTxPrepare::Versioned(versioned_tx_result) => {
                let message = v0::Message::try_compile(
                    &payer.pubkey(),
                    &versioned_tx_result.instructions,
                    &versioned_tx_result.alt_accounts,
                    blockhash,
                )?;
                Ok(VersionedTransaction::try_new(VersionedMessage::V0(message), keypairs)?.into())
            }
        }
    }
}
#[derive(Debug, Clone, derive_more::From, Serialize)]
#[serde(untagged)]
pub enum AnyTx {
    Legacy(Transaction),
    Versioned(VersionedTransaction),
}

const BASE64_ENGINE: GeneralPurpose = GeneralPurpose::new(&STANDARD, NO_PAD);

impl AnyTx {
    pub fn serialize_base64(&self) -> anyhow::Result<String> {
        let bincoded = match self {
            AnyTx::Legacy(tx) => bincode::serialize(tx)?,
            AnyTx::Versioned(tx) => bincode::serialize(tx)?,
        };

        Ok(BASE64_ENGINE.encode(bincoded))
    }
}

#[derive(Debug, Clone)]
pub struct LegacyTxPrepare {
    pub instructions: Vec<Instruction>,
}

#[derive(Debug, Clone)]
pub struct VersionedTxPrepare {
    pub alt_accounts: Vec<AddressLookupTableAccount>,
    pub instructions: Vec<Instruction>,
}

impl SerializableTransaction for AnyTx {
    delegate! {
        to match &self {
            AnyTx::Legacy(tx) => tx,
            AnyTx::Versioned(tx) => tx,
        } {
            fn get_signature(&self) -> &Signature;
            fn get_recent_blockhash(&self) -> &Hash;
            fn uses_durable_nonce(&self) -> bool;
        }
    }
}
