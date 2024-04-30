use cosmwasm_std::{to_binary, Addr, Binary, Deps, Env, Order, StdResult, Timestamp, Uint128};
use cw_storage_plus::Bound;

use crate::{
    msg::{
        ConfigResponse, Direction, FinishedRound, MyCurrentPositionResponse, QueryMsg,
        StatusResponse,
    },
    state::{
        bet_info_key, bet_info_storage, MyGameResponse, PendingRewardResponse, CONFIG, LIVE_ROUND,
        NEXT_ROUND, NEXT_ROUND_ID, ROUNDS, TOTAL_VOLUME,
    },
};

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

// Query limits
const DEFAULT_QUERY_LIMIT: u32 = 10;
const MAX_QUERY_LIMIT: u32 = 30;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::Status {} => to_binary(&query_status(deps, env)?),
        QueryMsg::MyCurrentPosition { address } => {
            to_binary(&query_my_current_position(deps, address)?)
        }
        QueryMsg::FinishedRound { round_id } => to_binary(&query_finished_round(deps, round_id)?),
        QueryMsg::MyGameList {
            player,
            start_after,
            limit,
        } => to_binary(&query_my_games(deps, player, start_after, limit)?),
        QueryMsg::MyPendingReward { player } => to_binary(&query_my_pending_reward(deps, player)?),
        QueryMsg::ReverseMyGameList {
            player,
            start_after,
            limit,
        } => to_binary(&query_reverse_my_games(deps, player, start_after, limit)?),
    }
}

fn query_finished_round(deps: Deps, round_id: Uint128) -> StdResult<FinishedRound> {
    let round = ROUNDS.may_load(deps.storage, round_id.u128())?;
    match round {
        Some(round) => Ok(round),
        None => Ok(FinishedRound {
            id: Uint128::zero(),
            bid_time: Timestamp::from_nanos(0),
            open_time: Timestamp::from_nanos(0),
            close_time: Timestamp::from_nanos(0),
            open_price: Uint128::zero(),
            close_price: Uint128::zero(),
            winner: Some(Direction::Bear),
            bull_amount: Uint128::zero(),
            bear_amount: Uint128::zero(),
        }),
    }
}

fn query_my_current_position(deps: Deps, address: String) -> StdResult<MyCurrentPositionResponse> {
    let round_id = NEXT_ROUND_ID.load(deps.storage)?;
    let next_bet_key = (round_id - 1, deps.api.addr_validate(&address)?);

    let next_bet_info = bet_info_storage().may_load(deps.storage, next_bet_key)?;

    let mut next_bull_amount = Uint128::zero();
    let mut next_bear_amount = Uint128::zero();

    match next_bet_info {
        Some(bet_info) => match bet_info.direction {
            Direction::Bull => {
                next_bull_amount = bet_info.amount;
            }
            Direction::Bear => {
                next_bear_amount = bet_info.amount;
            }
        },
        None => {}
    }

    let mut live_bull_amount: Uint128 = Uint128::zero();
    let mut live_bear_amount: Uint128 = Uint128::zero();
    if round_id > 1 {
        let live_bet_key = (round_id - 2, deps.api.addr_validate(&address)?);
        let live_bet_info = bet_info_storage().may_load(deps.storage, live_bet_key)?;
        match live_bet_info {
            Some(bet_info) => match bet_info.direction {
                Direction::Bull => {
                    live_bull_amount = bet_info.amount;
                }
                Direction::Bear => {
                    live_bear_amount = bet_info.amount;
                }
            },
            None => {}
        }
    }

    Ok(MyCurrentPositionResponse {
        next_bear_amount,
        next_bull_amount,
        live_bear_amount,
        live_bull_amount,
    })
}

fn query_status(deps: Deps, env: Env) -> StdResult<StatusResponse> {
    let live_round = LIVE_ROUND.may_load(deps.storage)?;
    let bidding_round = NEXT_ROUND.may_load(deps.storage)?;
    let total_volume = TOTAL_VOLUME.load(deps.storage)?;
    let current_time = env.block.time.seconds();

    let finished_round_id = live_round.clone().unwrap().id - Uint128::new(1);

    let finished_round = ROUNDS.load(deps.storage, finished_round_id.u128())?;

    Ok(StatusResponse {
        bidding_round,
        live_round,
        total_volume,
        current_time,
        finished_round,
    })
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    CONFIG.load(deps.storage)
}

pub fn query_my_games(
    deps: Deps,
    player: Addr,
    start_after: Option<Uint128>,
    limit: Option<u32>,
) -> StdResult<MyGameResponse> {
    let limit = limit.unwrap_or(DEFAULT_QUERY_LIMIT).min(MAX_QUERY_LIMIT) as usize;

    let start = if let Some(start) = start_after {
        let round_id = start;
        Some(Bound::exclusive(bet_info_key(round_id.u128(), &player)))
    } else {
        None
    };

    let my_game_list = bet_info_storage()
        .idx
        .player
        .prefix(player.clone())
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|res| res.map(|item| item.1))
        .collect::<StdResult<Vec<_>>>()?;
    Ok(MyGameResponse { my_game_list })
}

pub fn query_reverse_my_games(
    deps: Deps,
    player: Addr,
    start_after: Option<Uint128>,
    limit: Option<u32>,
) -> StdResult<MyGameResponse> {
    let limit = limit.unwrap_or(DEFAULT_QUERY_LIMIT).min(MAX_QUERY_LIMIT) as usize;

    let start = if let Some(start) = start_after {
        let round_id = start;
        Some(Bound::exclusive(bet_info_key(round_id.u128(), &player)))
    } else {
        None
    };

    let my_game_list = bet_info_storage()
        .idx
        .player
        .prefix(player.clone())
        .range(deps.storage, None, start, Order::Descending)
        .take(limit)
        .map(|res| res.map(|item| item.1))
        .collect::<StdResult<Vec<_>>>()?;
    Ok(MyGameResponse { my_game_list })
}

pub fn query_my_pending_reward(deps: Deps, player: Addr) -> StdResult<PendingRewardResponse> {
    let my_game_list = query_my_games_without_limit(deps, player.clone())?;
    let mut winnings = Uint128::zero();

    for game in my_game_list.my_game_list {
        let round_id = game.round_id;
        let round = ROUNDS.may_load(deps.storage, round_id.u128())?;

        if round.is_none() {
            continue;
        }
        let round = round.unwrap();

        let pool_shares = round.bear_amount + round.bull_amount;

        if round.bear_amount == Uint128::zero() || round.bull_amount == Uint128::zero() {
            winnings += game.amount;
        } else {
            let round_winnings = match round.winner {
                Some(Direction::Bull) => {
                    /* Only claimable once */
                    match game.direction {
                        Direction::Bull => {
                            let won_shares = game.amount;
                            pool_shares.multiply_ratio(won_shares, round.bull_amount)
                        }
                        Direction::Bear => Uint128::zero(),
                    }
                }
                Some(Direction::Bear) => {
                    /* Only claimable once */
                    match game.direction {
                        Direction::Bull => Uint128::zero(),
                        Direction::Bear => {
                            let won_shares = game.amount;
                            pool_shares.multiply_ratio(won_shares, round.bear_amount)
                        }
                    }
                }
                None => {
                    /* Only claimable once */
                    game.amount
                }
            };

            /* Count it up */
            winnings += round_winnings;
        }
    }

    Ok(PendingRewardResponse {
        pending_reward: winnings,
    })
}

pub fn query_my_games_without_limit(deps: Deps, player: Addr) -> StdResult<MyGameResponse> {
    let my_game_list = bet_info_storage()
        .idx
        .player
        .prefix(player.clone())
        .range(deps.storage, None, None, Order::Ascending)
        .map(|res| res.map(|item| item.1))
        .collect::<StdResult<Vec<_>>>()?;
    Ok(MyGameResponse { my_game_list })
}
