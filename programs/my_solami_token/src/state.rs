use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[account]
pub struct TokenState {
    pub admin: Pubkey,
    pub total_supply: u64,
    pub launch_time: i64,
    pub reward_distribution_start_time: i64,
    pub total_transactions: u64,
    pub total_tax_collected: u64,
    pub total_burned: u64,
    pub last_transfer_timestamp: i64,
    pub last_transfer_amount: u64,
    pub is_frozen: bool,
    pub liquidity_pool_balance: u64,
    pub staking_pool_balance: u64,
    pub rewards_pool_balance: u64,
}

impl TokenState {
    pub const SIZE: usize = 8 + // discriminator
        32 + // admin pubkey
        (8 * 8) + // existing u64 fields
        1 + // bool
        (8 * 3); // new TVL-related fields

    pub fn initialize(
        &mut self,
        admin: Pubkey,
        supply: u64,
        launch_time: i64,
    ) {
        self.admin = admin;
        self.total_supply = supply;
        self.launch_time = launch_time;
        self.reward_distribution_start_time = launch_time + 2520;
        self.total_transactions = 0;
        self.total_tax_collected = 0;
        self.total_burned = 0;
        self.last_transfer_timestamp = 0;
        self.last_transfer_amount = 0;
        self.is_frozen = false;
        self.liquidity_pool_balance = 0;
        self.staking_pool_balance = 0;
        self.rewards_pool_balance = 0;
    }

    pub fn update_liquidity_pool(&mut self, new_balance: u64) -> Result<()> {
        self.liquidity_pool_balance = new_balance;
        Ok(())
    }

    pub fn update_staking_pool(&mut self, new_balance: u64) -> Result<()> {
        self.staking_pool_balance = new_balance;
        Ok(())
    }

    pub fn update_rewards_pool(&mut self, new_balance: u64) -> Result<()> {
        self.rewards_pool_balance = new_balance;
        Ok(())
    }

    pub fn get_total_tvl(&self) -> u64 {
        self.liquidity_pool_balance
            .checked_add(self.staking_pool_balance)
            .and_then(|sum| sum.checked_add(self.rewards_pool_balance))
            .unwrap_or(0)
    }
}

#[account]
pub struct Whitelist {
    pub wallets: Vec<Pubkey>,
}

impl Whitelist {
    pub const SIZE: usize = 8 + // discriminator
        4 + // vec length
        (32 * 100); // max 100 wallets * pubkey size

    pub fn initialize(&mut self, wallets: Vec<Pubkey>) {
        self.wallets = wallets;
    }

    pub fn contains(&self, wallet: &Pubkey) -> bool {
        self.wallets.contains(wallet)
    }
}

// Account Validation Structures
#[derive(Accounts)]
pub struct InitializeToken<'info> {
    #[account(
        init,
        payer = admin,
        space = 8 + TokenState::SIZE,
        seeds = [b"token_state", mint.key().as_ref()],
        bump,
    )]
    pub token_state: Account<'info, TokenState>,

    #[account(
        init,
        payer = admin,
        space = 8 + Whitelist::SIZE,
        seeds = [b"whitelist", mint.key().as_ref()],
        bump,
    )]
    pub whitelist: Account<'info, Whitelist>,

    /// CHECK: This is the PDA that will be the mint authority
    #[account(
        seeds = [b"mint_authority", mint.key().as_ref()],
        bump,
    )]
    pub mint_authority: AccountInfo<'info>,

    #[account(mut)]
    pub mint: Account<'info, Mint>,
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut)]
    pub admin_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub lp_pool: Account<'info, TokenAccount>,
    #[account(mut)]
    pub rewards_pool: Account<'info, TokenAccount>,
    #[account(mut)]
    pub lp_fund: Account<'info, TokenAccount>,
    #[account(mut)]
    pub token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct TransferTokens<'info> {
    #[account(mut)]
    pub token_state: Account<'info, TokenState>,
    #[account(mut)]
    pub sender: Account<'info, TokenAccount>,
    #[account(mut)]
    pub receiver: Account<'info, TokenAccount>,
    #[account(mut)]
    pub rewards_pool: Account<'info, TokenAccount>,
    #[account(mut)]
    pub lp_fund: Account<'info, TokenAccount>,
    #[account(mut)]
    pub lp_pool: Account<'info, TokenAccount>,
    #[account(
        seeds = [b"mint_authority", mint.key().as_ref()],
        bump,
    )]
    pub sender_authority: AccountInfo<'info>,
    #[account(mut)]
    pub mint: Account<'info, Mint>,
    #[account(mut)]
    pub whitelist: Account<'info, Whitelist>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ManualBurn<'info> {
    #[account(mut)]
    pub mint: Account<'info, Mint>,
    #[account(mut)]
    pub admin_token_account: Account<'info, TokenAccount>,
    #[account(mut, address = token_state.admin)]
    pub admin: Signer<'info>,
    pub token_program: Program<'info, Token>,
    #[account(mut)]
    pub token_state: Account<'info, TokenState>,
}

#[derive(Accounts)]
pub struct SwapRewards<'info> {
    #[account(mut)]
    pub rewards_pool: Account<'info, TokenAccount>,
    #[account(mut)]
    pub swap_wallet: Account<'info, TokenAccount>,
    #[account(mut, address = token_state.admin)]
    pub admin: Signer<'info>,
    pub token_program: Program<'info, Token>,
    #[account(mut)]
    pub token_state: Account<'info, TokenState>,
    #[account(mut)]
    pub mint: Account<'info, Mint>,
}

#[derive(Accounts)]
pub struct WhitelistOperation<'info> {
    #[account(mut)]
    pub token_state: Account<'info, TokenState>,
    #[account(mut, address = token_state.admin)]
    pub admin: Signer<'info>,
    #[account(mut)]
    pub whitelist: Account<'info, Whitelist>,
}

#[derive(Accounts)]
pub struct TransferOwnership<'info> {
    #[account(mut)]
    pub token_state: Account<'info, TokenState>,
    #[account(mut, address = token_state.admin)]
    pub admin: Signer<'info>,
}

#[derive(Accounts)]
pub struct UserBurn<'info> {
    #[account(mut)]
    pub token_state: Account<'info, TokenState>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    pub user: Signer<'info>,
    #[account(mut)]
    pub mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct FreezeContract<'info> {
    #[account(mut)]
    pub token_state: Account<'info, TokenState>,
    #[account(mut, address = token_state.admin)]
    pub admin: Signer<'info>,
}

#[derive(Accounts)]
pub struct UpdateTVL<'info> {
    #[account(mut)]
    pub token_state: Account<'info, TokenState>,
    #[account(mut)]
    pub lp_pool: Account<'info, TokenAccount>,
    #[account(mut)]
    pub rewards_pool: Account<'info, TokenAccount>,
    #[account(mut)]
    pub staking_pool: Account<'info, TokenAccount>,
    #[account(mut, address = token_state.admin)]
    pub admin: Signer<'info>,
}

#[derive(Accounts, Clone)]
pub struct AllocateTaxAccounts<'info> {
    #[account(mut)]
    pub token_state: Account<'info, TokenState>,
    #[account(mut)]
    pub sender: Account<'info, TokenAccount>,
    #[account(mut)]
    pub rewards_pool: Account<'info, TokenAccount>,
    #[account(mut)]
    pub lp_fund: Account<'info, TokenAccount>,

    /// CHECK: This is the PDA that signs the tax allocation, validated by the program
    #[account(
        seeds = [b"mint_authority", mint.key().as_ref()],
        bump,
    )]
    pub sender_authority: AccountInfo<'info>,

    #[account(mut)]
    pub mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
}

// Events
#[event]
pub struct SwapEvent {
    pub amount: u64,
    pub timestamp: i64,
    pub pool: Pubkey,
}

#[event]
pub struct TransferEvent {
    pub sender: Pubkey,
    pub receiver: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
    pub tax_amount: u64,
}

#[event]
pub struct BurnEvent {
    pub burner: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
    pub new_total_supply: u64,
}

#[event]
pub struct WhitelistEvent {
    pub wallet: Pubkey,
    pub is_added: bool,
    pub timestamp: i64,
}

// Error Codes
#[error_code]
pub enum ErrorCode {
    #[msg("Invalid whitelist size")]
    InvalidWhitelistSize,
    #[msg("Insufficient balance for operation")]
    InsufficientBalance,
    #[msg("Trading not enabled yet")]
    TradingNotEnabled,
    #[msg("Whitelist is full")]
    WhitelistFull,
    #[msg("Address not in whitelist")]
    NotInWhitelist,
    #[msg("Unauthorized action")]
    Unauthorized,
    #[msg("Transfer cooldown active")]
    TransferCooldown,
    #[msg("Transfer amount exceeds limit")]
    TransferLimitExceeded,
    #[msg("Invalid new admin address")]
    InvalidAdminAddress,
    #[msg("Invalid supply")]
    InvalidSupply,
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("Contract is frozen")]
    ContractFrozen,
    #[msg("Arithmetic overflow")]
    ArithmeticOverflow,
    #[msg("Arithmetic underflow")]
    ArithmeticUnderflow,
    #[msg("Required account not found")]
    AccountNotFound,
    #[msg("Insufficient accounts provided")]
    InsufficientAccounts,
    #[msg("Account mismatch")]
    AccountMismatch,
}
