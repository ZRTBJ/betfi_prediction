use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Index, IndexList, IndexedMap, Item, Map, MultiIndex};

use crate::msg::{Config, Direction, FinishedRound, LiveRound, NextRound};

pub const IS_HAULTED: Item<bool> = Item::new("is_haulted");
pub const CONFIG: Item<Config> = Item::new("config");
pub const NEXT_ROUND_ID: Item<u128> = Item::new("next_round_id");
/* The round that's open for betting */
pub const NEXT_ROUND: Item<NextRound> = Item::new("next_round");
/* The live round; not accepting bets */
pub const LIVE_ROUND: Item<LiveRound> = Item::new("live_round");

pub const ACCUMULATED_FEE: Item<u128> = Item::new("accumulated_fee");

pub const ROUNDS: Map<u128, FinishedRound> = Map::new("rounds");

pub const TOTAL_VOLUME: Item<Uint128> = Item::new("total_volume");

#[cw_serde]
pub struct BetInfo {
    pub player: Addr,
    pub round_id: Uint128,
    pub amount: Uint128,
    pub direction: Direction,
}

/// Primary key for betinfo: (round_id, player)
pub type BetInfoKey = (u128, Addr);
/// Convenience bid key constructor
pub fn bet_info_key(round_id: u128, player: &Addr) -> BetInfoKey {
    (round_id, player.clone())
}

/// Defines incides for accessing bids
pub struct BetInfoIndicies<'a> {
    pub player: MultiIndex<'a, Addr, BetInfo, BetInfoKey>,
}

impl<'a> IndexList<BetInfo> for BetInfoIndicies<'a> {
    fn get_indexes(&'_ self) -> Box<dyn Iterator<Item = &'_ dyn Index<BetInfo>> + '_> {
        let v: Vec<&dyn Index<BetInfo>> = vec![&self.player];
        Box::new(v.into_iter())
    }
}

pub fn bet_info_storage<'a>() -> IndexedMap<'a, BetInfoKey, BetInfo, BetInfoIndicies<'a>> {
    let indexes = BetInfoIndicies {
        player: MultiIndex::new(
            |_pk: &[u8], d: &BetInfo| d.player.clone(),
            "bet_info",
            "bet_info_collection",
        ),
    };
    IndexedMap::new("bet_info", indexes)
}

#[cw_serde]
pub struct MyGameResponse {
    pub my_game_list: Vec<BetInfo>,
}

#[cw_serde]
pub struct PendingRewardResponse {
    pub pending_reward: Uint128,
}
