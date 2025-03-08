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
