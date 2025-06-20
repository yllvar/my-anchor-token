use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount, MintTo};
use solana_program::pubkey::Pubkey;
use solana_sdk::signer::Signer;
use solana_sdk::signature::Keypair;
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<()> {
    // Load the provider
    let provider = anchor_client::ClientProvider::new_cluster_and_wallet(anchor_client::Cluster::Devnet, "path/to/wallet.json").await?;
    let client = anchor_client::Client::new(provider);

    // Define the program ID and the LP address
    let program_id = Pubkey::from_str("EQ85HBoFJ6FiLz5NLZSuLnJ2Wr71q3P27rggw1z2WYAY")?;
    let lp_address = Pubkey::from_str("<LP_ADDRESS>")?;

    // Define the mint and admin keypairs
    let mint_keypair = Keypair::from_bytes(&[/* your mint keypair bytes */])?;
    let admin_keypair = Keypair::from_bytes(&[/* your admin keypair bytes */])?;

    // Create the mint to LP pool instruction
    let cpi_accounts = MintTo {
        mint: mint_keypair.pubkey(),
        to: lp_address,
        authority: admin_keypair.pubkey(),
    };
    let cpi_ctx = anchor_lang::context::CpiContext::new(
        client.program(program_id).account("token_program").unwrap().to_account_info(),
        cpi_accounts,
    );

    // Mint tokens to the LP pool
    anchor_spl::token::mint_to(cpi_ctx, <TOKEN_AMOUNT>)?;

    println!("Successfully minted tokens to LP pool");

    Ok(())
}