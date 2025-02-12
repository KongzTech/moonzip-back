use solana_sdk::pubkey::Pubkey;

const EVENT_AUTHORTIY_PREFIX: &[u8] = b"__event_authority";

pub fn anchor_event_authority(program_id: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[EVENT_AUTHORTIY_PREFIX], program_id).0
}
