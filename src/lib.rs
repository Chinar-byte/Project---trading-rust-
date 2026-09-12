//! Ferrum Exchange: a small, deterministic exchange simulator.
//!
//! Prices are represented as integer ticks and quantities as integers. The hot
//! path therefore contains no floating-point arithmetic and the same input
//! always produces the same output.

pub mod book;
pub mod model;
pub mod replay;
pub mod risk;

pub use book::{BookSnapshot, MatchingEngine, PriceLevel};
pub use model::{Event, NewOrder, OrderId, Price, Qty, RejectReason, Side, TimeInForce, Trade};
pub use replay::{ReplayError, ReplayReport, replay_reader};
pub use risk::{RiskDecision, RiskLimits, RiskManager};
