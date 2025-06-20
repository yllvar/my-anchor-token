import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { MySolamiToken } from "../target/types/my_solami_token";
import { Keypair, PublicKey } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID, ASSOCIATED_TOKEN_PROGRAM_ID, createMint, getOrCreateAssociatedTokenAccount, mintTo } from "@solana/spl-token";
import assert from "assert";

describe("my_solami_token", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());

  const program = anchor.workspace.MySolamiToken as Program<MySolamiToken>;
  const provider = anchor.getProvider();

  // Initialize keypairs and accounts
  const admin = anchor.web3.Keypair.generate();
  const user1 = anchor.web3.Keypair.generate();
  const user2 = anchor.web3.Keypair.generate();
  const whitelistWallet1 = anchor.web3.Keypair.generate();
  const whitelistWallet2 = anchor.web3.Keypair.generate();

  let mint: PublicKey;
  let adminTokenAccount: PublicKey;
  let lpPool: PublicKey;
  let rewardsPool: PublicKey;
  let lpFund: PublicKey;
  let whitelist: PublicKey;
  let tokenState: PublicKey;

  before(async () => {
    // Airdrop SOL to admin and users
    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(admin.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL),
      "confirmed"
    );

    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(user1.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL),
      "confirmed"
    );

    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(user2.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL),
      "confirmed"
    );

    // Create mint
    mint = await createMint(
      provider.connection,
      admin,
      admin.publicKey,
      null,
      6,
      TOKEN_PROGRAM_ID
    );

    // Create associated token accounts
    adminTokenAccount = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      admin,
      mint,
      admin.publicKey,
      true,
      "finalized",
      undefined,
      TOKEN_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID
    ).then((account) => account.address);

    lpPool = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      admin,
      mint,
      admin.publicKey,
      true,
      "finalized",
      undefined,
      TOKEN_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID
    ).then((account) => account.address);

    rewardsPool = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      admin,
      mint,
      admin.publicKey,
      true,
      "finalized",
      undefined,
      TOKEN_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID
    ).then((account) => account.address);

    lpFund = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      admin,
      mint,
      admin.publicKey,
      true,
      "finalized",
      undefined,
      TOKEN_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID
    ).then((account) => account.address);

    // Derive PDA accounts
    [tokenState] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("token_state"), mint.toBuffer()],
      program.programId
    );

    [whitelist] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("whitelist"), mint.toBuffer()],
      program.programId
    );
  });

  it("Initializes the token with distribution and burning setup", async () => {
    const totalSupply = 1000000;
    const whitelistWallets = [whitelistWallet1.publicKey, whitelistWallet2.publicKey];

    const tx = await program.methods
      .initializeToken(totalSupply, whitelistWallets)
      .accounts({
        tokenState: tokenState,
        whitelist: whitelist,
        mintAuthority: admin.publicKey, // Use the admin Keypair's publicKey
        mint: mint,
        admin: admin.publicKey, // Use the admin Keypair's publicKey
        adminTokenAccount: adminTokenAccount,
        lpPool: lpPool,
        rewardsPool: rewardsPool,
        lpFund: lpFund,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .signers([admin]) // Use the admin Keypair
      .rpc();

    console.log("Initialized token with signature:", tx);

    // Verify token state
    const tokenStateAccount = await program.account.tokenState.fetch(tokenState);
    assert.strictEqual(tokenStateAccount.totalSupply, totalSupply);
    assert.strictEqual(tokenStateAccount.admin.toString(), admin.publicKey.toString());
    assert.strictEqual(tokenStateAccount.totalTransactions, 0);
    assert.strictEqual(tokenStateAccount.totalTaxCollected, 0);
    assert.strictEqual(tokenStateAccount.totalBurned, 0);
    assert.strictEqual(tokenStateAccount.isFrozen, false);

    // Verify whitelist
    const whitelistAccount = await program.account.whitelist.fetch(whitelist);
    assert.strictEqual(whitelistAccount.wallets.length, 2);
    assert(whitelistAccount.wallets.some((wallet: PublicKey) => wallet.equals(whitelistWallet1.publicKey)));
    assert(whitelistAccount.wallets.some((wallet: PublicKey) => wallet.equals(whitelistWallet2.publicKey)));

    // Verify token balances
    const adminTokenBalance = await provider.connection.getTokenAccountBalance(adminTokenAccount);
    assert.strictEqual(adminTokenBalance.value.uiAmount, 500000); // 30% burn + 15% marketing

    const lpPoolBalance = await provider.connection.getTokenAccountBalance(lpPool);
    assert.strictEqual(lpPoolBalance.value.uiAmount, 400000); // 40% LP

    const whitelistWallet1Balance = await provider.connection.getTokenAccountBalance(
      await getOrCreateAssociatedTokenAccount(
        provider.connection,
        admin,
        mint,
        whitelistWallet1.publicKey,
        true,
        "finalized",
        undefined,
        TOKEN_PROGRAM_ID,
        ASSOCIATED_TOKEN_PROGRAM_ID
      ).then((account) => account.address)
    );
    assert.strictEqual(whitelistWallet1Balance.value.uiAmount, 11250); // 15% / 2

    const whitelistWallet2Balance = await provider.connection.getTokenAccountBalance(
      await getOrCreateAssociatedTokenAccount(
        provider.connection,
        admin,
        mint,
        whitelistWallet2.publicKey,
        true,
        "finalized",
        undefined,
        TOKEN_PROGRAM_ID,
        ASSOCIATED_TOKEN_PROGRAM_ID
      ).then((account) => account.address)
    );
    assert.strictEqual(whitelistWallet2Balance.value.uiAmount, 11250); // 15% / 2
  });

  it("Transfers tokens between users", async () => {
    // Mint some tokens to user1
    await mintTo(
      provider.connection,
      admin,
      mint,
      adminTokenAccount,
      admin,
      100000,
      [],
      TOKEN_PROGRAM_ID
    );

    // Get user1's token account
    const user1TokenAccount = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      admin,
      mint,
      user1.publicKey,
      true,
      "finalized",
      undefined,
      TOKEN_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID
    ).then((account) => account.address);

    // Transfer tokens from admin to user1
    await program.methods
      .transferTokens(100000)
      .accounts({
        tokenState: tokenState,
        sender: adminTokenAccount,
        receiver: user1TokenAccount,
        rewardsPool: rewardsPool,
        lpFund: lpFund,
        senderAuthority: admin.publicKey, // Use the admin Keypair's publicKey
        mint: mint,
        whitelist: whitelist,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([admin]) // Use the admin Keypair
      .rpc();

    // Verify balances
    const adminTokenBalance = await provider.connection.getTokenAccountBalance(adminTokenAccount);
    assert.strictEqual(adminTokenBalance.value.uiAmount, 400000); // 500000 - 100000

    const user1TokenBalance = await provider.connection.getTokenAccountBalance(user1TokenAccount);
    assert.strictEqual(user1TokenBalance.value.uiAmount, 100000);

    // Transfer tokens from user1 to user2
    const user2TokenAccount = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      admin,
      mint,
      user2.publicKey,
      true,
      "finalized",
      undefined,
      TOKEN_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID
    ).then((account) => account.address);

    await program.methods
      .transferTokens(50000)
      .accounts({
        tokenState: tokenState,
        sender: user1TokenAccount,
        receiver: user2TokenAccount,
        rewardsPool: rewardsPool,
        lpFund: lpFund,
        senderAuthority: user1.publicKey, // Use the user1 Keypair's publicKey
        mint: mint,
        whitelist: whitelist,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([user1]) // Use the user1 Keypair
      .rpc();

    // Verify balances after transfer
    const user1TokenBalanceAfter = await provider.connection.getTokenAccountBalance(user1TokenAccount);
    assert.strictEqual(user1TokenBalanceAfter.value.uiAmount, 50000); // 100000 - 50000

    const user2TokenBalanceAfter = await provider.connection.getTokenAccountBalance(user2TokenAccount);
    assert.strictEqual(user2TokenBalanceAfter.value.uiAmount, 50000);

    // Verify tax distribution
    const rewardsPoolBalance = await provider.connection.getTokenAccountBalance(rewardsPool);
    assert.strictEqual(rewardsPoolBalance.value.uiAmount, 3500); // 50000 * 10% * 70%

    const lpFundBalance = await provider.connection.getTokenAccountBalance(lpFund);
    assert.strictEqual(lpFundBalance.value.uiAmount, 1000); // 50000 * 10% * 20%

    const tokenStateAccount = await program.account.tokenState.fetch(tokenState);
    assert.strictEqual(tokenStateAccount.totalTaxCollected, 4500); // 3500 + 1000
    assert.strictEqual(tokenStateAccount.totalTransactions, 2);
  });

  it("Adds and removes wallets from whitelist", async () => {
    const newWhitelistWallet = anchor.web3.Keypair.generate();

    // Add new whitelist wallet
    await program.methods
      .addToWhitelist(newWhitelistWallet.publicKey)
      .accounts({
        tokenState: tokenState,
        whitelist: whitelist,
        admin: admin.publicKey, // Use the admin Keypair's publicKey
      })
      .signers([admin]) // Use the admin Keypair
      .rpc();

    // Verify whitelist
    const whitelistAccount = await program.account.whitelist.fetch(whitelist);
    assert.strictEqual(whitelistAccount.wallets.length, 3);
    assert(whitelistAccount.wallets.some((wallet: PublicKey) => wallet.equals(newWhitelistWallet.publicKey)));

    // Remove whitelist wallet
    await program.methods
      .removeFromWhitelist(newWhitelistWallet.publicKey)
      .accounts({
        tokenState: tokenState,
        whitelist: whitelist,
        admin: admin.publicKey, // Use the admin Keypair's publicKey
      })
      .signers([admin]) // Use the admin Keypair
      .rpc();

    // Verify whitelist after removal
    const whitelistAccountAfter = await program.account.whitelist.fetch(whitelist);
    assert.strictEqual(whitelistAccountAfter.wallets.length, 2);
    assert(!whitelistAccountAfter.wallets.some((wallet: PublicKey) => wallet.equals(newWhitelistWallet.publicKey)));
  });

  it("Burns tokens manually", async () => {
    // Mint some tokens to admin
    await mintTo(
      provider.connection,
      admin,
      mint,
      adminTokenAccount,
      admin,
      100000,
      [],
      TOKEN_PROGRAM_ID
    );

    // Burn tokens manually
    await program.methods
      .manualBurn(50000)
      .accounts({
        mint: mint,
        adminTokenAccount: adminTokenAccount,
        admin: admin.publicKey, // Use the admin Keypair's publicKey
        tokenProgram: TOKEN_PROGRAM_ID,
        tokenState: tokenState,
      })
      .signers([admin]) // Use the admin Keypair
      .rpc();

    // Verify balances
    const adminTokenBalance = await provider.connection.getTokenAccountBalance(adminTokenAccount);
    assert.strictEqual(adminTokenBalance.value.uiAmount, 450000); // 500000 - 50000

    // Verify token state
    const tokenStateAccount = await program.account.tokenState.fetch(tokenState);
    assert.strictEqual(tokenStateAccount.totalSupply, 950000); // 1000000 - 50000
    assert.strictEqual(tokenStateAccount.totalBurned, 50000);
  });

  it("Prepares rewards swap", async () => {
    // Prepare rewards swap
    await program.methods
      .prepareRewardsSwap(3500)
      .accounts({
        rewardsPool: rewardsPool,
        swapWallet: adminTokenAccount, // Use admin token account for testing
        admin: admin.publicKey, // Use the admin Keypair's publicKey
        tokenProgram: TOKEN_PROGRAM_ID,
        tokenState: tokenState,
        mint: mint,
      })
      .signers([admin]) // Use the admin Keypair
      .rpc();

    // Verify balances
    const rewardsPoolBalance = await provider.connection.getTokenAccountBalance(rewardsPool);
    assert.strictEqual(rewardsPoolBalance.value.uiAmount, 0); // 3500 - 3500

    const adminTokenBalance = await provider.connection.getTokenAccountBalance(adminTokenAccount);
    assert.strictEqual(adminTokenBalance.value.uiAmount, 453500); // 450000 + 3500
  });

  it("Transfers ownership", async () => {
    const newAdmin = anchor.web3.Keypair.generate();

    // Transfer ownership
    await program.methods
      .transferOwnership(newAdmin.publicKey)
      .accounts({
        tokenState: tokenState,
        admin: admin.publicKey, // Use the admin Keypair's publicKey
      })
      .signers([admin]) // Use the admin Keypair
      .rpc();

    // Verify token state
    const tokenStateAccount = await program.account.tokenState.fetch(tokenState);
    assert.strictEqual(tokenStateAccount.admin.toString(), newAdmin.publicKey.toString());
  });

  it("Freezes and unfreezes the contract", async () => {
    // Freeze contract
    await program.methods
      .freezeContract(true)
      .accounts({
        tokenState: tokenState,
        admin: admin.publicKey, // Use the admin Keypair's publicKey
      })
      .signers([admin]) // Use the admin Keypair
      .rpc();

    // Verify token state
    let tokenStateAccount = await program.account.tokenState.fetch(tokenState);
    assert.strictEqual(tokenStateAccount.isFrozen, true);

    // Attempt to transfer tokens (should fail)
    try {
      await program.methods
        .transferTokens(10000)
        .accounts({
          tokenState: tokenState,
          sender: adminTokenAccount,
          receiver: user1.publicKey,
          rewardsPool: rewardsPool,
          lpFund: lpFund,
          senderAuthority: admin.publicKey, // Use the admin Keypair's publicKey
          mint: mint,
          whitelist: whitelist,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([admin]) // Use the admin Keypair
        .rpc();
    } catch (err) {
      assert.strictEqual(err.error.errorCode.code, "ContractFrozen");
    }

    // Unfreeze contract
    await program.methods
      .freezeContract(false)
      .accounts({
        tokenState: tokenState,
        admin: admin.publicKey, // Use the admin Keypair's publicKey
      })
      .signers([admin]) // Use the admin Keypair
      .rpc();

    // Verify token state
    tokenStateAccount = await program.account.tokenState.fetch(tokenState);
    assert.strictEqual(tokenStateAccount.isFrozen, false);

    // Transfer tokens again (should succeed)
    await program.methods
      .transferTokens(10000)
      .accounts({
        tokenState: tokenState,
        sender: adminTokenAccount,
        receiver: user1.publicKey,
        rewardsPool: rewardsPool,
        lpFund: lpFund,
        senderAuthority: admin.publicKey, // Use the admin Keypair's publicKey
        mint: mint,
        whitelist: whitelist,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([admin]) // Use the admin Keypair
      .rpc();

    // Verify balances
    const adminTokenBalance = await provider.connection.getTokenAccountBalance(adminTokenAccount);
    assert.strictEqual(adminTokenBalance.value.uiAmount, 443500); // 453500 - 10000

    const user1TokenBalance = await provider.connection.getTokenAccountBalance(user1.publicKey);
    assert.strictEqual(user1TokenBalance.value.uiAmount, 60000); // 50000 + 10000
  });
});