use thiserror::Error;

use cosmwasm_std::{StdError, Uint128};

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("InsufficientFunds")]
    InsufficientFunds {},

    #[error("You can not bet more than 19 points bets at the same time.")]
    ExceedBetPoints {},

    #[error("Incorrect native denom: provided: {provided}, required: {required}")]
    IncorrectNativeDenom { provided: String, required: String },

    #[error("This round is not started, so you can not close it")]
    RoundNotStarted {},

    #[error("This round is not finished, so you can not close it")]
    RoundNotFinished {},

    #[error("This round is finished, you can not bet")]
    RoundFinished {},

    #[error("You can withdraw ax maximum {withdrawal_amount} because of current user's betting reward for maximum case, now you are trying to withdraw {amount}")]
    WithdrawalMoneyExceeded {
        withdrawal_amount: Uint128,
        amount: Uint128,
    },

    #[error("It is less than the minimum bet amount")]
    InsufficientFundsForBet {},
}
