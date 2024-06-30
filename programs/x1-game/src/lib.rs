use anchor_lang::{
    prelude::*,
};
use anchor_spl::{
    token::{Token, Mint, MintTo, TokenAccount},
    // metadata::{create_metadata_accounts_v3, CreateMetadataAccountsV3, Metadata, mpl_token_metadata},
    token::{mint_to},
    associated_token::AssociatedToken,
};
// use mpl_token_metadata::{types::DataV2};

declare_id!("hZQJ9V29g9Mx9x47QRh4eRCKShrhbfnuz8DG4N7u4Sr");

const SLOTS_PER_ROUND: u16 = 256;
const ROUND_CAP_START: u128 = 10_000_000;
const ROUND_CAP_DEC: u128 = 100;
const POINTS_BASE: u128 = 100;
const USER_SCORE_BASE_INC: u64 = 1;

#[program]
pub mod x1_game {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, _params: InitTokenParams) -> Result<()> {
        let slot = Clock::get().unwrap().slot;

        ctx.accounts.x1_game_state.current_round = 1;
        ctx.accounts.x1_game_state.last_round_slot = slot;
        Ok(())
    }

    pub fn click(ctx: Context<Click>, round: u64) -> Result<()> {
        let slot = Clock::get().unwrap().slot;
        require!(slot > ctx.accounts.x1_game_state.last_round_slot, X1GameError::BadSlotValue);

        // check if new round has to be started
        let round_inc = (slot - ctx.accounts.x1_game_state.last_round_slot) / SLOTS_PER_ROUND as u64;
        if round_inc > 0 && ROUND_CAP_START > ROUND_CAP_DEC * ctx.accounts.x1_game_state.current_round as u128 {
            ctx.accounts.x1_game_state.current_round += round_inc;
            ctx.accounts.x1_game_state.last_round_slot = slot;

            // calculate and assign round's cap
            ctx.accounts.x1_round_state.cap = ROUND_CAP_START - ROUND_CAP_DEC * ctx.accounts.x1_game_state.current_round as u128;
        }

        // check if correct round was submitted with tx
        require!(round == ctx.accounts.x1_game_state.current_round, X1GameError::WrongRound);

        // calculate # of missed rounds and increase/decrease user score accordingly
        let missed_rounds = round - ctx.accounts.x1_user_state.last_round;
        if missed_rounds < 2 {
            ctx.accounts.x1_user_state.score += USER_SCORE_BASE_INC
        } else if missed_rounds > 20 {
            ctx.accounts.x1_user_state.score = 0
        } else if missed_rounds > 10 {
            if ctx.accounts.x1_user_state.score >= USER_SCORE_BASE_INC {
                ctx.accounts.x1_user_state.score -= USER_SCORE_BASE_INC
            } else {
                ctx.accounts.x1_user_state.score = 0
            }
        }

        // calculate user points and adjust counters
        let new_points = POINTS_BASE * (ctx.accounts.x1_user_state.score + 1) as u128;
        ctx.accounts.x1_game_state.total_points += new_points;
        ctx.accounts.x1_round_state.total_points += new_points;
        ctx.accounts.x1_user_round_state.points += new_points;
        ctx.accounts.x1_user_state.user_points += new_points;

        ctx.accounts.x1_user_state.last_round = round;

        Ok(())
    }

    pub fn mint(ctx: Context<MintTokens>, round: u64) -> Result<()> {
        require!(round < ctx.accounts.x1_game_state.current_round, X1GameError::WrongRound);

        let (round_state_pda, _bump_round) =
            Pubkey::find_program_address(&[
                b"x1-game-round",
                round.to_le_bytes().as_slice(),
            ], &ctx.program_id);
        require!(round_state_pda == ctx.accounts.x1_round_state.key(), X1GameError::BadParam);

        let (user_round_state_pda, _bump_user_round) =
            Pubkey::find_program_address(&[
                b"x1-user-round",
                round.to_le_bytes().as_slice(),
                ctx.accounts.user.key.to_bytes().as_slice()
            ], &ctx.program_id);
        require!(user_round_state_pda == ctx.accounts.x1_user_round_state.key(), X1GameError::BadParam);

        let (user_state_pda, _bump_user) =
            Pubkey::find_program_address(&[
                b"x1-game-user",
                ctx.accounts.user.key.to_bytes().as_slice()
            ], &ctx.program_id);
        require!(user_state_pda == ctx.accounts.x1_user_state.key(), X1GameError::BadParam);

        // check if there's anything to be minted
        if ctx.accounts.x1_user_round_state.points > 0 && ctx.accounts.x1_round_state.total_points > 0 {
            // check if points haven't been minted yet
            require!(ctx.accounts.x1_user_round_state.minted_points == 0, X1GameError::AlreadyMinted);
            // calc number of tokens to mint
            let tokens_to_mint = ctx.accounts.x1_round_state.cap * ctx.accounts.x1_user_round_state.points / ctx.accounts.x1_round_state.total_points;
            // mint tokens
            let token_account_seeds: &[&[&[u8]]] = &[&[b"x1-game-mint", &[ctx.bumps.mint_account]]];
            mint_to(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    MintTo {
                        mint: ctx.accounts.mint_account.to_account_info(),
                        authority: ctx.accounts.mint_account.to_account_info(),
                        to: ctx.accounts.user_token_account.to_account_info(),
                    },
                    token_account_seeds
                ), // using PDA to sign
                tokens_to_mint as u64,
            )?;
            // adjust minted_points
            ctx.accounts.x1_user_round_state.minted_points += ctx.accounts.x1_user_round_state.points;
        }

        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(_params: InitTokenParams)]
pub struct Initialize<'info> {
    #[account(
        init_if_needed,
        seeds = [
            b"x1-game"
        ],
        payer = admin,
        space = 8 + X1GameState::INIT_SPACE,
        bump
    )]
    pub x1_game_state: Box<Account<'info, X1GameState>>,
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        init_if_needed,
        seeds = [b"x1-game-mint"],
        bump,
        payer = admin,
        mint::decimals = _params.decimals,
        mint::authority = mint_account.key(),
    )]
    pub mint_account: Box<Account<'info, Mint>>,
    /// CHECK: Address validated using constraint
    // #[account(mut)]
    // pub metadata: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token>,
    // pub token_metadata_program: Program<'info, Metadata>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>
}

#[account]
#[derive(InitSpace, Default)]
pub struct X1GameState {
    pub last_round_slot: u64,
    pub current_round: u64,
    pub total_points: u128
}

#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone)]
pub struct InitTokenParams {
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub decimals: u8,
}

#[derive(Accounts)]
#[instruction(round: u64)]
pub struct Click<'info> {
    #[account(mut)]
    pub x1_game_state: Box<Account<'info, X1GameState>>,
    #[account(
        init_if_needed,
        seeds = [
            b"x1-game-round",
            round.to_le_bytes().as_slice(),
        ],
        payer = user,
        space = 8 + X1RoundState::INIT_SPACE,
        bump
    )]
    pub x1_round_state: Box<Account<'info, X1RoundState>>,
    #[account(
        init_if_needed,
        seeds = [
            b"x1-user-round",
            round.to_le_bytes().as_slice(),
            user.key.to_bytes().as_slice()
        ],
        payer = user,
        space = 8 + X1UserRoundState::INIT_SPACE,
        bump
    )]
    pub x1_user_round_state: Box<Account<'info, X1UserRoundState>>,
    #[account(
        init_if_needed,
        seeds = [
            b"x1-game-user",
            user.key.to_bytes().as_slice()
        ],
        payer = user,
        space = 8 + X1UserState::INIT_SPACE,
        bump
    )]
    pub x1_user_state: Box<Account<'info, X1UserState>>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[account]
#[derive(InitSpace, Default)]
pub struct X1RoundState {
    pub cap: u128,
    pub total_points: u128
}

#[account]
#[derive(InitSpace, Default)]
pub struct X1UserRoundState {
    pub points: u128,
    pub minted_points: u128

}

#[account]
#[derive(InitSpace, Default)]
pub struct X1UserState {
    pub last_round: u64,
    pub score: u64,
    pub user_points: u128,
}

#[derive(Accounts)]
pub struct MintTokens<'info> {
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = mint_account,
        associated_token::authority = user,
    )]
    pub user_token_account: Box<Account<'info, TokenAccount>>,
    #[account(mut)]
    pub x1_game_state: Box<Account<'info, X1GameState>>,
    #[account(mut)]
    pub x1_round_state: Box<Account<'info, X1RoundState>>,
    #[account(mut)]
    pub x1_user_round_state: Box<Account<'info, X1UserRoundState>>,
    #[account(mut)]
    pub x1_user_state: Box<Account<'info, X1UserState>>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut, seeds = [b"x1-game-mint"], bump)]
    pub mint_account: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>
}

#[error_code]
pub enum X1GameError {
    #[msg("X1Game Mint has been already initialized")]
    MintIsAlreadyActive,
    #[msg("X1Game Mint has not yet started or is over")]
    MintIsNotActive,
    #[msg("Slot value is Out of Order")]
    BadSlotValue,
    #[msg("Bad param value")]
    BadParam,
    #[msg("Wrong round")]
    WrongRound,
    #[msg("Already minted")]
    AlreadyMinted,
}

