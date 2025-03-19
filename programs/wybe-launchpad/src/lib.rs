use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, MintTo, Token, TokenAccount, Transfer};
use solana_program::{program::invoke, system_instruction};

declare_id!("8254Y8fZnZN6xsi6xGADpjUDrd78PeQCg6kbfzpRMYPS");

#[program]
pub mod wybe_launchpad {
    use super::*;

    // ======================
    // Initialize Project
    // ======================
    pub fn initialize_project(ctx: Context<InitializeProject>) -> Result<()> {
        ctx.accounts.project_state.is_community_owned = false;
        ctx.accounts.project_state.is_renounced = false;
        ctx.accounts.project_state.authority = *ctx.accounts.authority.key;
        Ok(())
    }

    // ======================
    // Mint Tokens with Fee
    // ======================
    pub fn mint_tokens(ctx: Context<MintTokens>, amount: u64) -> Result<()> {
        // Ensure the amount is exactly 1 billion tokens
        require!(amount == 1_000_000_000, ErrorCode::InvalidMintAmount);

        // Set fee to 0.01 SOL (in lamports)
        let fee = 10_000_000; // 0.01 SOL

        require!(
            ctx.accounts.user.lamports() >= fee,
            ErrorCode::InsufficientSOL
        );

        // Ensure the creator does not exceed 5% of the total supply
        let total_supply = ctx.accounts.mint.supply;
        let creator_balance = ctx.accounts.creator_token_account.amount;
        let max_allowed = total_supply.checked_div(20).ok_or(ErrorCode::CalculationOverflow)?; // 5% of total supply

        require!(
            creator_balance + amount <= max_allowed,
            ErrorCode::CreatorLimitExceeded
        );

        // SOL Transfer to Treasury
        invoke(
            &system_instruction::transfer(
                &ctx.accounts.user.key(),
                &ctx.accounts.treasury.key(),
                fee,
            ),
            &[
                ctx.accounts.user.to_account_info(),
                ctx.accounts.treasury.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;

        // Minting Tokens
        let seeds = &[b"mint_authority".as_ref(), &[ctx.bumps.mint_authority]];
        let signer_seeds = &[&seeds[..]];
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.user_token_account.to_account_info(),
                authority: ctx.accounts.mint_authority.to_account_info(),
            },
            signer_seeds,
        );
        token::mint_to(cpi_ctx, amount)?;

        Ok(())
    }

    // ======================
    // Apply Trading Fee
    // ======================
    pub fn trading_fee(ctx: Context<TradingFee>, amount: u64) -> Result<()> {
        let fee = amount
            .checked_div(100)
            .ok_or(ErrorCode::CalculationOverflow)?;

        let cpi_accounts = Transfer {
            from: ctx.accounts.user_token_account.to_account_info(),
            to: ctx.accounts.treasury_token_account.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
        );

        token::transfer(cpi_ctx, fee)?;

        // Check if the creator has sold all tokens
        if ctx.accounts.creator_token_account.amount == 0 {
            ctx.accounts.project_state.is_community_owned = true;
        }

        Ok(())
    }

    // ======================
    // Allocate Tokens to DEX
    // ======================
    pub fn allocate_dex(ctx: Context<AllocateDEX>, amount: u64) -> Result<()> {
        let allocation = amount
            .checked_div(100)
            .ok_or(ErrorCode::CalculationOverflow)?;

        let cpi_accounts = Transfer {
            from: ctx.accounts.creator_token_account.to_account_info(),
            to: ctx.accounts.treasury_token_account.to_account_info(),
            authority: ctx.accounts.creator.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
        );

        token::transfer(cpi_ctx, allocation)?;
        Ok(())
    }

    // ======================
    // Migrate Tokens to Raydium Pool
    // ======================
    pub fn migrate_to_raydium(ctx: Context<MigrateToRaydium>, amount: u64) -> Result<()> {
        let seeds = &[b"treasury".as_ref(), &[ctx.bumps.treasury]];
        let signer_seeds = &[&seeds[..]];
        let cpi_accounts = Transfer {
            from: ctx.accounts.treasury_token_account.to_account_info(),
            to: ctx.accounts.raydium_pool.to_account_info(),
            authority: ctx.accounts.treasury.to_account_info(),
        };

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        );

        token::transfer(cpi_ctx, amount)?;
        Ok(())
    }

    // ======================
    // Renounce Ownership
    // ======================
    pub fn renounce_ownership(ctx: Context<RenounceOwnership>) -> Result<()> {
        ctx.accounts.project_state.is_renounced = true;
        Ok(())
    }

    // ======================
    // Swap SOL to Tokens
    // ======================
    pub fn swap_sol_to_tokens(ctx: Context<SwapSolToTokens>, sol_amount: u64) -> Result<()> {
        // Transfer SOL from user to treasury
        invoke(
            &system_instruction::transfer(
                &ctx.accounts.user.key(),
                &ctx.accounts.treasury.key(),
                sol_amount,
            ),
            &[
                ctx.accounts.user.to_account_info(),
                ctx.accounts.treasury.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;

        // Mint tokens to user
        let seeds = &[b"mint_authority".as_ref(), &[ctx.bumps.mint_authority]];
        let signer_seeds = &[&seeds[..]];
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.user_token_account.to_account_info(),
                authority: ctx.accounts.mint_authority.to_account_info(),
            },
            signer_seeds,
        );
        token::mint_to(cpi_ctx, sol_amount * 100)?; // Example conversion rate

        Ok(())
    }
}

// ====================== ACCOUNT STRUCTS ======================

#[derive(Accounts)]
pub struct InitializeProject<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + 1 + 1 + 32,
        seeds = [b"project_state"],
        bump
    )]
    pub project_state: Account<'info, ProjectState>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct MintTokens<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut, constraint = user_token_account.mint == mint.key())]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub mint: Account<'info, Mint>,
    /// CHECK: This is a treasury account, verified by seeds.
    #[account(mut, seeds = [b"treasury"], bump)]
    pub treasury: UncheckedAccount<'info>,
    /// CHECK: This is the mint authority, verified by seeds.
    #[account(seeds = [b"mint_authority"], bump)]
    pub mint_authority: UncheckedAccount<'info>,
    #[account(mut)]
    pub creator_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct TradingFee<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut, constraint = user_token_account.mint == treasury_token_account.mint)]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub treasury_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub creator_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub project_state: Account<'info, ProjectState>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct AllocateDEX<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(mut, constraint = creator_token_account.mint == treasury_token_account.mint)]
    pub creator_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub treasury_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct MigrateToRaydium<'info> {
    #[account(mut, seeds = [b"treasury"], bump)]
    pub treasury: Signer<'info>,
    #[account(mut, constraint = treasury_token_account.owner == treasury.key())]
    pub treasury_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub raydium_pool: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct RenounceOwnership<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(mut)]
    pub project_state: Account<'info, ProjectState>,
}

#[derive(Accounts)]
pub struct SwapSolToTokens<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub mint: Account<'info, Mint>,
    /// CHECK: This is a treasury account, verified by seeds.
    #[account(mut, seeds = [b"treasury"], bump)]
    pub treasury: UncheckedAccount<'info>,
    /// CHECK: This is the mint authority, verified by seeds.
    #[account(seeds = [b"mint_authority"], bump)]
    pub mint_authority: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

// ====================== STATE STRUCTS ======================
#[account]
pub struct ProjectState {
    pub is_community_owned: bool,
    pub is_renounced: bool,
    pub authority: Pubkey,
}

// ====================== ERROR CODES ======================
#[error_code]
pub enum ErrorCode {
    #[msg("Insufficient SOL for minting fee.")]
    InsufficientSOL,
    #[msg("Arithmetic overflow/underflow occurred.")]
    CalculationOverflow,
    #[msg("Invalid mint amount. Must be exactly 1 billion tokens.")]
    InvalidMintAmount,
    #[msg("Creator cannot hold more than 5% of the total supply.")]
    CreatorLimitExceeded,
}
