use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount, Transfer, Burn, MintTo, TokenProgram};
use spl_associated_token_account::{
    get_associated_token_address,
    instruction::create_associated_token_account,
};
use anchor_lang::solana_program::program::invoke;

mod state;
use state::{
    TokenState, Whitelist, SwapEvent, TransferEvent, BurnEvent, WhitelistEvent,
    ErrorCode, InitializeToken, TransferTokens, ManualBurn, SwapRewards, 
    WhitelistOperation, TransferOwnership, UserBurn, FreezeContract
};

declare_id!("EQ85HBoFJ6FiLz5NLZSuLnJ2Wr71q3P27rggw1z2WYAY");

#[program]
pub mod my_solami_token {
    use super::*;

    pub fn initialize_token<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, InitializeToken<'info>>,
        total_supply: u64,
        whitelist_wallets: Vec<Pubkey>,
    ) -> Result<()> {
        require!(total_supply > 0, ErrorCode::InvalidSupply);
        
        require!(
            whitelist_wallets.len() >= 1 && whitelist_wallets.len() <= 15,
            ErrorCode::InvalidWhitelistSize
        );

        let (mint_authority, _mint_bump) = Pubkey::find_program_address(
            &[b"mint_authority", ctx.accounts.mint.key().as_ref()],
            ctx.program_id
        );
        
        require!(
            ctx.accounts.mint_authority.key() == mint_authority,
            ErrorCode::AccountMismatch
        );

        ctx.accounts.token_state.initialize(
            ctx.accounts.admin.key(),
            total_supply,
            Clock::get()?.unix_timestamp,
        );

        distribute_initial_supply(
            ctx,
            total_supply,
            &whitelist_wallets,
        )?;

        Ok(())
    }

    pub fn transfer_tokens(
        ctx: Context<TransferTokens>,
        amount: u64,
    ) -> Result<()> {
        require!(
            Clock::get()?.unix_timestamp - ctx.accounts.token_state.launch_time >= 300,
            ErrorCode::TradingNotEnabled
        );

        require!(!ctx.accounts.token_state.is_frozen, ErrorCode::ContractFrozen);

        require!(
            ctx.accounts.sender.amount >= amount,
            ErrorCode::InsufficientBalance
        );

        require!(amount > 0, ErrorCode::InvalidAmount);

        let (net_amount, tax_amount) = calculate_transfer_amounts(
            &ctx.accounts.whitelist,
            &ctx.accounts.receiver.key(),
            amount,
        )?;

        transfer_within_program(
            &ctx.accounts.sender.to_account_info(),
            &ctx.accounts.receiver.to_account_info(),
            &ctx.accounts.sender_authority.to_account_info(),
            &ctx.accounts.token_program,
            net_amount,
        )?;

        if tax_amount > 0 {
            allocate_tax(
                AllocateTaxAccounts {
                    token_state: ctx.accounts.token_state.clone(),
                    sender: ctx.accounts.sender.clone(),
                    rewards_pool: ctx.accounts.rewards_pool.clone(),
                    lp_fund: ctx.accounts.lp_fund.clone(),
                    sender_authority: ctx.accounts.sender_authority.clone(),
                    mint: ctx.accounts.mint.clone(),
                    token_program: ctx.accounts.token_program.clone(),
                },
                tax_amount,
            )?;
        }

        ctx.accounts.token_state.total_transactions += 1;
        ctx.accounts.token_state.total_tax_collected += tax_amount;

        emit!(TransferEvent {
            sender: ctx.accounts.sender.key(),
            receiver: ctx.accounts.receiver.key(),
            amount: net_amount,
            timestamp: Clock::get()?.unix_timestamp,
            tax_amount,
        });

        // Update TVL-related balances after transfer
        if ctx.accounts.receiver.key() == ctx.accounts.lp_pool.key() {
            ctx.accounts.token_state.update_liquidity_pool(
                ctx.accounts.lp_pool.amount
            )?;
        } else if ctx.accounts.receiver.key() == ctx.accounts.rewards_pool.key() {
            ctx.accounts.token_state.update_rewards_pool(
                ctx.accounts.rewards_pool.amount
            )?;
        }

        Ok(())
    }

    pub fn manual_burn(
        ctx: Context<ManualBurn>,
        amount: u64,
    ) -> Result<()> {
        require!(
            ctx.accounts.admin.key() == ctx.accounts.token_state.admin,
            ErrorCode::Unauthorized
        );

        require!(
            ctx.accounts.admin_token_account.amount >= amount,
            ErrorCode::InsufficientBalance
        );

        burn_tokens(
            &ctx.accounts.mint,
            &ctx.accounts.admin_token_account,
            &ctx.accounts.admin.to_account_info(),
            &ctx.accounts.token_program,
            amount,
        )?;

        ctx.accounts.token_state.total_supply = ctx.accounts.token_state.total_supply
            .checked_sub(amount)
            .ok_or(ErrorCode::ArithmeticOverflow)?;
        ctx.accounts.token_state.total_burned += amount;

        emit!(BurnEvent {
            burner: ctx.accounts.admin.key(),
            amount,
            timestamp: Clock::get()?.unix_timestamp,
            new_total_supply: ctx.accounts.token_state.total_supply,
        });

        Ok(())
    }

    pub fn prepare_rewards_swap(
        ctx: Context<SwapRewards>,
        amount: u64,
    ) -> Result<()> {
        require!(
            ctx.accounts.admin.key() == ctx.accounts.token_state.admin,
            ErrorCode::Unauthorized
        );

        require!(
            ctx.accounts.rewards_pool.amount >= amount,
            ErrorCode::InsufficientBalance
        );

        transfer_within_program(
            &ctx.accounts.rewards_pool.to_account_info(),
            &ctx.accounts.swap_wallet.to_account_info(),
            &ctx.accounts.admin.to_account_info(),
            &ctx.accounts.token_program,
            amount,
        )?;

        emit!(SwapEvent {
            amount,
            timestamp: Clock::get()?.unix_timestamp,
            pool: ctx.accounts.rewards_pool.key(),
        });

        Ok(())
    }

    pub fn transfer_ownership(
        ctx: Context<TransferOwnership>,
        new_admin: Pubkey,
    ) -> Result<()> {
        require!(
            ctx.accounts.admin.key() == ctx.accounts.token_state.admin,
            ErrorCode::Unauthorized
        );

        require!(new_admin != Pubkey::default(), ErrorCode::InvalidAdminAddress);

        ctx.accounts.token_state.admin = new_admin;
        Ok(())
    }

    pub fn user_burn(
        ctx: Context<UserBurn>,
        amount: u64,
    ) -> Result<()> {
        require!(
            ctx.accounts.user_token_account.amount >= amount,
            ErrorCode::InsufficientBalance
        );

        burn_tokens(
            &ctx.accounts.mint,
            &ctx.accounts.user_token_account,
            &ctx.accounts.user.to_account_info(),
            &ctx.accounts.token_program,
            amount,
        )?;

        ctx.accounts.token_state.total_supply = ctx.accounts.token_state.total_supply
            .checked_sub(amount)
            .ok_or(ErrorCode::ArithmeticOverflow)?;
        ctx.accounts.token_state.total_burned += amount;

        emit!(BurnEvent {
            burner: ctx.accounts.user.key(),
            amount,
            timestamp: Clock::get()?.unix_timestamp,
            new_total_supply: ctx.accounts.token_state.total_supply,
        });

        Ok(())
    }

    pub fn freeze_contract(
        ctx: Context<FreezeContract>,
        freeze: bool,
    ) -> Result<()> {
        require!(
            ctx.accounts.admin.key() == ctx.accounts.token_state.admin,
            ErrorCode::Unauthorized
        );

        ctx.accounts.token_state.is_frozen = freeze;
        Ok(())
    }

    pub fn update_tvl_data(ctx: Context<UpdateTVL>) -> Result<()> {
        require!(
            ctx.accounts.admin.key() == ctx.accounts.token_state.admin,
            ErrorCode::Unauthorized
        );

        ctx.accounts.token_state.update_liquidity_pool(
            ctx.accounts.lp_pool.amount
        )?;
        ctx.accounts.token_state.update_rewards_pool(
            ctx.accounts.rewards_pool.amount
        )?;
        ctx.accounts.token_state.update_staking_pool(
            ctx.accounts.staking_pool.amount
        )?;

        Ok(())
    }
}

// =====================
// Core Implementation
// =====================

/// Token state management
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
}

impl TokenState {
    pub const SIZE: usize = 32 + (8 * 8) + 1; // Pubkey + 8 numeric fields + 1 bool

    pub fn initialize(
        &mut self,
        admin: Pubkey,
        supply: u64,
        launch_time: i64,
    ) {
        self.admin = admin;
        self.total_supply = supply;
        self.launch_time = launch_time;
        self.reward_distribution_start_time = launch_time + 2520; // 42 minutes
        self.total_transactions = 0;
        self.total_tax_collected = 0;
        self.total_burned = 0;
        self.last_transfer_timestamp = 0;
        self.last_transfer_amount = 0;
        self.is_frozen = false;
    }
}

// =====================
// Modular Components
// =====================

/// Mint tokens to specified account
#[allow(dead_code)]
fn mint_tokens<'info>(
    mint: &AccountInfo<'info>,
    recipient: &AccountInfo<'info>,
    authority: &AccountInfo<'info>,
    token_program: &Program<'info, Token>,
    amount: u64,
) -> Result<()> {
    let cpi_accounts = MintTo {
        mint: mint.clone(),
        to: recipient.clone(),
        authority: authority.clone(),
    };
    let cpi_ctx = CpiContext::new(
        token_program.to_account_info(),
        cpi_accounts,
    );
    anchor_spl::token::mint_to(cpi_ctx, amount)
}

/// Burn tokens from specified account
fn burn_tokens<'info>(
    mint: &Account<'info, Mint>,
    account: &Account<'info, TokenAccount>,
    authority: &AccountInfo<'info>,
    token_program: &Program<'info, Token>,
    amount: u64,
) -> Result<()> {
    let cpi_accounts = Burn {
        mint: mint.to_account_info(),
        from: account.to_account_info(),
        authority: authority.clone(),
    };
    anchor_spl::token::burn(
        CpiContext::new(
            token_program.to_account_info(),
            cpi_accounts,
        ),
        amount,
    )
}

/// Calculate transfer amounts with tax
fn calculate_transfer_amounts(
    whitelist: &Account<'_, Whitelist>,
    receiver: &Pubkey,
    amount: u64,
) -> Result<(u64, u64)> {
    let is_whitelisted = whitelist.wallets.contains(receiver);
    let tax_amount = if is_whitelisted { 0 } else {
        amount
            .checked_mul(10)
            .ok_or(ErrorCode::ArithmeticOverflow)?
            .checked_div(100)
            .ok_or(ErrorCode::ArithmeticOverflow)?
    };
    let net_amount = amount
        .checked_sub(tax_amount)
        .ok_or(ErrorCode::ArithmeticUnderflow)?;
    Ok((net_amount, tax_amount))
}

/// Handle tax allocation
fn allocate_tax(
    mut ctx: AllocateTaxAccounts,
    tax_amount: u64,
) -> Result<()> {
    // Calculate allocations
    let rewards = tax_amount
        .checked_mul(7)
        .ok_or(ErrorCode::ArithmeticOverflow)?
        .checked_div(10)
        .ok_or(ErrorCode::ArithmeticOverflow)?;
    let lp_fund = tax_amount
        .checked_mul(2)
        .ok_or(ErrorCode::ArithmeticOverflow)?
        .checked_div(10)
        .ok_or(ErrorCode::ArithmeticOverflow)?;
    let burn_amount = tax_amount
        .checked_sub(rewards)
        .ok_or(ErrorCode::ArithmeticOverflow)?
        .checked_sub(lp_fund)
        .ok_or(ErrorCode::ArithmeticOverflow)?;

    // Distribute to rewards pool
    transfer_within_program(
        &ctx.sender.to_account_info(),
        &ctx.rewards_pool.to_account_info(),
        &ctx.sender_authority.to_account_info(),
        &ctx.token_program,
        rewards,
    )?;

    // Add to LP fund
    transfer_within_program(
        &ctx.sender.to_account_info(),
        &ctx.lp_fund.to_account_info(),
        &ctx.sender_authority.to_account_info(),
        &ctx.token_program,
        lp_fund,
    )?;

    // Burn portion
    burn_tokens(
        &ctx.mint,
        &ctx.sender,
        &ctx.sender_authority,
        &ctx.token_program,
        burn_amount,
    )?;

    // Update state
    ctx.token_state.total_tax_collected += tax_amount;
    ctx.token_state.total_burned += burn_amount;
    Ok(())
}

/// Transfer tokens within the program
fn transfer_within_program<'info>(
    from: &AccountInfo<'info>,
    to: &AccountInfo<'info>,
    authority: &AccountInfo<'info>,
    token_program: &Program<'info, Token>,
    amount: u64,
) -> Result<()> {
    let cpi_accounts = Transfer {
        from: from.clone(),
        to: to.clone(),
        authority: authority.clone(),
    };
    let cpi_ctx = CpiContext::new(
        token_program.to_account_info(),
        cpi_accounts,
    );
    anchor_spl::token::transfer(cpi_ctx, amount)
}

/// Add wallet to whitelist
fn add_wallet(
    whitelist: &mut Account<'_, Whitelist>,
    wallet: Pubkey,
) -> Result<()> {
    require!(whitelist.wallets.len() < 15, ErrorCode::WhitelistFull);
    whitelist.wallets.push(wallet);
    Ok(())
}

/// Remove wallet from whitelist
fn remove_wallet(
    whitelist: &mut Account<'_, Whitelist>,
    wallet: Pubkey,
) -> Result<()> {
    let index = whitelist.wallets.iter()
        .position(|&w| w == wallet)
        .ok_or(ErrorCode::NotInWhitelist)?;
    whitelist.wallets.remove(index);
    Ok(())
}

/// Distribute initial supply according to tokenomics
fn distribute_initial_supply<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, InitializeToken<'info>>,
    total_supply: u64,
    whitelist_wallets: &Vec<Pubkey>,
) -> Result<()> {
    // Calculate allocations
    let lp_amount = total_supply
        .checked_mul(40)
        .ok_or(ErrorCode::ArithmeticOverflow)?
        .checked_div(100)
        .ok_or(ErrorCode::ArithmeticOverflow)?;
    let whitelist_amount = total_supply
        .checked_mul(15)
        .ok_or(ErrorCode::ArithmeticOverflow)?
        .checked_div(100)
        .ok_or(ErrorCode::ArithmeticOverflow)?;
    let burn_allocation = total_supply
        .checked_mul(30)
        .ok_or(ErrorCode::ArithmeticOverflow)?
        .checked_div(100)
        .ok_or(ErrorCode::ArithmeticOverflow)?;
    let marketing_amount = total_supply
        .checked_mul(15)
        .ok_or(ErrorCode::ArithmeticOverflow)?
        .checked_div(100)
        .ok_or(ErrorCode::ArithmeticOverflow)?;

    // Mint to liquidity pool
    let mint_cpi_accounts = MintTo {
        mint: ctx.accounts.mint.to_account_info(),
        to: ctx.accounts.lp_pool.to_account_info(),
        authority: ctx.accounts.mint_authority.to_account_info(),
    };
    anchor_spl::token::mint_to(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            mint_cpi_accounts,
        ),
        lp_amount,
    )?;

    // Mint to admin for burn allocation
    let mint_cpi_accounts = MintTo {
        mint: ctx.accounts.mint.to_account_info(),
        to: ctx.accounts.admin_token_account.to_account_info(),
        authority: ctx.accounts.mint_authority.to_account_info(),
    };
    anchor_spl::token::mint_to(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            mint_cpi_accounts,
        ),
        burn_allocation,
    )?;

    // Mint to admin for marketing
    let mint_cpi_accounts = MintTo {
        mint: ctx.accounts.mint.to_account_info(),
        to: ctx.accounts.admin_token_account.to_account_info(),
        authority: ctx.accounts.mint_authority.to_account_info(),
    };
    anchor_spl::token::mint_to(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            mint_cpi_accounts,
        ),
        marketing_amount,
    )?;

    // Calculate per-wallet whitelist amount
    let whitelist_per_wallet = whitelist_amount
        .checked_div(whitelist_wallets.len() as u64)
        .ok_or(ErrorCode::ArithmeticOverflow)?;

    // Convert remaining_accounts to an iterator
    let mut accounts_iter = ctx.remaining_accounts.iter();

    // Mint to each whitelisted wallet
    for wallet in whitelist_wallets {
        // Derive the ATA address
        let _ata_address = get_associated_token_address(wallet, &ctx.accounts.mint.key());

        // Get or create the ATA
        let recipient_token_account = next_account_info(&mut accounts_iter)?;
        get_or_create_associated_token_account(
            ctx.accounts.token_program.clone(),
            ctx.accounts.system_program.clone(),
            ctx.accounts.rent.clone(),
            &ctx.accounts.mint,
            *wallet,
            &ctx.accounts.admin,
            recipient_token_account,
        )?;

        let mint_cpi_accounts = MintTo {
            mint: ctx.accounts.mint.to_account_info(),
            to: recipient_token_account.clone(),
            authority: ctx.accounts.mint_authority.to_account_info(),
        };

        anchor_spl::token::mint_to(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                mint_cpi_accounts,
            ),
            whitelist_per_wallet,
        )?;
    }
    Ok(())
}

// Helper function to get or create associated token account
fn get_or_create_associated_token_account<'info>(                                                                                                                                                      
    token_program: Program<'info, Token>,                                                                                                                                                              
    system_program: Program<'info, System>,                                                                                                                                                            
    rent: Sysvar<'info, Rent>,                                                                                                                                                                         
    mint: &Account<'info, Mint>,                                                                                                                                                                       
    owner: Pubkey,                                                                                                                                                                                     
    payer: &Signer<'info>,                                                                                                                                                                             
    associated_token_account: &AccountInfo<'info>,                                                                                                                                                     
) -> Result<()> {                                                                                                                                                                                      
    // Derive the associated token account address                                                                                                                                                     
    let ata_address = get_associated_token_address(&owner, &mint.key());                                                                                                                               
                                                                                                                                                                                                       
    // Check if we need to create the account                                                                                                                                                          
    if associated_token_account.data_is_empty() {                                                                                                                                                      
        let create_ata_instruction = create_associated_token_account(                                                                                                                                  
            &payer.key(),                                                                                                                                                                              
            &owner,                                                                                                                                                                                    
            &mint.key(),                                                                                                                                                                               
            &spl_token::id(),                                                                                                                                                                          
        );                                                                                                                                                                                             
                                                                                                                                                                                                       
        invoke(                                                                                                                                                                                        
            &create_ata_instruction,                                                                                                                                                                   
            &[                                                                                                                                                                                         
                payer.to_account_info(),                                                                                                                                                               
                system_program.to_account_info(),                                                                                                                                                      
                rent.to_account_info(),                                                                                                                                                                
                token_program.to_account_info(),                                                                                                                                                       
                mint.to_account_info(),                                                                                                                                                                
                associated_token_account.clone(),                                                                                                                                                      
            ],                                                                                                                                                                                         
        )?;                                                                                                                                                                                            
    }                                                                                                                                                                                                  
                                                                                                                                                                                                       
    // Verify the account matches the expected address                                                                                                                                                 
    require_keys_eq!(                                                                                                                                                                                  
        associated_token_account.key(),                                                                                                                                                                
        ata_address,                                                                                                                                                                                   
        ErrorCode::AccountMismatch                                                                                                                                                                     
    );                                                                                                                                                                                                 
                                                                                                                                                                                                       
    Ok(())                                                                                                                                                                                             
}                                 

// =====================
// Account Structures
// =====================

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

    /// CHECK: This is the PDA that will be the mint authority, its seeds are checked in the program
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
    #[account(mut)]  // Add this field
    pub token_account: Account<'info, TokenAccount>,  // Add this field
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

    /// CHECK: This is the PDA that signs the transfer, validated by the program
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

// =====================
// Additional Components
// =====================

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

// =====================
// Whitelist Struct
// =====================

#[account]
pub struct Whitelist {
    pub wallets: Vec<Pubkey>, // 4 bytes for length + 15*32 bytes
}

impl Whitelist {
    pub const SIZE: usize = 4 + (15 * 32); // 4 bytes vec length + space for 15 Pubkeys
}
