use std::fmt;
use std::io::BufRead;

use crate::{Event, MatchingEngine, NewOrder, Price, Side, TimeInForce};

#[derive(Debug, PartialEq, Eq)]
pub enum ReplayError {
    Io(String),
    Malformed { line: usize, reason: String },
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(reason) => write!(f, "I/O error: {reason}"),
            Self::Malformed { line, reason } => write!(f, "line {line}: {reason}"),
        }
    }
}

impl std::error::Error for ReplayError {}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayReport {
    pub orders: u64,
    pub trades: u64,
    pub traded_qty: u64,
    pub rejected: u64,
}

/// Replay CSV rows: `timestamp_ns,side,price_ticks,qty,tif`.
/// Use `MKT` as the price of a market order. A header and comments are optional.
///
/// # Errors
///
/// Returns [`ReplayError::Io`] if the reader fails, or
/// [`ReplayError::Malformed`] with a line number when a row cannot be parsed.
pub fn replay_reader<R: BufRead>(
    reader: R,
    engine: &mut MatchingEngine,
) -> Result<ReplayReport, ReplayError> {
    let mut report = ReplayReport::default();
    for (index, line) in reader.lines().enumerate() {
        let line_number = index + 1;
        let line = line.map_err(|error| ReplayError::Io(error.to_string()))?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("timestamp_ns") {
            continue;
        }
        let fields: Vec<_> = trimmed.split(',').map(str::trim).collect();
        if fields.len() != 5 {
            return Err(malformed(
                line_number,
                "expected five comma-separated fields",
            ));
        }
        let timestamp_ns = parse(fields[0], line_number, "timestamp")?;
        let side = match fields[1].to_ascii_uppercase().as_str() {
            "BUY" | "B" => Side::Buy,
            "SELL" | "S" => Side::Sell,
            _ => return Err(malformed(line_number, "side must be BUY or SELL")),
        };
        let limit_price = if fields[2].eq_ignore_ascii_case("MKT") {
            None
        } else {
            Some(Price(parse(fields[2], line_number, "price")?))
        };
        let qty = parse(fields[3], line_number, "quantity")?;
        let tif = match fields[4].to_ascii_uppercase().as_str() {
            "GTC" => TimeInForce::Gtc,
            "IOC" => TimeInForce::Ioc,
            _ => return Err(malformed(line_number, "TIF must be GTC or IOC")),
        };
        report.orders += 1;
        let order = NewOrder {
            id: report.orders,
            side,
            limit_price,
            qty,
            timestamp_ns,
            tif,
        };
        for event in engine.submit(order) {
            match event {
                Event::Trade(trade) => {
                    report.trades += 1;
                    report.traded_qty += trade.qty;
                }
                Event::Rejected { .. } => report.rejected += 1,
                _ => {}
            }
        }
    }
    Ok(report)
}

fn parse<T: std::str::FromStr>(value: &str, line: usize, field: &str) -> Result<T, ReplayError> {
    value
        .parse()
        .map_err(|_| malformed(line, &format!("invalid {field}: {value}")))
}

fn malformed(line: usize, reason: &str) -> ReplayError {
    ReplayError::Malformed {
        line,
        reason: reason.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn deterministic_replay_produces_report() {
        let csv = "timestamp_ns,side,price_ticks,qty,tif\n1,SELL,101,5,GTC\n2,BUY,MKT,3,IOC\n";
        let mut engine = MatchingEngine::new();
        let report = replay_reader(Cursor::new(csv), &mut engine).unwrap();
        assert_eq!(
            report,
            ReplayReport {
                orders: 2,
                trades: 1,
                traded_qty: 3,
                rejected: 0
            }
        );
        assert_eq!(engine.best_ask(), Some(Price(101)));
    }

    #[test]
    fn malformed_rows_include_line_number() {
        let mut engine = MatchingEngine::new();
        let error = replay_reader(Cursor::new("1,HOLD,100,2,GTC\n"), &mut engine).unwrap_err();
        assert!(matches!(error, ReplayError::Malformed { line: 1, .. }));
    }
}
