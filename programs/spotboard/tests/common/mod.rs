#![allow(dead_code)]

use anchor_lang::{
    solana_program::{
        clock::Clock,
        instruction::{AccountMeta, Instruction},
        system_instruction, system_program,
    },
    AccountDeserialize, InstructionData, ToAccountMetas,
};
use spl_token_interface::state;

pub use anchor_lang::prelude::Pubkey;
pub use anchor_lang::solana_program::program_pack::Pack;
use litesvm::LiteSVM;
use solana_account::Account;
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;

pub const PROGRAM_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/deploy/spotboard.so"
));

/// Lamports funded into every mint / token account. Far above rent-exemption.
pub const RENT_FUNDING: u64 = 100_000_000;

pub fn program_id() -> Pubkey {
    spotboard::id()
}

pub fn token_program_id() -> Pubkey {
    spl_token::ID
}

pub fn config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[spotboard::constants::CONFIG_SEED], &program_id())
}

pub fn listing_pda(seller: &Pubkey, slug_hash: &[u8; 16]) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            spotboard::constants::LISTING_SEED,
            seller.as_ref(),
            slug_hash.as_ref(),
        ],
        &program_id(),
    )
}

pub fn spot_pda(listing: &Pubkey, index: u8) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[spotboard::constants::SPOT_SEED, listing.as_ref(), &[index]],
        &program_id(),
    )
}

pub fn vault_pda(spot: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[spotboard::constants::VAULT_SEED, spot.as_ref()],
        &program_id(),
    )
}

pub fn proof_pda(spot: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[spotboard::constants::PROOF_SEED, spot.as_ref()],
        &program_id(),
    )
}

pub fn dispute_pda(spot: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[spotboard::constants::DISPUTE_SEED, spot.as_ref()],
        &program_id(),
    )
}

pub struct Env {
    pub svm: LiteSVM,
    pub mint_authority: Keypair,
    pub usdc: Pubkey,
    pub config: Pubkey,
    pub treasury: Pubkey,
}

impl Env {
    pub fn new() -> Self {
        let mut svm = LiteSVM::new().with_default_programs();
        svm.add_program(program_id(), PROGRAM_BYTES).unwrap();

        let mint_authority = Keypair::new();
        svm.airdrop(&mint_authority.pubkey(), 10_000_000_000)
            .unwrap();

        let usdc = Pubkey::new_unique();
        let mint = state::Mint {
            mint_authority: spl_token::solana_program::program_option::COption::Some(
                mint_authority.pubkey(),
            ),
            supply: 0,
            decimals: 6,
            is_initialized: true,
            freeze_authority: spl_token::solana_program::program_option::COption::None,
        };
        let mut mint_data = vec![0; state::Mint::LEN];
        state::Mint::pack(mint, &mut mint_data).unwrap();
        svm.set_account(
            usdc,
            Account {
                lamports: RENT_FUNDING,
                data: mint_data,
                owner: token_program_id(),
                executable: false,
                rent_epoch: 0,
            },
        )
        .unwrap();

        let (config, _) = config_pda();
        let treasury = Pubkey::new_unique();
        let initialize_config = new_ix(
            program_id(),
            spotboard::instruction::InitializeConfig {
                treasury,
                fee_bps: spotboard::constants::PLATFORM_FEE_BPS,
            }
            .data(),
            spotboard::accounts::InitializeConfig {
                authority: mint_authority.pubkey(),
                config,
                payment_mint: usdc,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        );
        send(&mut svm, &[initialize_config], &mint_authority, &[]).unwrap();

        Self {
            svm,
            mint_authority,
            usdc,
            config,
            treasury,
        }
    }

    /// Fresh funded keypair with an empty USDC token account.
    pub fn new_user(&mut self, usdc_amount: u64) -> (Keypair, Pubkey) {
        let user = Keypair::new();
        self.svm.airdrop(&user.pubkey(), 10_000_000_000).unwrap();

        let token_account = Keypair::new();
        let create = system_instruction::create_account(
            &user.pubkey(),
            &token_account.pubkey(),
            RENT_FUNDING,
            state::Account::LEN as u64,
            &token_program_id(),
        );
        send(&mut self.svm, &[create], &user, &[&token_account]).unwrap();

        let init = spl_token::instruction::initialize_account(
            &token_program_id(),
            &token_account.pubkey(),
            &self.usdc,
            &user.pubkey(),
        )
        .unwrap();
        send(&mut self.svm, &[init], &user, &[]).unwrap();

        if usdc_amount > 0 {
            let mint_to = spl_token::instruction::mint_to(
                &token_program_id(),
                &self.usdc,
                &token_account.pubkey(),
                &self.mint_authority.pubkey(),
                &[],
                usdc_amount,
            )
            .unwrap();
            send(&mut self.svm, &[mint_to], &self.mint_authority, &[]).unwrap();
        }

        (user, token_account.pubkey())
    }

    pub fn clock(&mut self) -> Clock {
        self.svm.get_sysvar::<Clock>()
    }

    pub fn set_clock(&mut self, clock: &Clock) {
        self.svm.set_sysvar::<Clock>(clock);
    }

    pub fn warp_to_timestamp(&mut self, timestamp: i64) {
        let mut clock = self.clock();
        clock.unix_timestamp = timestamp;
        self.set_clock(&clock);
    }
}

pub fn new_ix(program_id: Pubkey, data: Vec<u8>, accounts: Vec<AccountMeta>) -> Instruction {
    Instruction::new_with_bytes(program_id, &data, accounts)
}

/// Builds, signs and sends one transaction with the latest blockhash.
#[allow(clippy::result_large_err)]
pub fn send(
    svm: &mut LiteSVM,
    ixs: &[Instruction],
    payer: &Keypair,
    extra_signers: &[&Keypair],
) -> litesvm::types::TransactionResult {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &blockhash);
    let mut signers = vec![payer];
    signers.extend(extra_signers.iter());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &signers).unwrap();
    svm.send_transaction(tx)
}

pub fn read_listing(svm: &LiteSVM, listing: &Pubkey) -> spotboard::state::Listing {
    let account = svm.get_account(listing).unwrap();
    let mut data: &[u8] = &account.data;
    spotboard::state::Listing::try_deserialize(&mut data).unwrap()
}

pub fn read_spot(svm: &LiteSVM, spot: &Pubkey) -> spotboard::state::Spot {
    let account = svm.get_account(spot).unwrap();
    let mut data: &[u8] = &account.data;
    spotboard::state::Spot::try_deserialize(&mut data).unwrap()
}

pub fn read_proof(svm: &LiteSVM, proof: &Pubkey) -> spotboard::state::ProofRecord {
    let account = svm.get_account(proof).unwrap();
    let mut data: &[u8] = &account.data;
    spotboard::state::ProofRecord::try_deserialize(&mut data).unwrap()
}

pub fn read_dispute(svm: &LiteSVM, dispute: &Pubkey) -> spotboard::state::DisputeRecord {
    let account = svm.get_account(dispute).unwrap();
    let mut data: &[u8] = &account.data;
    spotboard::state::DisputeRecord::try_deserialize(&mut data).unwrap()
}

pub fn read_config(svm: &LiteSVM, config: &Pubkey) -> spotboard::state::Config {
    let account = svm.get_account(config).unwrap();
    let mut data: &[u8] = &account.data;
    spotboard::state::Config::try_deserialize(&mut data).unwrap()
}

pub fn token_balance(svm: &LiteSVM, account: &Pubkey) -> u64 {
    let raw = svm.get_account(account).unwrap();
    state::Account::unpack(&raw.data).unwrap().amount
}
