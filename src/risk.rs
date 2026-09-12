use crate::model::{NewOrder, Price, Qty, Side};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RiskLimits {
    pub max_order_qty: Qty,
    /// Maximum absolute position after a full fill.
    pub max_position: i64,
    /// Price ticks multiplied by quantity.
    pub max_order_notional: u128,
}

impl Default for RiskLimits {
    fn default() -> Self {
        Self {
            max_order_qty: 1_000,
            max_position: 5_000,
            max_order_notional: 10_000_000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiskDecision {
    Accepted,
    Rejected(RiskRejectReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiskRejectReason {
    OrderQuantityLimit,
    PositionLimit,
    OrderNotionalLimit,
    MissingReferencePrice,
    ArithmeticOverflow,
}

#[derive(Debug)]
pub struct RiskManager {
    limits: RiskLimits,
    position: i64,
}

impl RiskManager {
    #[must_use]
    pub const fn new(limits: RiskLimits) -> Self {
        Self {
            limits,
            position: 0,
        }
    }

    #[must_use]
    pub const fn position(&self) -> i64 {
        self.position
    }

    /// Evaluate worst-case exposure assuming the order fills completely.
    #[must_use]
    pub fn check(&self, order: &NewOrder, reference_price: Option<Price>) -> RiskDecision {
        if order.qty > self.limits.max_order_qty {
            return RiskDecision::Rejected(RiskRejectReason::OrderQuantityLimit);
        }
        let Ok(qty) = i64::try_from(order.qty) else {
            return RiskDecision::Rejected(RiskRejectReason::ArithmeticOverflow);
        };
        let Some(projected) = self
            .position
            .checked_add(order.side.sign().saturating_mul(qty))
        else {
            return RiskDecision::Rejected(RiskRejectReason::ArithmeticOverflow);
        };
        if projected.unsigned_abs() > self.limits.max_position.unsigned_abs() {
            return RiskDecision::Rejected(RiskRejectReason::PositionLimit);
        }
        let Some(price) = order.limit_price.or(reference_price) else {
            return RiskDecision::Rejected(RiskRejectReason::MissingReferencePrice);
        };
        let Some(notional) = u128::from(order.qty).checked_mul(u128::from(price.0.unsigned_abs()))
        else {
            return RiskDecision::Rejected(RiskRejectReason::ArithmeticOverflow);
        };
        if notional > self.limits.max_order_notional {
            return RiskDecision::Rejected(RiskRejectReason::OrderNotionalLimit);
        }
        RiskDecision::Accepted
    }

    /// Apply an execution belonging to this portfolio.
    ///
    /// # Errors
    ///
    /// Returns an error if the fill quantity cannot be represented or applying
    /// it would overflow the signed position.
    pub fn on_fill(&mut self, side: Side, qty: Qty) -> Result<(), &'static str> {
        let qty = i64::try_from(qty).map_err(|_| "fill quantity exceeds i64")?;
        self.position = self
            .position
            .checked_add(side.sign().checked_mul(qty).ok_or("position overflow")?)
            .ok_or("position overflow")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TimeInForce;

    fn order(side: Side, price: Option<Price>, qty: Qty) -> NewOrder {
        NewOrder {
            id: 1,
            side,
            limit_price: price,
            qty,
            timestamp_ns: 1,
            tif: TimeInForce::Ioc,
        }
    }

    #[test]
    fn rejects_oversized_order() {
        let risk = RiskManager::new(RiskLimits {
            max_order_qty: 10,
            ..RiskLimits::default()
        });
        assert_eq!(
            risk.check(&order(Side::Buy, Some(Price(100)), 11), None),
            RiskDecision::Rejected(RiskRejectReason::OrderQuantityLimit)
        );
    }

    #[test]
    fn position_is_directional() {
        let mut risk = RiskManager::new(RiskLimits {
            max_position: 10,
            ..RiskLimits::default()
        });
        risk.on_fill(Side::Buy, 8).unwrap();
        assert_eq!(
            risk.check(&order(Side::Sell, Some(Price(100)), 9), None),
            RiskDecision::Accepted
        );
        assert_eq!(risk.position(), 8);
    }

    #[test]
    fn market_order_requires_reference_price() {
        let risk = RiskManager::new(RiskLimits::default());
        assert_eq!(
            risk.check(&order(Side::Buy, None, 1), None),
            RiskDecision::Rejected(RiskRejectReason::MissingReferencePrice)
        );
    }
}
