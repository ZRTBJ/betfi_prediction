use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Timestamp, Uint128};

use crate::state::BetInfo;

#[cw_serde]
pub struct InstantiateMsg {
    pub config: Config,
}

#[cw_serde]
pub enum ExecuteMsg {
    /**
     * Update part of or all of the mutable config params
     */
    UpdateConfig {
        config: Config,
    },
    /**
     * Price go up
     */
    BetBull {
        /* In case the TX is delayed */
        round_id: Uint128,
        amount: Uint128,
    },
    /**
     * Price go down
     */
    BetBear {
        /* In case the TX is delayed */
        round_id: Uint128,
        amount: Uint128,
    },
    /**
     * Permissionless msg to close the current round and open the next
     * NOTE It is permissionless because we can check timestamps :)
     */
    CloseRound {},
    /**
     * Settle winnings for an account
     */
    CollectWinnings {},
    Hault {},
    Resume {},
}
#[cw_serde]
pub enum QueryMsg {
    Config {},
    Status {},
    MyCurrentPosition {
        address: String,
    },
    FinishedRound {
        round_id: Uint128,
    },
    MyGameList {
        player: Addr,
        start_after: Option<Uint128>,
        limit: Option<u32>,
    },
    ReverseMyGameList {
        player: Addr,
        start_after: Option<Uint128>,
        limit: Option<u32>,
    },
    MyPendingReward {
        player: Addr,
    },
}

#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
pub enum Direction {
    Bull,
    Bear,
}

impl ToString for Direction {
    fn to_string(&self) -> String {
        match self {
            Direction::Bull => "bull",
            Direction::Bear => "bear",
        }
        .to_string()
    }
}

pub type ConfigResponse = Config;
pub type RoundResponse = FinishedRound;

#[cw_serde]
pub struct StatusResponse {
    pub bidding_round: Option<NextRound>,
    pub live_round: Option<LiveRound>,
    pub total_volume: Uint128,
    pub current_time: u64,
    pub finished_round: FinishedRound,
}

#[cw_serde]
pub struct MyCurrentPositionResponse {
    pub live_bear_amount: Uint128,
    pub live_bull_amount: Uint128,
    pub next_bear_amount: Uint128,
    pub next_bull_amount: Uint128,
}

#[cw_serde]
/**
 * Parameters which are mutable by a governance vote
 */
pub struct Config {
    /* After a round ends this is the duration of the next */
    pub next_round_seconds: Uint128,
    pub fast_oracle_addr: Addr,
    pub minimum_bet: Uint128,
    pub burn_fee: Uint128,
    pub gaming_fee: Uint128,
    pub token_addr: Addr,
}
#[cw_serde]
pub struct NextRound {
    pub id: Uint128,
    pub bid_time: Timestamp,
    pub open_time: Timestamp,
    pub close_time: Timestamp,
    pub bull_amount: Uint128,
    pub bear_amount: Uint128,
}

#[cw_serde]
pub struct LiveRound {
    pub id: Uint128,
    pub bid_time: Timestamp,
    pub open_time: Timestamp,
    pub close_time: Timestamp,
    pub open_price: Uint128,
    pub bull_amount: Uint128,
    pub bear_amount: Uint128,
}

#[cw_serde]
pub struct FinishedRound {
    pub id: Uint128,
    pub bid_time: Timestamp,
    pub open_time: Timestamp,
    pub close_time: Timestamp,
    pub open_price: Uint128,
    pub close_price: Uint128,
    pub winner: Option<Direction>,
    pub bull_amount: Uint128,
    pub bear_amount: Uint128,
}

#[cw_serde]
pub enum FastOracleQueryMsg {
    Price {},
}

#[cw_serde]
pub enum FastOracleExecuteMsg {
    Update { price: Uint128 },
    Owner { owner: Addr },
}

#[cw_serde]
pub struct FastOracleInstantiateMsg {}
