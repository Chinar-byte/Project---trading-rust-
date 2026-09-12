use std::fmt;

pub type OrderId = u64;
pub type Qty = u64;

/// An integer number of ticks. Avoids rounding ambiguity in monetary values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Price(pub i64);

impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    #[must_use]
    pub const fn sign(self) -> i64 {
        match self {
            Self::Buy => 1,
            Self::Sell => -1,
        }
    }
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Buy => f.write_str("BUY"),
            Self::Sell => f.write_str("SELL"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeInForce {
    /// Rest any unfilled quantity on the book.
    Gtc,
    /// Cancel any quantity that cannot execute immediately.
    Ioc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NewOrder {
    pub id: OrderId,
    pub side: Side,
    /// `None` is a market order. Market orders must use IOC.
    pub limit_price: Option<Price>,
    pub qty: Qty,
    /// Source timestamp retained for replay/audit. Submission order determines FIFO priority.
    pub timestamp_ns: u64,
    pub tif: TimeInForce,
}

impl NewOrder {
    #[must_use]
    pub const fn limit(
        id: OrderId,
        side: Side,
        price_ticks: i64,
        qty: Qty,
        timestamp_ns: u64,
    ) -> Self {
        Self {
            id,
            side,
            limit_price: Some(Price(price_ticks)),
            qty,
            timestamp_ns,
            tif: TimeInForce::Gtc,
        }
    }

    #[must_use]
    pub const fn market(id: OrderId, side: Side, qty: Qty, timestamp_ns: u64) -> Self {
        Self {
            id,
            side,
            limit_price: None,
            qty,
            timestamp_ns,
            tif: TimeInForce::Ioc,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trade {
    pub sequence: u64,
    pub maker_order_id: OrderId,
    pub taker_order_id: OrderId,
    pub aggressor_side: Side,
    pub price: Price,
    pub qty: Qty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RejectReason {
    DuplicateOrderId,
    EmptyQuantity,
    InvalidPrice,
    MarketOrderMustBeIoc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Accepted {
        order_id: OrderId,
    },
    Trade(Trade),
    Resting {
        order_id: OrderId,
        remaining: Qty,
    },
    Cancelled {
        order_id: OrderId,
        remaining: Qty,
    },
    Rejected {
        order_id: OrderId,
        reason: RejectReason,
    },
}
