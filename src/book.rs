use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::model::{Event, NewOrder, OrderId, Price, Qty, RejectReason, Side, TimeInForce, Trade};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RestingOrder {
    id: OrderId,
    qty: Qty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PriceLevel {
    pub price: Price,
    pub total_qty: Qty,
    pub order_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BookSnapshot {
    /// Best bid first.
    pub bids: Vec<PriceLevel>,
    /// Best ask first.
    pub asks: Vec<PriceLevel>,
}

/// A single-instrument matching engine with price-time priority.
///
/// The engine intentionally owns no clock and performs no I/O. A caller supplies
/// timestamps, which makes replay and testing deterministic.
#[derive(Debug, Default)]
pub struct MatchingEngine {
    bids: BTreeMap<Price, VecDeque<RestingOrder>>,
    asks: BTreeMap<Price, VecDeque<RestingOrder>>,
    locations: HashMap<OrderId, (Side, Price)>,
    next_trade_sequence: u64,
}

impl MatchingEngine {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Submit an order and return every resulting event in causal order.
    ///
    /// # Panics
    ///
    /// Panics only if the engine's private book/index invariants have been
    /// corrupted, which cannot occur through the public API.
    pub fn submit(&mut self, order: NewOrder) -> Vec<Event> {
        if let Some(reason) = self.validate(&order) {
            return vec![Event::Rejected {
                order_id: order.id,
                reason,
            }];
        }

        let mut events = vec![Event::Accepted { order_id: order.id }];
        let mut remaining = order.qty;

        while remaining > 0 {
            let Some(best_price) = self.best_opposite_price(order.side) else {
                break;
            };
            if !crosses(order.side, order.limit_price, best_price) {
                break;
            }

            let (maker_id, fill_qty, level_empty) = {
                let levels = match order.side {
                    Side::Buy => &mut self.asks,
                    Side::Sell => &mut self.bids,
                };
                let queue = levels.get_mut(&best_price).expect("best price must exist");
                let maker = queue.front_mut().expect("price level must not be empty");
                let fill_qty = remaining.min(maker.qty);
                maker.qty -= fill_qty;
                let maker_id = maker.id;
                if maker.qty == 0 {
                    queue.pop_front();
                    self.locations.remove(&maker_id);
                }
                (maker_id, fill_qty, queue.is_empty())
            };

            if level_empty {
                match order.side {
                    Side::Buy => self.asks.remove(&best_price),
                    Side::Sell => self.bids.remove(&best_price),
                };
            }

            remaining -= fill_qty;
            self.next_trade_sequence += 1;
            events.push(Event::Trade(Trade {
                sequence: self.next_trade_sequence,
                maker_order_id: maker_id,
                taker_order_id: order.id,
                aggressor_side: order.side,
                price: best_price,
                qty: fill_qty,
            }));
        }

        if remaining > 0 {
            if order.tif == TimeInForce::Gtc {
                let price = order.limit_price.expect("validated GTC order has a price");
                let resting = RestingOrder {
                    id: order.id,
                    qty: remaining,
                };
                let levels = match order.side {
                    Side::Buy => &mut self.bids,
                    Side::Sell => &mut self.asks,
                };
                levels.entry(price).or_default().push_back(resting);
                self.locations.insert(order.id, (order.side, price));
                events.push(Event::Resting {
                    order_id: order.id,
                    remaining,
                });
            } else {
                events.push(Event::Cancelled {
                    order_id: order.id,
                    remaining,
                });
            }
        }

        events
    }

    /// Cancel a resting order. Returns the removed quantity, if it existed.
    pub fn cancel(&mut self, order_id: OrderId) -> Option<Event> {
        let (side, price) = self.locations.remove(&order_id)?;
        let levels = match side {
            Side::Buy => &mut self.bids,
            Side::Sell => &mut self.asks,
        };
        let queue = levels.get_mut(&price)?;
        let position = queue.iter().position(|order| order.id == order_id)?;
        let removed = queue.remove(position)?;
        if queue.is_empty() {
            levels.remove(&price);
        }
        Some(Event::Cancelled {
            order_id,
            remaining: removed.qty,
        })
    }

    #[must_use]
    pub fn best_bid(&self) -> Option<Price> {
        self.bids.last_key_value().map(|(price, _)| *price)
    }

    #[must_use]
    pub fn best_ask(&self) -> Option<Price> {
        self.asks.first_key_value().map(|(price, _)| *price)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.locations.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.locations.is_empty()
    }

    #[must_use]
    pub fn snapshot(&self, depth: usize) -> BookSnapshot {
        let to_level = |(price, orders): (&Price, &VecDeque<RestingOrder>)| PriceLevel {
            price: *price,
            total_qty: orders.iter().map(|order| order.qty).sum(),
            order_count: orders.len(),
        };
        BookSnapshot {
            bids: self.bids.iter().rev().take(depth).map(to_level).collect(),
            asks: self.asks.iter().take(depth).map(to_level).collect(),
        }
    }

    fn validate(&self, order: &NewOrder) -> Option<RejectReason> {
        if self.locations.contains_key(&order.id) {
            return Some(RejectReason::DuplicateOrderId);
        }
        if order.qty == 0 {
            return Some(RejectReason::EmptyQuantity);
        }
        if order.limit_price.is_some_and(|price| price.0 <= 0) {
            return Some(RejectReason::InvalidPrice);
        }
        if order.limit_price.is_none() && order.tif != TimeInForce::Ioc {
            return Some(RejectReason::MarketOrderMustBeIoc);
        }
        None
    }

    fn best_opposite_price(&self, side: Side) -> Option<Price> {
        match side {
            Side::Buy => self.best_ask(),
            Side::Sell => self.best_bid(),
        }
    }
}

fn crosses(side: Side, limit: Option<Price>, opposite: Price) -> bool {
    match (side, limit) {
        (_, None) => true,
        (Side::Buy, Some(limit)) => limit >= opposite,
        (Side::Sell, Some(limit)) => limit <= opposite,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trades(events: &[Event]) -> Vec<Trade> {
        events
            .iter()
            .filter_map(|event| match event {
                Event::Trade(trade) => Some(*trade),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn matches_at_resting_orders_price() {
        let mut engine = MatchingEngine::new();
        engine.submit(NewOrder::limit(1, Side::Sell, 101, 10, 1));
        let result = engine.submit(NewOrder::limit(2, Side::Buy, 105, 4, 2));
        assert_eq!(trades(&result)[0].price, Price(101));
        assert_eq!(engine.best_ask(), Some(Price(101)));
    }

    #[test]
    fn preserves_fifo_at_a_price_level() {
        let mut engine = MatchingEngine::new();
        engine.submit(NewOrder::limit(10, Side::Sell, 100, 2, 1));
        engine.submit(NewOrder::limit(11, Side::Sell, 100, 2, 2));
        let result = engine.submit(NewOrder::market(12, Side::Buy, 3, 3));
        let fills = trades(&result);
        assert_eq!(fills.len(), 2);
        assert_eq!(fills[0].maker_order_id, 10);
        assert_eq!(fills[1].maker_order_id, 11);
        assert_eq!(fills[1].qty, 1);
    }

    #[test]
    fn walks_multiple_price_levels() {
        let mut engine = MatchingEngine::new();
        engine.submit(NewOrder::limit(1, Side::Sell, 100, 2, 1));
        engine.submit(NewOrder::limit(2, Side::Sell, 101, 3, 2));
        let fills = trades(&engine.submit(NewOrder::market(3, Side::Buy, 5, 3)));
        assert_eq!(fills.iter().map(|fill| fill.qty).sum::<u64>(), 5);
        assert!(engine.is_empty());
    }

    #[test]
    fn ioc_cancels_unfilled_remainder() {
        let mut engine = MatchingEngine::new();
        engine.submit(NewOrder::limit(1, Side::Sell, 100, 2, 1));
        let result = engine.submit(NewOrder::market(2, Side::Buy, 5, 2));
        assert_eq!(
            result.last(),
            Some(&Event::Cancelled {
                order_id: 2,
                remaining: 3
            })
        );
    }

    #[test]
    fn cancellation_removes_order_and_empty_level() {
        let mut engine = MatchingEngine::new();
        engine.submit(NewOrder::limit(1, Side::Buy, 99, 7, 1));
        assert_eq!(
            engine.cancel(1),
            Some(Event::Cancelled {
                order_id: 1,
                remaining: 7
            })
        );
        assert_eq!(engine.best_bid(), None);
    }

    #[test]
    fn snapshot_aggregates_levels_in_market_order() {
        let mut engine = MatchingEngine::new();
        engine.submit(NewOrder::limit(1, Side::Buy, 99, 2, 1));
        engine.submit(NewOrder::limit(2, Side::Buy, 99, 3, 2));
        engine.submit(NewOrder::limit(3, Side::Buy, 98, 5, 3));
        let snapshot = engine.snapshot(2);
        assert_eq!(
            snapshot.bids[0],
            PriceLevel {
                price: Price(99),
                total_qty: 5,
                order_count: 2
            }
        );
        assert_eq!(snapshot.bids[1].price, Price(98));
    }
}
