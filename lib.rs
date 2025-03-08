fn get_or_create_associated_token_account<'info>(
    token_program: AccountInfo<'info>,
    system_program: AccountInfo<'info>,
    rent: AccountInfo<'info>,
    mint: &Account<'info, Mint>,
    owner: Pubkey,
    payer: &Signer<'info>,
    associated_token_account: &Account<'info, TokenAccount>,
) -> Result<()> {
    // Check if the associated token account is initialized
    if associated_token_account.amount == 0 && associated_token_account.owner == Pubkey::default() {
        // Create the associated token account
        let create_ata_instruction = create_associated_token_account(
            &payer.key(),
            &owner,
            &mint.key(),
            &spl_token::id(),
        );

        // Invoke the instruction to create the associated token account
        invoke(
            &create_ata_instruction,
            &[
                payer.to_account_info(),
                system_program.to_account_info(),
                rent.to_account_info(),
                token_program.to_account_info(),
                mint.to_account_info(),
                associated_token_account.to_account_info(),
            ],
        )?;
    }

    Ok(())
}
