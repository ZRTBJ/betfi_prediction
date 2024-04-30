use crate::error::ContractError;
use crate::msg::{
    Config, Direction, ExecuteMsg, FastOracleQueryMsg, FinishedRound, InstantiateMsg, LiveRound,
    MigrateMsg, NextRound, QueryMsg,
};
use crate::query::query_my_games_without_limit;
use crate::state::{
    bet_info_key, bet_info_storage, BetInfo, ACCUMULATED_FEE, CONFIG, IS_HAULTED, LIVE_ROUND,
    NEXT_ROUND, NEXT_ROUND_ID, ROUNDS, TOTAL_VOLUME,
};

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Coin, CosmosMsg, Deps, DepsMut, Env, Event, MessageInfo, QueryRequest,
    Response, StdError, StdResult, Uint128, WasmMsg, WasmQuery,
};
use cw20::Cw20ExecuteMsg;

const CONTRACT_NAME: &str = "price_prediction";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const FEE_PRECISION: u128 = 100u128;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    /* Validate addresses */
    deps.api
        .addr_validate(msg.config.fast_oracle_addr.as_ref())?;

    CONFIG.save(deps.storage, &msg.config)?;
    NEXT_ROUND_ID.save(deps.storage, &0u128)?;
    ACCUMULATED_FEE.save(deps.storage, &0u128)?;
    IS_HAULTED.save(deps.storage, &false)?;
    TOTAL_VOLUME.save(deps.storage, &Uint128::zero())?;

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, MigrateMsg {}: MigrateMsg) -> StdResult<Response> {
    let version = cw2::get_contract_version(deps.storage)?;
    if version.contract != CONTRACT_NAME {
        return Err(StdError::generic_err("Can only upgrade from same type"));
    }

    cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateConfig { config } => execute_update_config(deps, info, env, config),
        ExecuteMsg::BetBear { round_id, amount } => {
            execute_bet(deps, info, env, round_id, Direction::Bear, amount)
        }
        ExecuteMsg::BetBull { round_id, amount } => {
            execute_bet(deps, info, env, round_id, Direction::Bull, amount)
        }
        ExecuteMsg::CloseRound {} => execute_close_round(deps, info, env),
        ExecuteMsg::CollectWinnings {} => execute_collect_winnings(deps, info),
        ExecuteMsg::Hault {} => execute_update_hault(deps, info, env, true),
        ExecuteMsg::Resume {} => execute_update_hault(deps, info, env, false),
    }
}

fn execute_collect_winnings(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let mut winnings = Uint128::zero();
    let resp = Response::new();

    // let no_duplicate_rounds: HashSet<u128> =
    //     HashSet::from_iter(rounds.iter().cloned());

    let my_game_list = query_my_games_without_limit(deps.as_ref(), info.sender.clone())?;

    for game in my_game_list.my_game_list {
        let next_round_id = NEXT_ROUND_ID.load(deps.storage)?;
        if game.round_id < (Uint128::new(next_round_id) - Uint128::new(1)) {
            let round_id = game.round_id;
            let round = ROUNDS.load(deps.storage, round_id.u128())?;

            let pool_shares = round.bear_amount + round.bull_amount;
            let bet_info_key = bet_info_key(round_id.u128(), &info.sender);

            bet_info_storage().remove(deps.storage, bet_info_key.clone())?;

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
    }

    if winnings == Uint128::zero() {
        return Err(ContractError::Std(StdError::generic_err(
            "Nothing to claim",
        )));
    }

    let msg_send_winnings: CosmosMsg;

    msg_send_winnings = get_cw20_transfer_msg(&config.token_addr, &info.sender, winnings)?;

    Ok(resp
        .add_message(msg_send_winnings)
        .add_attribute("action", "collect-winnings")
        .add_attribute("amount", winnings))
}

fn execute_bet(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    round_id: Uint128,
    dir: Direction,
    gross: Uint128,
) -> Result<Response, ContractError> {
    assert_not_haulted(deps.as_ref())?;

    let mut bet_round = assert_is_current_round(deps.as_ref(), round_id)?;
    let mut resp = Response::new();
    let config = CONFIG.load(deps.storage)?;

    let current_time = env.block.time.seconds();
    let open_time = bet_round.open_time.seconds();

    if gross < config.minimum_bet {
        return Err(ContractError::InsufficientFundsForBet {});
    }

    if env.block.time > bet_round.open_time {
        return Err(ContractError::Std(StdError::generic_err(format!(
            "Round {} stopped accepting bids {} second(s) ago; the next round has not yet begun",
            round_id,
            (current_time - open_time)
        ))));
    }

    let staker_fee = compute_gaming_fee(deps.as_ref(), gross)?;
    ACCUMULATED_FEE.update(deps.storage, |fee_before| -> Result<u128, StdError> {
        Ok(fee_before + staker_fee.u128())
    })?;

    /* Deduct open + burn fee from the gross amount */
    let bet_amt = gross - staker_fee;

    TOTAL_VOLUME.update(deps.storage, |mut volume| -> StdResult<_> {
        volume = volume + bet_amt;
        Ok(volume)
    })?;

    let bet_info_key = bet_info_key(round_id.u128(), &info.sender.clone());

    let bet_info = bet_info_storage().may_load(deps.storage, bet_info_key.clone())?;

    if !bet_info.is_none() {
        return Err(ContractError::Std(StdError::generic_err(format!(
            "You are already bet for this game for {}, with amount: {}",
            bet_info.clone().unwrap().direction.to_string(),
            bet_info.unwrap().amount
        ))));
    }

    match dir {
        Direction::Bull => {
            // BULL_BETS.save(deps.storage, bet_key, &bet_amt.u128())?;
            bet_info_storage().save(
                deps.storage,
                bet_info_key.clone(),
                &BetInfo {
                    player: info.sender.clone(),
                    round_id,
                    amount: bet_amt,
                    direction: Direction::Bull,
                },
            )?;
            bet_round.bull_amount += bet_amt;
            NEXT_ROUND.save(deps.storage, &bet_round)?;
            resp = resp.add_event(Event::new("prediction_bet").add_attributes(vec![
                ("action", "betfi-bet".to_string()),
                ("round", round_id.to_string()),
                ("direction", "bull".to_string()),
                ("amount", bet_amt.to_string()),
                ("round_bull_total", bet_round.bull_amount.to_string()),
                ("account", info.sender.to_string()),
            ]));
        }
        Direction::Bear => {
            bet_info_storage().save(
                deps.storage,
                bet_info_key.clone(),
                &BetInfo {
                    player: info.sender.clone(),
                    round_id,
                    amount: bet_amt,
                    direction: Direction::Bear,
                },
            )?;
            bet_round.bear_amount += bet_amt;
            NEXT_ROUND.save(deps.storage, &bet_round)?;
            resp = resp.add_event(Event::new("prediction_bet").add_attributes(vec![
                ("action", "betfi-bet".to_string()),
                ("round", round_id.to_string()),
                ("direction", "bear".to_string()),
                ("amount", bet_amt.to_string()),
                ("round_bear_total", bet_round.bear_amount.to_string()),
                ("account", info.sender.to_string()),
            ]));
        }
    }

    let contract_addrss = env.contract.address;

    let transfer_from_msg = get_cw20_transfer_from_msg(
        &config.token_addr,
        &info.sender,
        &contract_addrss,
        //burn fee would be disappeared from user's wallet directly
        gross,
    )?;
    resp = resp.add_message(transfer_from_msg);

    Ok(resp)
}

fn execute_close_round(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
) -> Result<Response, ContractError> {
    assert_not_haulted(deps.as_ref())?;
    assert_is_admin(deps.as_ref(), info, env.clone())?;
    let now = env.block.time;
    let config = CONFIG.load(deps.storage)?;
    let mut resp: Response = Response::new();

    /*
     * Close the live round if it is finished
     */
    let maybe_live_round = LIVE_ROUND.may_load(deps.storage)?;
    match &maybe_live_round {
        Some(live_round) => {
            if now >= live_round.close_time {
                let finished_round = compute_round_close(deps.as_ref(), live_round)?;
                ROUNDS.save(deps.storage, live_round.id.u128(), &finished_round)?;

                resp = resp.add_event(Event::new("prediction_bet").add_attributes(vec![
                    ("round_dead", live_round.id.to_string()),
                    ("close_price", finished_round.close_price.to_string()),
                    (
                        "winner",
                        match finished_round.winner {
                            Some(w) => w.to_string(),
                            None => "everybody".to_string(),
                        },
                    ),
                ]));
                LIVE_ROUND.remove(deps.storage);
            }
        }
        None => {}
    }

    /* Close the bidding round if it is finished
     * NOTE Don't allow two live rounds at the same time - wait for the other to close
     */
    let new_bid_round = |deps: DepsMut, env: Env| -> StdResult<Uint128> {
        let id: Uint128 = Uint128::from(NEXT_ROUND_ID.load(deps.storage)?);
        let open_time = match LIVE_ROUND.may_load(deps.storage)? {
            Some(live_round) => live_round.close_time,
            None => env
                .block
                .time
                .plus_seconds(config.next_round_seconds.u128() as u64),
        };
        let close_time = open_time.plus_seconds(config.next_round_seconds.u128() as u64);

        NEXT_ROUND.save(
            deps.storage,
            &NextRound {
                bear_amount: Uint128::zero(),
                bull_amount: Uint128::zero(),
                bid_time: env.block.time,
                close_time,
                open_time,
                id,
            },
        )?;
        NEXT_ROUND_ID.save(deps.storage, &(id.u128() + 1u128))?;
        Ok(id)
    };
    let maybe_open_round = NEXT_ROUND.may_load(deps.storage)?;
    match &maybe_open_round {
        Some(open_round) => {
            if LIVE_ROUND.may_load(deps.storage)?.is_none() && now >= open_round.open_time {
                let live_round = compute_round_open(deps.as_ref(), env.clone(), open_round)?;
                resp = resp.add_event(Event::new("prediction_bet").add_attributes(vec![
                    ("round_bidding_close", live_round.id),
                    ("open_price", live_round.open_price),
                    ("bear_amount", live_round.bear_amount),
                    ("bull_amount", live_round.bull_amount),
                ]));
                LIVE_ROUND.save(deps.storage, &live_round)?;
                NEXT_ROUND.remove(deps.storage);
                let new_round_id = new_bid_round(deps, env)?;
                resp = resp.add_event(
                    Event::new("prediction_bet").add_attribute("round_bidding_open", new_round_id),
                );
            }
        }
        None => {
            let new_round_id = new_bid_round(deps, env)?;
            resp = resp.add_event(
                Event::new("prediction_bet").add_attribute("round_bidding_open", new_round_id),
            );
        }
    }

    Ok(resp.add_attribute("action", "close-round"))
}

fn execute_update_config(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    config: Config,
) -> Result<Response, ContractError> {
    assert_is_admin(deps.as_ref(), info, env)?;

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new())
}

fn assert_is_current_round(deps: Deps, round_id: Uint128) -> StdResult<NextRound> {
    let open_round = NEXT_ROUND.load(deps.storage)?;

    if round_id != open_round.id {
        return Err(StdError::generic_err(format!(
            "Tried to open at round {} but it's currently round {}",
            round_id, open_round.id
        )));
    }

    Ok(open_round)
}

fn compute_gaming_fee(deps: Deps, gross: Uint128) -> StdResult<Uint128> {
    let staker_fee = CONFIG.load(deps.storage)?.gaming_fee;

    staker_fee
        .checked_multiply_ratio(gross, FEE_PRECISION * 100)
        .map_err(|e| StdError::generic_err(e.to_string()))
}

fn compute_round_open(deps: Deps, env: Env, round: &NextRound) -> StdResult<LiveRound> {
    /* TODO */
    let open_price = get_current_price(deps)?;
    let config = CONFIG.load(deps.storage)?;

    Ok(LiveRound {
        id: round.id,
        bid_time: round.bid_time,
        open_time: env.block.time,
        close_time: env
            .block
            .time
            .plus_seconds(config.next_round_seconds.u128() as u64),
        open_price,
        bull_amount: round.bull_amount,
        bear_amount: round.bear_amount,
    })
}

fn get_current_price(deps: Deps) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;

    let price: Uint128 = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: config.fast_oracle_addr.to_string(),
        msg: to_binary(&FastOracleQueryMsg::Price {})?,
    }))?;

    Ok(price)
}

fn compute_round_close(deps: Deps, round: &LiveRound) -> StdResult<FinishedRound> {
    let close_price = get_current_price(deps)?;

    let winner = match close_price.cmp(&round.open_price) {
        std::cmp::Ordering::Greater =>
        /* Bulls win */
        {
            Some(Direction::Bull)
        }
        std::cmp::Ordering::Less =>
        /* Bears win */
        {
            Some(Direction::Bear)
        }
        std::cmp::Ordering::Equal =>
        /* Weird case where nobody was right */
        {
            None
        }
    };

    Ok(FinishedRound {
        id: round.id,
        bid_time: round.bid_time,
        open_time: round.open_time,
        close_time: round.close_time,
        open_price: round.open_price,
        bear_amount: round.bear_amount,
        bull_amount: round.bull_amount,
        winner,
        close_price,
    })
}

fn assert_not_haulted(deps: Deps) -> StdResult<bool> {
    let is_haulted = IS_HAULTED.load(deps.storage)?;
    if is_haulted {
        return Err(StdError::generic_err("Contract is haulted"));
    }
    Ok(true)
}

fn execute_update_hault(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    is_haulted: bool,
) -> Result<Response, ContractError> {
    assert_is_admin(deps.as_ref(), info, env)?;
    IS_HAULTED.save(deps.storage, &is_haulted)?;
    Ok(
        Response::new()
            .add_event(Event::new("prediction_bet").add_attribute("hault_games", "true")),
    )
}

fn assert_is_admin(deps: Deps, info: MessageInfo, env: Env) -> StdResult<bool> {
    let admin = deps
        .querier
        .query_wasm_contract_info(env.contract.address)?
        .admin
        .unwrap_or_default();

    if info.sender != admin {
        return Err(StdError::generic_err(format!(
            "Only the admin can execute this function. Admin: {}, Sender: {}",
            admin, info.sender
        )));
    }

    Ok(true)
}

pub fn get_cw20_transfer_msg(
    token_addr: &Addr,
    recipient: &Addr,
    amount: Uint128,
) -> StdResult<CosmosMsg> {
    let transfer_cw20_msg = Cw20ExecuteMsg::Transfer {
        recipient: recipient.into(),
        amount,
    };

    let exec_cw20_transfer_msg = WasmMsg::Execute {
        contract_addr: token_addr.into(),
        msg: to_binary(&transfer_cw20_msg)?,
        funds: vec![],
    };

    let cw20_transfer_msg: CosmosMsg = exec_cw20_transfer_msg.into();
    Ok(cw20_transfer_msg)
}

pub fn get_cw20_transfer_from_msg(
    token_addr: &Addr,
    owner: &Addr,
    recipient: &Addr,
    amount: Uint128,
) -> StdResult<CosmosMsg> {
    let transfer_cw20_msg = Cw20ExecuteMsg::TransferFrom {
        owner: owner.into(),
        recipient: recipient.into(),
        amount,
    };

    let exec_cw20_transfer_msg = WasmMsg::Execute {
        contract_addr: token_addr.into(),
        msg: to_binary(&transfer_cw20_msg)?,
        funds: vec![],
    };

    let cw20_transfer_msg: CosmosMsg = exec_cw20_transfer_msg.into();
    Ok(cw20_transfer_msg)
}

pub fn get_cw20_burn_from_msg(
    token_addr: &Addr,
    owner: &Addr,
    amount: Uint128,
) -> StdResult<CosmosMsg> {
    let transfer_cw20_msg = Cw20ExecuteMsg::BurnFrom {
        owner: owner.into(),
        amount,
    };
    let exec_cw20_transfer_msg = WasmMsg::Execute {
        contract_addr: token_addr.into(),
        msg: to_binary(&transfer_cw20_msg)?,
        funds: vec![],
    };

    let cw20_transfer_msg: CosmosMsg = exec_cw20_transfer_msg.into();
    Ok(cw20_transfer_msg)
}

pub fn get_bank_transfer_to_msg(
    recipient: &Addr,
    denom: &str,
    amount: Uint128,
) -> StdResult<CosmosMsg> {
    let transfer_bank_msg = cosmwasm_std::BankMsg::Send {
        to_address: recipient.into(),
        amount: vec![Coin {
            denom: denom.to_string(),
            amount,
        }],
    };

    let transfer_bank_cosmos_msg: CosmosMsg = transfer_bank_msg.into();
    Ok(transfer_bank_cosmos_msg)
}
