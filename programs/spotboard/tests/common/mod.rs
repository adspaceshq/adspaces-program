use anchor_lang::{
    solana_program::{
        clock::Clock,
        instruction::{AccountMeta, Instruction},
        program_pack::Pack,
        system_instruction,
    },
    AccountDeserialize,
};
#[allow(deprecated)]
use spl_token::state;

pub use anchor_lang::prelude::Pubkey;
pub use anchor_lang::solana_program::program_pack::Pack;
use litesvm::LiteSVM;
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

/// The devnet test-USDC address doubles as the test mint. The program only
/// accepts the two known USDC mints, so tests plant a mint there.
pub fn usdc_mint_address() -> Pubkey {
    spotboard::constants::USDC_MINT_DEVNET.parse().unwrap()
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

pub struct Env {
    pub svm: LiteSVM,
    pub mint_authority: Keypair,
    pub usdc: Pubkey,
}

impl Env {
    pub fn new() -> Self {
        let mut svm = LiteSVM::new().with_default_programs();
        svm.add_program(program_id(), PROGRAM_BYTES).unwrap();

        let mint_authority = Keypair::new();
        svm.airdrop(&mint_authority.pubkey(), 10_000_000_000).unwrap();

        let usdc = usdc_mint_address();
        let create_mint = system_instruction::create_account(
            &mint_authority.pubkey(),
            &usdc,
            RENT_FUNDING,
            state::Mint::LEN as u64,
            &token_program_id(),
        );
        send(&mut svm, &[create_mint], &mint_authority, &[]).unwrap();

        let init_mint = spl_token::instruction::initialize_mint(
            &token_program_id(),
            &usdc,
            &mint_authority.pubkey(),
            None,
            6,
        )
        .unwrap();
        send(&mut svm, &[init_mint], &mint_authority, &[]).unwrap();

        Self {
            svm,
            mint_authority,
            usdc,
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

pub fn token_balance(svm: &LiteSVM, account: &Pubkey) -> u64 {
    let raw = svm.get_account(account).unwrap();
    state::Account::unpack(&raw.data).unwrap().amount
}
