//! Randomized property test (audit A1.5).
//!
//! Runs many random instruction sequences against fixed-price and auction listings and checks,
//! after every step, whether or not the instruction succeeded:
//! - A1.1: an open spot's vault holds only donations; a funded or disputed spot's vault holds the
//!   escrowed amount (price for fixed, current_bid for auctions) plus donations; once a spot is
//!   finalized, refunded, cancelled or closed, both the spot and its vault are gone.
//! - USDC is conserved: participant accounts + treasury + live vaults always equal the mint supply,
//!   which never changes.
//! - A failed transaction moves no tokens; a successful one moves exactly the expected amounts
//!   (in particular an outbid bidder gets exactly their bid back).
//! - Rent is conserved: the listing's rent payer plus every listing, spot, vault, proof and
//!   platform-paid dispute account always hold the same lamports, so every closed account's rent
//!   went to the rent payer. A buyer may pay a proof-bearing dispute's rent itself; that rent
//!   returns to the buyer and never to the platform. After a successful step every actor's SOL moved
//!   by exactly its transaction fee plus that dispute rent, so no seller or buyer gains anything else.
//! - Each listing's spot_count / closed_spot_count match the spots added and closed. At the end the
//!   rent payer closes the open spots, and close_listing succeeds exactly for listings whose spots
//!   are all closed, and only for the rent payer.
//!
//! Reproduce a failure with the printed seed:
//! `ADSPACES_PROP_SEED=0x... cargo test -p adspaces --test test_property`.
//! `ADSPACES_PROP_CASES` / `ADSPACES_PROP_STEPS` override the defaults.

mod common;

use anchor_lang::{
    solana_program::{instruction::Instruction, rent, system_program},
    InstructionData, ToAccountMetas,
};
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_signer::Signer;
use spl_token_interface::state;
use std::cell::Cell;

use adspaces::constants::*;
use common::*;

const DEFAULT_CASES: u64 = 200;
const DEFAULT_STEPS: u64 = 30;
const BASE_SEED: u64 = 0xad5_9ace_5a1_5eed;
const USDC: u64 = 1_000_000;
const SELLER: usize = 3;
const TREASURY: usize = 4;
const MAX_SPOTS: usize = 8;
/// Names the listing's real rent payer in action descriptions.
const PLATFORM: &str = "platform rent payer";

/// xorshift64: deterministic, dependency-free.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        for _ in 0..4 {
            rng.next();
        }
        rng
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    /// Inclusive range.
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.below(hi - lo + 1)
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

struct SpotModel {
    listing: usize,
    spot: Pubkey,
    vault: Pubkey,
    price: u64,
    /// Amount escrowed by the buyer / top bidder, per the test's own bookkeeping.
    escrow: u64,
    /// Tokens sent straight to the vault outside the program.
    donations: u64,
    closed: bool,
    /// Actor that paid the open dispute's rent itself (buyer-paid path); None when the platform did.
    dispute_payer: Option<usize>,
}

fn balance(svm: &LiteSVM, account: &Pubkey) -> Option<u64> {
    svm.get_account(account)
        .filter(|a| a.data.len() == state::Account::LEN)
        .map(|a| state::Account::unpack(&a.data).unwrap().amount)
}

thread_local! {
    /// Fee payer and fee of the last successful `exec`.
    static LAST_FEE: Cell<(Pubkey, u64)> = const { Cell::new((Pubkey::new_from_array([0; 32]), 0)) };
}

fn exec(svm: &mut LiteSVM, ix: Instruction, payer: &Keypair, extra: &[&Keypair]) -> bool {
    // A fresh blockhash per transaction, so repeating an identical instruction is never
    // rejected as a duplicate.
    svm.expire_blockhash();
    match send(svm, &[ix], payer, extra) {
        Ok(meta) => {
            LAST_FEE.with(|fee| fee.set((payer.pubkey(), meta.fee)));
            true
        }
        Err(_) => false,
    }
}

struct Case {
    seed: u64,
    rng: Rng,
    env: Env,
    /// Buyers 0..3 and the seller at index 3.
    actors: Vec<Keypair>,
    /// Token accounts for each actor, then the treasury's at index 4.
    accounts: Vec<Pubkey>,
    listings: Vec<Pubkey>,
    spots: Vec<SpotModel>,
    total: u64,
    /// Rent payer lamports plus every lamport held by the accounts it funded; constant, because
    /// the rent payer never pays a transaction fee and all rent returns to it.
    rent_total: u64,
    log: Vec<String>,
}

impl Case {
    fn new(seed: u64, base: &Env) -> Self {
        let mut rng = Rng::new(seed);
        // Cloning a ready environment skips reloading the program for every case.
        let mut env = Env {
            // Signatures are still required by the program (signer flags); skipping ed25519
            // verification and history only saves time in this debug-build harness.
            svm: base
                .svm
                .clone()
                .with_sigverify(false)
                .with_transaction_history(0),
            mint_authority: base.mint_authority.insecure_clone(),
            usdc: base.usdc,
            config: base.config,
            program_data: base.program_data,
            review_authority: base.review_authority,
            treasury: base.treasury,
        };
        let mut actors = Vec::new();
        let mut accounts = Vec::new();
        let mut total = 0;
        for i in 0..4 {
            let amount = if i == SELLER {
                rng.range(0, 50) * USDC
            } else {
                rng.range(1, 2_000) * USDC + rng.below(USDC)
            };
            let (keypair, ata) = env.new_user(amount);
            total += amount;
            actors.push(keypair);
            accounts.push(ata);
        }
        let treasury = env.treasury;
        let treasury_ata = empty_token_account(&mut env, &actors[SELLER], &treasury);
        accounts.push(treasury_ata);

        let mut case = Case {
            seed,
            rng,
            env,
            actors,
            accounts,
            listings: Vec::new(),
            spots: Vec::new(),
            total,
            rent_total: 0,
            log: Vec::new(),
        };
        case.create_listing(MODE_FIXED);
        case.create_listing(MODE_AUCTION);
        for listing in 0..2 {
            for _ in 0..case.rng.range(1, 3) {
                assert!(
                    case.add_spot(listing),
                    "seed {seed:#x}: setup add_spot failed"
                );
            }
        }
        case.rent_total = case.rent_pool();
        case
    }

    fn fail(&self, message: String) -> ! {
        panic!(
            "property violated (seed {:#x}): {message}\naction log:\n  {}",
            self.seed,
            self.log.join("\n  ")
        );
    }

    fn create_listing(&mut self, mode: u8) {
        let slug_hash = [self.listings.len() as u8 + 1; 16];
        let seller = &self.actors[SELLER];
        let (listing, _) = listing_pda(&seller.pubkey(), &slug_hash);
        let now = self.env.clock().unix_timestamp;
        let deadline = now + self.rng.range(3_600, 3 * 86_400) as i64;
        let artwork_close = if self.rng.chance(50) {
            deadline
        } else {
            deadline + self.rng.range(1, 86_400) as i64
        };
        let delivery_deadline =
            artwork_close + self.rng.range(MIN_PROOF_WINDOW_SECONDS as u64, 3 * 86_400) as i64;
        let review = self.rng.range(MIN_REVIEW_WINDOW_SECONDS as u64, 3 * 86_400) as i64;
        let fee_bps = self.rng.range(250, 1_000) as u16;
        let (double_up, min_increment, uplift_fee_bps, max_fee_bps) = if mode == MODE_FIXED {
            (false, 0, 0, fee_bps)
        } else {
            let max = self.rng.range(fee_bps as u64, 2_000) as u16;
            let uplift = self.rng.range(max as u64, 3_000) as u16;
            let double_up = self.rng.chance(25);
            let increment = if double_up {
                0
            } else {
                self.rng.range(MIN_BID_INCREMENT, 5 * USDC)
            };
            (double_up, increment, uplift, max)
        };
        let ix = new_ix(
            program_id(),
            adspaces::instruction::CreateListing {
                slug_hash,
                metadata_uri: "https://adspaces.test/api/listings/property".to_string(),
                mode,
                deadline,
                artwork_close,
                delivery_deadline,
                review_window_seconds: review,
                min_increment,
                double_up,
                fee_bps,
                uplift_fee_bps,
                max_fee_bps,
            }
            .data(),
            adspaces::accounts::CreateListing {
                seller: seller.pubkey(),
                reviewer: self.env.mint_authority.pubkey(),
                rent_payer: self.env.rent_payer(),
                listing,
                config: self.env.config,
                review_authority: self.env.review_authority,
                usdc_mint: self.env.usdc,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        );
        assert!(
            exec(&mut self.env.svm, ix, seller, &[&self.env.mint_authority]),
            "seed {:#x}: create_listing failed",
            self.seed
        );
        self.listings.push(listing);
    }

    fn add_spot(&mut self, listing_index: usize) -> bool {
        let listing = self.listings[listing_index];
        let index = read_listing(&self.env.svm, &listing).spot_count;
        let (spot, _) = spot_pda(&listing, index);
        let (vault, _) = vault_pda(&spot);
        let price = if self.rng.chance(20) {
            self.rng.range(1, 20_000)
        } else {
            self.rng.range(USDC, 100 * USDC)
        };
        let ix = new_ix(
            program_id(),
            adspaces::instruction::AddSpot { price }.data(),
            adspaces::accounts::AddSpot {
                seller: self.actors[SELLER].pubkey(),
                listing,
                spot,
                vault,
                config: self.env.config,
                usdc_mint: self.env.usdc,
                system_program: system_program::ID,
                token_program: token_program_id(),
                rent: rent::ID,
                reviewer: self.env.mint_authority.pubkey(),
                rent_payer: self.env.rent_payer(),
                review_authority: self.env.review_authority,
            }
            .to_account_metas(None),
        );
        let ok = exec(
            &mut self.env.svm,
            ix,
            &self.actors[SELLER],
            &[&self.env.mint_authority],
        );
        if ok {
            self.spots.push(SpotModel {
                listing: listing_index,
                spot,
                vault,
                price,
                escrow: 0,
                donations: 0,
                closed: false,
                dispute_payer: None,
            });
        }
        ok
    }

    // ---- views -------------------------------------------------------------------------------

    fn is_fixed(&self, s: usize) -> bool {
        self.spots[s].listing == 0
    }

    fn listing_of(&self, s: usize) -> adspaces::state::Listing {
        read_listing(&self.env.svm, &self.listings[self.spots[s].listing])
    }

    fn chain_spot(&self, s: usize) -> Option<adspaces::state::Spot> {
        self.env
            .svm
            .get_account(&self.spots[s].spot)
            .map(|_| read_spot(&self.env.svm, &self.spots[s].spot))
    }

    fn actor_of(&self, key: &Pubkey) -> Option<usize> {
        self.actors.iter().position(|a| a.pubkey() == *key)
    }

    /// Actor index of the spot's buyer (fixed) or top bidder (auction).
    fn winner(&self, s: usize) -> Option<usize> {
        let spot = self.chain_spot(s)?;
        let key = if self.is_fixed(s) {
            spot.buyer
        } else {
            spot.current_bidder
        };
        self.actor_of(&key)
    }

    fn random_actor(&mut self) -> usize {
        self.rng.below(4) as usize
    }

    /// Usually the winner, otherwise anyone.
    fn likely_winner(&mut self, s: usize) -> usize {
        match self.winner(s) {
            Some(w) if self.rng.chance(85) => w,
            _ => self.random_actor(),
        }
    }

    fn proof_account(&self, s: usize) -> Option<Pubkey> {
        let (proof, _) = proof_pda(&self.spots[s].spot);
        self.env.svm.get_account(&proof).map(|_| proof)
    }

    fn dispute_rent_payer(&self, s: usize) -> Pubkey {
        let (dispute, _) = dispute_pda(&self.spots[s].spot);
        if self.env.svm.get_account(&dispute).is_some() {
            read_dispute(&self.env.svm, &dispute).rent_payer
        } else {
            self.env.mint_authority.pubkey()
        }
    }

    /// Balances of every tracked token account: actors, treasury, then one vault per spot.
    fn snapshot(&self) -> Vec<Option<u64>> {
        self.accounts
            .iter()
            .chain(self.spots.iter().map(|s| &s.vault))
            .map(|a| balance(&self.env.svm, a))
            .collect()
    }

    /// Rent payer lamports plus the lamports of every listing, spot, vault, proof and platform-paid
    /// dispute account (closed accounts count as 0). A buyer-paid dispute's rent is the buyer's.
    fn rent_pool(&self) -> u64 {
        let svm = &self.env.svm;
        let funded: u64 = self
            .listings
            .iter()
            .map(|l| lamports(svm, l))
            .chain(self.spots.iter().flat_map(|m| {
                let dispute = if m.dispute_payer.is_none() {
                    lamports(svm, &dispute_pda(&m.spot).0)
                } else {
                    0
                };
                [m.spot, m.vault, proof_pda(&m.spot).0]
                    .map(|a| lamports(svm, &a))
                    .into_iter()
                    .chain([dispute])
            }))
            .sum();
        lamports(svm, &self.env.rent_payer()) + funded
    }

    fn actor_lamports(&self) -> Vec<u64> {
        self.actors
            .iter()
            .map(|a| lamports(&self.env.svm, &a.pubkey()))
            .collect()
    }

    fn vault_slot(s: usize) -> usize {
        TREASURY + 1 + s
    }

    /// Every tracked account changed by exactly the listed amounts and nothing else moved.
    fn expect_deltas(
        &self,
        before: &[Option<u64>],
        after: &[Option<u64>],
        deltas: &[(usize, i128)],
    ) {
        for i in 0..before.len() {
            let expected: i128 = deltas.iter().filter(|(j, _)| *j == i).map(|(_, d)| d).sum();
            let actual = after[i].unwrap_or(0) as i128 - before[i].unwrap_or(0) as i128;
            if actual != expected {
                self.fail(format!(
                    "token account #{i} moved by {actual}, expected {expected} (before {:?}, after {:?})",
                    before, after
                ));
            }
        }
    }

    // ---- the step ----------------------------------------------------------------------------

    fn step(&mut self) {
        let s = self.rng.below(self.spots.len() as u64) as usize;
        let before = self.snapshot();
        let lamports_before = self.actor_lamports();
        let vault_before = before[Self::vault_slot(s)].unwrap_or(0);
        let pre_spot = self.chain_spot(s);
        let pre_listing = self.listing_of(s);
        let (dispute, _) = dispute_pda(&self.spots[s].spot);
        let pre_dispute_lamports = lamports(&self.env.svm, &dispute);
        let pre_timeout_at = self
            .env
            .svm
            .get_account(&dispute)
            .map(|_| read_dispute(&self.env.svm, &dispute))
            .map(|d| d.opened_at + d.timeout_seconds);
        let pre_dispute_payer = self.spots[s].dispute_payer;
        LAST_FEE.with(|fee| fee.set((Pubkey::default(), 0)));
        let now = self.env.clock().unix_timestamp;
        let roll = self.rng.below(100);

        let (description, ok, expected): (String, bool, Vec<(usize, i128)>) = match roll {
            0..=7 => self.claim(s),
            8..=21 => self.bid(s),
            22..=27 => self.donate(s),
            28..=41 => self.warp(s),
            42..=49 => self.submit_proof(s),
            50..=57 => self.raise_dispute(s),
            58..=61 => self.withdraw_dispute(s),
            62..=67 => self.resolve(s, vault_before),
            68..=72 => self.timeout(s, vault_before),
            73..=78 => self.refund(s, vault_before, true),
            79..=84 => self.refund(s, vault_before, false),
            85..=91 => self.withdraw_seller(s, vault_before),
            92..=96 => self.close_spot(s, vault_before),
            _ => {
                let listing = self.rng.below(2) as usize;
                if self.spots.len() < MAX_SPOTS {
                    let ok = self.add_spot(listing);
                    (format!("add_spot(listing {listing})"), ok, vec![])
                } else {
                    ("add_spot skipped (max spots)".to_string(), false, vec![])
                }
            }
        };
        self.log.push(format!(
            "t={now} spot {s}: {description} -> {}",
            if ok { "ok" } else { "err" }
        ));

        let mut after = self.snapshot();
        after.truncate(before.len()); // A newly added spot's empty vault is checked below.
        if ok {
            self.expect_deltas(&before, &after, &expected);
            self.check_success_preconditions(
                &description,
                s,
                pre_spot.as_ref(),
                &pre_listing,
                pre_timeout_at,
                now,
            );
        } else if after != before {
            self.fail(format!(
                "failed transaction moved tokens: {before:?} -> {after:?}"
            ));
        }
        // Rent never lands on the seller or a buyer, except a buyer-paid dispute's rent coming back
        // to that buyer: on success each actor's SOL moves by exactly its fee and that rent.
        let lamports_after = self.actor_lamports();
        let mut rent_delta = vec![0i128; self.actors.len()];
        if ok {
            match (pre_dispute_payer, self.spots[s].dispute_payer) {
                (None, Some(payer)) => {
                    rent_delta[payer] -= lamports(&self.env.svm, &dispute) as i128;
                }
                (Some(payer), None) => rent_delta[payer] += pre_dispute_lamports as i128,
                _ => {}
            }
        }
        let (fee_payer, fee) = LAST_FEE.with(|fee| fee.get());
        for (i, (b, a)) in lamports_before.iter().zip(&lamports_after).enumerate() {
            let actual = *a as i128 - *b as i128;
            if ok {
                let paid = if self.actors[i].pubkey() == fee_payer {
                    fee as i128
                } else {
                    0
                };
                if actual != rent_delta[i] - paid {
                    self.fail(format!(
                        "actor {i} SOL moved by {actual}, expected rent {} minus fee {paid}",
                        rent_delta[i]
                    ));
                }
            } else if actual > 0 {
                self.fail(format!(
                    "actor {i} gained {actual} lamports in a failed step"
                ));
            }
        }
        self.check_invariants();
    }

    /// Successful instructions must have been legal in the pre-state.
    fn check_success_preconditions(
        &self,
        description: &str,
        s: usize,
        pre_spot: Option<&adspaces::state::Spot>,
        listing: &adspaces::state::Listing,
        timeout_at: Option<i64>,
        now: i64,
    ) {
        let review_end = listing.delivery_deadline + listing.review_window_seconds;
        let status = pre_spot.map(|p| p.status);
        let proof = pre_spot.map(|p| p.proof_submitted_at > 0).unwrap_or(false);
        let legal = if description.starts_with("claim") {
            status == Some(SPOT_OPEN) && now < listing.deadline && listing.mode == MODE_FIXED
        } else if description.starts_with("bid") {
            matches!(status, Some(SPOT_OPEN) | Some(SPOT_FUNDED))
                && now < listing.deadline
                && listing.mode == MODE_AUCTION
        } else if description.starts_with("submit_proof") {
            status == Some(SPOT_FUNDED)
                && now >= listing.artwork_close
                && now < listing.delivery_deadline
                && description == format!("submit_proof(actor {SELLER}, {PLATFORM})")
        } else if description.starts_with("raise_dispute") {
            status == Some(SPOT_FUNDED)
                && now >= listing.delivery_deadline
                && (!proof || now < review_end)
                && (proof || !description.contains("buyer-paid"))
        } else if description.starts_with("withdraw_dispute") {
            status == Some(SPOT_DISPUTED)
        } else if description.starts_with("resolve") {
            status == Some(SPOT_DISPUTED) && timeout_at.is_some_and(|t| now < t)
        } else if description.starts_with("timeout") {
            status == Some(SPOT_DISPUTED) && timeout_at.is_some_and(|t| now >= t)
        } else if description.starts_with("refund(third party") {
            status == Some(SPOT_FUNDED) && now >= listing.delivery_deadline && !proof
        } else if description.starts_with("refund") {
            status == Some(SPOT_FUNDED)
        } else if description.starts_with("withdraw_seller") {
            status == Some(SPOT_FUNDED) && proof && now >= review_end
        } else if description.starts_with("close_spot") {
            status == Some(SPOT_OPEN) && description.starts_with(&format!("close_spot({PLATFORM}"))
        } else {
            true
        };
        if !legal {
            self.fail(format!(
                "`{description}` on spot {s} succeeded from an illegal state (status {status:?}, \
                 proof {proof}, now {now}, deadline {}, delivery {}, review_end {review_end})",
                listing.deadline, listing.delivery_deadline
            ));
        }
    }

    fn check_invariants(&self) {
        let snapshot = self.snapshot();
        let held: u128 = snapshot.iter().map(|b| b.unwrap_or(0) as u128).sum();
        let mint = self.env.svm.get_account(&self.env.usdc).unwrap();
        let supply = state::Mint::unpack(&mint.data).unwrap().supply;
        if held != self.total as u128 || supply != self.total {
            self.fail(format!(
                "USDC not conserved: tracked {held}, supply {supply}, expected {}",
                self.total
            ));
        }

        let rent_pool = self.rent_pool();
        if rent_pool != self.rent_total {
            self.fail(format!(
                "rent not conserved: rent payer + funded accounts hold {rent_pool}, expected {}",
                self.rent_total
            ));
        }
        for (l, listing) in self.listings.iter().enumerate() {
            if self.env.svm.get_account(listing).is_none() {
                continue;
            }
            let on_chain = read_listing(&self.env.svm, listing);
            let spots = self.spots.iter().filter(|m| m.listing == l);
            let added = spots.clone().count();
            let closed = spots.filter(|m| m.closed).count();
            if on_chain.spot_count as usize != added
                || on_chain.closed_spot_count as usize != closed
            {
                self.fail(format!(
                    "listing {l} counts {}/{} closed, tracked {closed}/{added}",
                    on_chain.closed_spot_count, on_chain.spot_count
                ));
            }
        }

        for (s, model) in self.spots.iter().enumerate() {
            let vault = balance(&self.env.svm, &model.vault);
            let vault_exists = self.env.svm.get_account(&model.vault).is_some();
            let (dispute, _) = dispute_pda(&model.spot);
            let dispute_exists = self.env.svm.get_account(&dispute).is_some();
            match self.chain_spot(s) {
                None => {
                    if !model.closed {
                        self.fail(format!(
                            "spot {s} disappeared without a terminal instruction"
                        ));
                    }
                    if vault_exists {
                        self.fail(format!(
                            "spot {s} is closed but its vault still exists ({vault:?})"
                        ));
                    }
                    if dispute_exists {
                        self.fail(format!("spot {s} is closed but its dispute record remains"));
                    }
                }
                Some(spot) => {
                    if model.closed {
                        self.fail(format!(
                            "spot {s} still exists after a terminal instruction"
                        ));
                    }
                    let vault =
                        vault.unwrap_or_else(|| self.fail(format!("spot {s} lost its vault")));
                    let escrowed = match spot.status {
                        SPOT_OPEN => {
                            if spot.current_bid != 0 || model.escrow != 0 {
                                self.fail(format!("open spot {s} has a bid / escrow"));
                            }
                            0
                        }
                        SPOT_FUNDED | SPOT_DISPUTED => {
                            let on_chain = if self.is_fixed(s) {
                                spot.price
                            } else {
                                spot.current_bid
                            };
                            if on_chain != model.escrow {
                                self.fail(format!(
                                    "spot {s} escrow on chain {on_chain} != tracked {}",
                                    model.escrow
                                ));
                            }
                            on_chain
                        }
                        other => self.fail(format!("spot {s} left alive in status {other}")),
                    };
                    if vault != escrowed + model.donations {
                        self.fail(format!(
                            "A1.1: spot {s} (status {}) vault {vault} != escrow {escrowed} + donations {}",
                            spot.status, model.donations
                        ));
                    }
                    if dispute_exists != (spot.status == SPOT_DISPUTED) {
                        self.fail(format!(
                            "spot {s} status {} but dispute record exists = {dispute_exists}",
                            spot.status
                        ));
                    }
                }
            }
        }
    }

    // ---- actions -----------------------------------------------------------------------------

    fn claim(&mut self, s: usize) -> (String, bool, Vec<(usize, i128)>) {
        let a = self.random_actor();
        let m = &self.spots[s];
        let ix = new_ix(
            program_id(),
            adspaces::instruction::ClaimSpot {}.data(),
            adspaces::accounts::ClaimSpot {
                buyer: self.actors[a].pubkey(),
                listing: self.listings[m.listing],
                spot: m.spot,
                vault: m.vault,
                buyer_ata: self.accounts[a],
                config: self.env.config,
                usdc_mint: self.env.usdc,
                token_program: token_program_id(),
            }
            .to_account_metas(None),
        );
        let ok = exec(&mut self.env.svm, ix, &self.actors[a], &[]);
        let price = m.price;
        if ok {
            self.spots[s].escrow = price;
        }
        (
            format!("claim(actor {a})"),
            ok,
            vec![(a, -(price as i128)), (Self::vault_slot(s), price as i128)],
        )
    }

    fn bid(&mut self, s: usize) -> (String, bool, Vec<(usize, i128)>) {
        let a = self.random_actor();
        let listing = self.listing_of(s);
        let (current_bid, current_bidder) = self
            .chain_spot(s)
            .map(|p| (p.current_bid, p.current_bidder))
            .unwrap_or((0, Pubkey::default()));
        let has_bid = current_bidder != Pubkey::default();
        let price = self.spots[s].price;
        let minimum = if !has_bid {
            price
        } else if listing.double_up {
            current_bid.saturating_mul(2)
        } else {
            current_bid.saturating_add(listing.min_increment)
        };
        let funds = balance(&self.env.svm, &self.accounts[a]).unwrap_or(0);
        let (kind, amount) = match self.rng.below(10) {
            0..=3 => ("min", minimum),
            4..=6 => ("above", minimum + self.rng.range(1, 5 * USDC)),
            7 | 8 => {
                let gap = self.rng.range(1, minimum.max(2) - 1);
                ("too low", minimum.saturating_sub(gap))
            }
            _ => ("over balance", funds + 1),
        };
        let prev_actor = self.actor_of(&current_bidder);
        let prev_bidder_ata = match prev_actor {
            Some(p) if self.rng.chance(85) => Some(self.accounts[p]),
            _ if self.rng.chance(50) => None,
            _ => {
                let r = self.random_actor();
                Some(self.accounts[r])
            }
        };
        let m = &self.spots[s];
        let ix = new_ix(
            program_id(),
            adspaces::instruction::PlaceBid { amount }.data(),
            adspaces::accounts::PlaceBid {
                bidder: self.actors[a].pubkey(),
                listing: self.listings[m.listing],
                spot: m.spot,
                vault: m.vault,
                bidder_ata: self.accounts[a],
                prev_bidder_ata,
                config: self.env.config,
                usdc_mint: self.env.usdc,
                token_program: token_program_id(),
            }
            .to_account_metas(None),
        );
        let ok = exec(&mut self.env.svm, ix, &self.actors[a], &[]);
        let mut expected = vec![
            (a, -(amount as i128)),
            (Self::vault_slot(s), amount as i128),
        ];
        if has_bid {
            // The outbid bidder gets exactly their bid back, out of the vault.
            let p = prev_actor.expect("top bidder is always a known actor");
            expected.push((p, current_bid as i128));
            expected.push((Self::vault_slot(s), -(current_bid as i128)));
        }
        if ok {
            if amount < minimum {
                self.fail(format!("bid {amount} accepted below minimum {minimum}"));
            }
            if prev_actor == Some(a) {
                self.fail(format!("actor {a} outbid itself"));
            }
            self.spots[s].escrow = amount;
        }
        (
            format!("bid(actor {a}, {kind} {amount}, min {minimum}, prev {prev_actor:?})"),
            ok,
            expected,
        )
    }

    fn donate(&mut self, s: usize) -> (String, bool, Vec<(usize, i128)>) {
        let a = self.random_actor();
        let amount = if self.rng.chance(50) {
            self.rng.range(1, 100)
        } else {
            self.rng.range(1, 5 * USDC)
        };
        let ix = spl_token::instruction::transfer(
            &token_program_id(),
            &self.accounts[a],
            &self.spots[s].vault,
            &self.actors[a].pubkey(),
            &[],
            amount,
        )
        .unwrap();
        let ok = exec(&mut self.env.svm, ix, &self.actors[a], &[]);
        if ok {
            self.spots[s].donations += amount;
        }
        (
            format!("donate(actor {a}, {amount})"),
            ok,
            vec![
                (a, -(amount as i128)),
                (Self::vault_slot(s), amount as i128),
            ],
        )
    }

    fn warp(&mut self, s: usize) -> (String, bool, Vec<(usize, i128)>) {
        let now = self.env.clock().unix_timestamp;
        let target = match self.rng.below(5) {
            0 => now + self.rng.range(1, 900) as i64,
            1 => now + self.rng.range(3_600, 86_400) as i64,
            2 => now + self.rng.range(86_400, 4 * 86_400) as i64,
            _ => {
                // Land on (or right next to) a timeline boundary of this spot.
                let listing = self.listing_of(s);
                let mut marks = vec![
                    listing.deadline - self.rng.range(0, SNIPE_WINDOW_SECONDS as u64) as i64,
                    listing.deadline,
                    listing.artwork_close,
                    listing.delivery_deadline,
                    listing.delivery_deadline + listing.review_window_seconds,
                ];
                let (dispute, _) = dispute_pda(&self.spots[s].spot);
                if self.env.svm.get_account(&dispute).is_some() {
                    let d = read_dispute(&self.env.svm, &dispute);
                    marks.push(d.opened_at + d.timeout_seconds);
                }
                let mark = marks[self.rng.below(marks.len() as u64) as usize];
                mark + self.rng.below(3) as i64 - 1
            }
        };
        if target <= now {
            return (format!("warp to {target} skipped (past)"), false, vec![]);
        }
        self.env.warp_to_timestamp(target);
        (format!("warp +{}", target - now), true, vec![])
    }

    fn submit_proof(&mut self, s: usize) -> (String, bool, Vec<(usize, i128)>) {
        let a = if self.rng.chance(85) {
            SELLER
        } else {
            self.random_actor()
        };
        // Usually the listing's rent payer co-signs; sometimes an actor poses as it (must be rejected).
        let impostor = if self.rng.chance(90) {
            None
        } else {
            Some(self.random_actor())
        };
        let m = &self.spots[s];
        let (proof, _) = proof_pda(&m.spot);
        let rent_payer = match impostor {
            None => &self.env.mint_authority,
            Some(i) => &self.actors[i],
        };
        let ix = new_ix(
            program_id(),
            adspaces::instruction::SubmitProof {
                proof_hash: [7; 32],
                proof_uri: "ipfs://adspaces-property-proof".to_string(),
            }
            .data(),
            adspaces::accounts::SubmitProof {
                seller: self.actors[a].pubkey(),
                rent_payer: rent_payer.pubkey(),
                listing: self.listings[m.listing],
                spot: m.spot,
                proof,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        );
        let ok = exec(&mut self.env.svm, ix, &self.actors[a], &[rent_payer]);
        let payer = match impostor {
            None => PLATFORM.to_string(),
            Some(i) => format!("rent payer actor {i}"),
        };
        (format!("submit_proof(actor {a}, {payer})"), ok, vec![])
    }

    fn raise_dispute(&mut self, s: usize) -> (String, bool, Vec<(usize, i128)>) {
        let a = self.likely_winner(s);
        let (evidence_hash, evidence_uri, kind) = match self.rng.below(4) {
            0 => ([0; 32], String::new(), "no evidence"),
            1 => ([9; 32], String::new(), "hash without uri"),
            _ => (
                [9; 32],
                "ipfs://adspaces-property-dispute".to_string(),
                "evidence",
            ),
        };
        // Usually the platform co-signs and pays the rent; sometimes the buyer pays it instead.
        let buyer_paid = self.rng.chance(30);
        let m = &self.spots[s];
        let (dispute, _) = dispute_pda(&m.spot);
        let rent_payer = if buyer_paid {
            self.actors[a].pubkey()
        } else {
            self.env.mint_authority.pubkey()
        };
        let ix = new_ix(
            program_id(),
            adspaces::instruction::RaiseDispute {
                evidence_hash,
                evidence_uri,
            }
            .data(),
            adspaces::accounts::RaiseDispute {
                buyer: self.actors[a].pubkey(),
                rent_payer,
                config: self.env.config,
                listing: self.listings[m.listing],
                spot: m.spot,
                dispute,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        );
        let co_signers: Vec<&Keypair> = if buyer_paid {
            vec![]
        } else {
            vec![&self.env.mint_authority]
        };
        let ok = exec(&mut self.env.svm, ix, &self.actors[a], &co_signers);
        if ok && buyer_paid {
            self.spots[s].dispute_payer = Some(a);
        }
        let payer = if buyer_paid { "buyer-paid" } else { PLATFORM };
        (
            format!("raise_dispute(actor {a}, {kind}, {payer})"),
            ok,
            vec![],
        )
    }

    fn withdraw_dispute(&mut self, s: usize) -> (String, bool, Vec<(usize, i128)>) {
        let a = self.likely_winner(s);
        let rent_payer = self.dispute_rent_payer(s);
        let m = &self.spots[s];
        let (dispute, _) = dispute_pda(&m.spot);
        let ix = new_ix(
            program_id(),
            adspaces::instruction::WithdrawDispute {}.data(),
            adspaces::accounts::WithdrawDispute {
                buyer: self.actors[a].pubkey(),
                rent_payer,
                listing: self.listings[m.listing],
                spot: m.spot,
                dispute,
            }
            .to_account_metas(None),
        );
        let ok = exec(&mut self.env.svm, ix, &self.actors[a], &[]);
        if ok {
            self.spots[s].dispute_payer = None;
        }
        (format!("withdraw_dispute(actor {a})"), ok, vec![])
    }

    fn resolve(&mut self, s: usize, vault_before: u64) -> (String, bool, Vec<(usize, i128)>) {
        let bps: u16 = match self.rng.below(8) {
            0 => 0,
            1 => 10_000,
            2 => self.rng.range(10_001, 20_000) as u16,
            _ => self.rng.range(0, 10_000) as u16,
        };
        let b = self.likely_winner(s);
        let dispute_rent_payer = self.dispute_rent_payer(s);
        let proof = self.proof_account(s);
        let fee = match self.chain_spot(s) {
            Some(spot) if bps <= 10_000 => {
                adspaces::fees::calculate_dispute_fee(&self.listing_of(s), &spot, vault_before, bps)
                    .unwrap()
            }
            _ => 0,
        };
        let m = &self.spots[s];
        let (dispute, _) = dispute_pda(&m.spot);
        let ix = new_ix(
            program_id(),
            adspaces::instruction::ResolveDispute {
                seller_share_bps: bps,
                decision_hash: [8; 32],
                decision_uri: "ipfs://adspaces-property-decision".to_string(),
            }
            .data(),
            adspaces::accounts::ResolveDispute {
                resolver: self.env.mint_authority.pubkey(),
                config: self.env.config,
                listing: self.listings[m.listing],
                spot: m.spot,
                dispute,
                rent_payer: self.env.rent_payer(),
                dispute_rent_payer,
                vault: m.vault,
                seller_ata: self.accounts[SELLER],
                seller: self.actors[SELLER].pubkey(),
                buyer_ata: self.accounts[b],
                treasury_ata: self.accounts[TREASURY],
                proof,
                usdc_mint: self.env.usdc,
                token_program: token_program_id(),
            }
            .to_account_metas(None),
        );
        // The resolver co-signs; an actor pays the fee, so the rent payer only ever moves rent.
        let fee_payer = self.random_actor();
        let ok = exec(
            &mut self.env.svm,
            ix,
            &self.actors[fee_payer],
            &[&self.env.mint_authority],
        );
        let seller_gross = (vault_before as u128 * bps.min(10_000) as u128 / 10_000) as i128;
        let expected = vec![
            (b, vault_before as i128 - seller_gross),
            (SELLER, seller_gross - fee as i128),
            (TREASURY, fee as i128),
            (Self::vault_slot(s), -(vault_before as i128)),
        ];
        if ok {
            self.spots[s].closed = true;
            self.spots[s].dispute_payer = None;
        }
        (format!("resolve(bps {bps}, buyer actor {b})"), ok, expected)
    }

    fn timeout(&mut self, s: usize, vault_before: u64) -> (String, bool, Vec<(usize, i128)>) {
        let caller = self.random_actor();
        let b = self.likely_winner(s);
        let dispute_rent_payer = self.dispute_rent_payer(s);
        let proof = self.proof_account(s);
        let m = &self.spots[s];
        let (dispute, _) = dispute_pda(&m.spot);
        let ix = new_ix(
            program_id(),
            adspaces::instruction::TimeoutDispute {}.data(),
            adspaces::accounts::TimeoutDispute {
                caller: self.actors[caller].pubkey(),
                config: self.env.config,
                listing: self.listings[m.listing],
                spot: m.spot,
                dispute,
                rent_payer: self.env.rent_payer(),
                dispute_rent_payer,
                proof,
                vault: m.vault,
                buyer_ata: self.accounts[b],
                usdc_mint: self.env.usdc,
                token_program: token_program_id(),
            }
            .to_account_metas(None),
        );
        let ok = exec(&mut self.env.svm, ix, &self.actors[caller], &[]);
        if ok {
            self.spots[s].closed = true;
            self.spots[s].dispute_payer = None;
        }
        (
            format!("timeout(caller {caller}, buyer actor {b})"),
            ok,
            vec![
                (b, vault_before as i128),
                (Self::vault_slot(s), -(vault_before as i128)),
            ],
        )
    }

    fn refund(
        &mut self,
        s: usize,
        vault_before: u64,
        by_seller: bool,
    ) -> (String, bool, Vec<(usize, i128)>) {
        let caller = if by_seller {
            SELLER
        } else {
            self.rng.below(3) as usize
        };
        let b = self.likely_winner(s);
        let proof = self.proof_account(s);
        let m = &self.spots[s];
        let ix = new_ix(
            program_id(),
            adspaces::instruction::RefundSpot {}.data(),
            adspaces::accounts::RefundSpot {
                caller: self.actors[caller].pubkey(),
                listing: self.listings[m.listing],
                rent_payer: self.env.rent_payer(),
                spot: m.spot,
                vault: m.vault,
                buyer_ata: self.accounts[b],
                usdc_mint: self.env.usdc,
                token_program: token_program_id(),
                proof,
            }
            .to_account_metas(None),
        );
        let ok = exec(&mut self.env.svm, ix, &self.actors[caller], &[]);
        if ok {
            self.spots[s].closed = true;
        }
        let who = if by_seller {
            "seller".to_string()
        } else {
            format!("third party {caller}")
        };
        (
            format!("refund({who}, buyer actor {b})"),
            ok,
            vec![
                (b, vault_before as i128),
                (Self::vault_slot(s), -(vault_before as i128)),
            ],
        )
    }

    fn withdraw_seller(
        &mut self,
        s: usize,
        vault_before: u64,
    ) -> (String, bool, Vec<(usize, i128)>) {
        let caller = self.random_actor();
        let fee = self
            .chain_spot(s)
            .map(|spot| {
                adspaces::fees::calculate_fee(&self.listing_of(s), &spot, vault_before).unwrap()
            })
            .unwrap_or(0);
        let m = &self.spots[s];
        let ix = new_ix(
            program_id(),
            adspaces::instruction::WithdrawSeller {}.data(),
            adspaces::accounts::WithdrawSeller {
                caller: self.actors[caller].pubkey(),
                listing: self.listings[m.listing],
                seller: self.actors[SELLER].pubkey(),
                rent_payer: self.env.rent_payer(),
                spot: m.spot,
                vault: m.vault,
                seller_ata: self.accounts[SELLER],
                treasury_ata: self.accounts[TREASURY],
                proof: proof_pda(&m.spot).0,
                usdc_mint: self.env.usdc,
                token_program: token_program_id(),
            }
            .to_account_metas(None),
        );
        let ok = exec(&mut self.env.svm, ix, &self.actors[caller], &[]);
        if ok {
            self.spots[s].closed = true;
        }
        (
            format!("withdraw_seller(caller {caller})"),
            ok,
            vec![
                (SELLER, vault_before as i128 - fee as i128),
                (TREASURY, fee as i128),
                (Self::vault_slot(s), -(vault_before as i128)),
            ],
        )
    }

    fn close_spot(&mut self, s: usize, vault_before: u64) -> (String, bool, Vec<(usize, i128)>) {
        // Usually the seller's own token account; sometimes someone else's (must be rejected).
        let destination = if self.rng.chance(85) {
            SELLER
        } else {
            self.random_actor()
        };
        // Usually the listing's rent payer; sometimes the seller or another actor (must be rejected).
        let intruder = if self.rng.chance(80) {
            None
        } else {
            Some(self.random_actor())
        };
        let signer = match intruder {
            None => &self.env.mint_authority,
            Some(i) => &self.actors[i],
        };
        let m = &self.spots[s];
        let ix = new_ix(
            program_id(),
            adspaces::instruction::CloseSpot {}.data(),
            adspaces::accounts::CloseSpot {
                rent_payer: signer.pubkey(),
                listing: self.listings[m.listing],
                spot: m.spot,
                vault: m.vault,
                seller_ata: self.accounts[destination],
                usdc_mint: self.env.usdc,
                token_program: token_program_id(),
            }
            .to_account_metas(None),
        );
        // The seller pays the fee, so the rent payer only ever moves rent.
        let ok = exec(&mut self.env.svm, ix, &self.actors[SELLER], &[signer]);
        if ok {
            self.spots[s].closed = true;
        }
        let by = match intruder {
            None => PLATFORM.to_string(),
            Some(i) => format!("actor {i}"),
        };
        (
            format!("close_spot({by}, to actor {destination})"),
            ok,
            vec![
                (destination, vault_before as i128),
                (Self::vault_slot(s), -(vault_before as i128)),
            ],
        )
    }

    // ---- wind-down ---------------------------------------------------------------------------

    /// The rent payer closes every spot still open, then each listing must close exactly when all
    /// of its spots are closed, only for the rent payer, and with its rent going back to it.
    fn finish(&mut self) {
        let lamports_before = self.actor_lamports();
        for s in 0..self.spots.len() {
            if self.chain_spot(s).map(|p| p.status) != Some(SPOT_OPEN) {
                continue;
            }
            let m = &self.spots[s];
            let ix = new_ix(
                program_id(),
                adspaces::instruction::CloseSpot {}.data(),
                adspaces::accounts::CloseSpot {
                    rent_payer: self.env.rent_payer(),
                    listing: self.listings[m.listing],
                    spot: m.spot,
                    vault: m.vault,
                    seller_ata: self.accounts[SELLER],
                    usdc_mint: self.env.usdc,
                    token_program: token_program_id(),
                }
                .to_account_metas(None),
            );
            let signer = self.env.mint_authority.insecure_clone();
            if !exec(&mut self.env.svm, ix, &self.actors[SELLER], &[&signer]) {
                self.fail(format!(
                    "wind-down: rent payer could not close open spot {s}"
                ));
            }
            self.spots[s].closed = true;
            self.log
                .push(format!("wind-down: close_spot({PLATFORM}) spot {s}"));
            self.check_invariants();
        }

        for l in 0..self.listings.len() {
            let listing = self.listings[l];
            let close = |rent_payer: Pubkey| {
                new_ix(
                    program_id(),
                    adspaces::instruction::CloseListing {}.data(),
                    adspaces::accounts::CloseListing {
                        rent_payer,
                        listing,
                    }
                    .to_account_metas(None),
                )
            };
            let seller = self.actors[SELLER].insecure_clone();
            if exec(&mut self.env.svm, close(seller.pubkey()), &seller, &[]) {
                self.fail(format!("wind-down: the seller closed listing {l}"));
            }
            let all_closed = self
                .spots
                .iter()
                .filter(|m| m.listing == l)
                .all(|m| m.closed);
            let signer = self.env.mint_authority.insecure_clone();
            let ok = exec(
                &mut self.env.svm,
                close(signer.pubkey()),
                &seller,
                &[&signer],
            );
            self.log.push(format!(
                "wind-down: close_listing({PLATFORM}) listing {l} -> {}",
                if ok { "ok" } else { "err" }
            ));
            if ok != all_closed {
                self.fail(format!(
                    "close_listing on listing {l} returned ok={ok} with all spots closed={all_closed}"
                ));
            }
            if ok && self.env.svm.get_account(&listing).is_some() {
                self.fail(format!("listing {l} survived close_listing"));
            }
            // The rent invariant covers the closed listing: its lamports are now the rent payer's.
            self.check_invariants();
        }

        let lamports_after = self.actor_lamports();
        for (i, (b, a)) in lamports_before.iter().zip(&lamports_after).enumerate() {
            if a > b {
                self.fail(format!("wind-down: actor {i} gained {} lamports", a - b));
            }
        }
    }
}

fn empty_token_account(env: &mut Env, funder: &Keypair, owner: &Pubkey) -> Pubkey {
    let account = Keypair::new();
    let create = anchor_lang::solana_program::system_instruction::create_account(
        &funder.pubkey(),
        &account.pubkey(),
        RENT_FUNDING,
        state::Account::LEN as u64,
        &token_program_id(),
    );
    send(&mut env.svm, &[create], funder, &[&account]).unwrap();
    let init = spl_token::instruction::initialize_account(
        &token_program_id(),
        &account.pubkey(),
        &env.usdc,
        owner,
    )
    .unwrap();
    send(&mut env.svm, &[init], funder, &[]).unwrap();
    account.pubkey()
}

fn env_u64(name: &str) -> Option<u64> {
    let raw = std::env::var(name).ok()?;
    let raw = raw.trim();
    match raw.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => raw.parse().ok(),
    }
}

fn run_case(seed: u64, steps: u64, base: &Env) -> u64 {
    let mut case = Case::new(seed, base);
    case.check_invariants();
    for _ in 0..steps {
        case.step();
    }
    case.finish();
    case.log.iter().filter(|l| l.ends_with("-> ok")).count() as u64
}

#[test]
fn random_sequences_preserve_escrow_invariants() {
    let steps = env_u64("ADSPACES_PROP_STEPS").unwrap_or(DEFAULT_STEPS);
    let seeds: Vec<u64> = match env_u64("ADSPACES_PROP_SEED") {
        Some(seed) => vec![seed],
        None => {
            let cases = env_u64("ADSPACES_PROP_CASES").unwrap_or(DEFAULT_CASES);
            (0..cases)
                .map(|i| BASE_SEED ^ i.wrapping_mul(0x2545_f491_4f6c_dd1d))
                .collect()
        }
    };
    let base = Env::new();
    let started = std::time::Instant::now();
    let mut succeeded = 0;
    for seed in &seeds {
        succeeded += run_case(*seed, steps, &base);
    }
    let total = seeds.len() as u64 * steps;
    println!(
        "{} cases x {steps} steps: {succeeded}/{total} actions succeeded in {:?}",
        seeds.len(),
        started.elapsed()
    );
    // Guard against a harness that only ever exercises failing paths.
    assert!(
        succeeded * 5 >= total,
        "too few successful actions: {succeeded}/{total}"
    );
}
