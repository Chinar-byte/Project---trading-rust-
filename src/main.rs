use std::env;
use std::fs::File;
use std::io::BufReader;
use std::process::ExitCode;
use std::time::Instant;

use mock_test_proj::{replay_reader, Event, MatchingEngine, NewOrder, Side};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        None | Some("demo") => demo(),
        Some("replay") => {
            let path = args
                .next()
                .ok_or("usage: mock-test-proj replay <orders.csv>")?;
            replay(&path)?;
        }
        Some("benchmark") => {
            let count = args
                .next()
                .map_or(Ok(250_000), |value| value.parse::<u64>())?;
            benchmark(count);
        }
        Some(command) => {
            return Err(
                format!("unknown command '{command}'; use demo, replay, or benchmark").into(),
            );
        }
    }
    Ok(())
}

fn demo() {
    let mut engine = MatchingEngine::new();
    let orders = [
        NewOrder::limit(1, Side::Buy, 9_999, 12, 1),
        NewOrder::limit(2, Side::Buy, 10_000, 8, 2),
        NewOrder::limit(3, Side::Sell, 10_002, 10, 3),
        NewOrder::limit(4, Side::Sell, 10_001, 7, 4),
        NewOrder::market(5, Side::Buy, 13, 5),
    ];
    println!("MOCK TEST PROJ — deterministic matching demo\n");
    for order in orders {
        println!(
            "submit #{:<2} {:<4} qty={:<3} price={:?}",
            order.id, order.side, order.qty, order.limit_price
        );
        for event in engine.submit(order) {
            if let Event::Trade(trade) = event {
                println!(
                    "  TRADE seq={} maker=#{} taker=#{} price={} qty={}",
                    trade.sequence,
                    trade.maker_order_id,
                    trade.taker_order_id,
                    trade.price,
                    trade.qty
                );
            }
        }
    }
    print_book(&engine);
}

fn replay(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let mut engine = MatchingEngine::new();
    let report = replay_reader(BufReader::new(file), &mut engine)?;
    println!(
        "replayed {} orders: {} trades, {} units traded, {} rejected",
        report.orders, report.trades, report.traded_qty, report.rejected
    );
    print_book(&engine);
    Ok(())
}

fn benchmark(count: u64) {
    let mut engine = MatchingEngine::new();
    let started = Instant::now();
    let mut event_count = 0_u64;
    for id in 1..=count {
        let side = if id % 2 == 0 { Side::Buy } else { Side::Sell };
        let price = if side == Side::Buy { 10_001 } else { 9_999 };
        event_count += engine.submit(NewOrder::limit(id, side, price, 1, id)).len() as u64;
    }
    let elapsed = started.elapsed();
    let throughput = u128::from(count).saturating_mul(1_000_000_000) / elapsed.as_nanos().max(1);
    println!("processed {count} orders / {event_count} events in {elapsed:?}");
    println!("throughput: {throughput} orders/second");
    println!(
        "note: run with --release; this is an end-to-end smoke benchmark, not a latency distribution"
    );
}

fn print_book(engine: &MatchingEngine) {
    let snapshot = engine.snapshot(5);
    println!("\nASKS (best first)");
    for level in snapshot.asks {
        println!(
            "  {:>8} x {:<6} ({} orders)",
            level.price, level.total_qty, level.order_count
        );
    }
    println!("BIDS (best first)");
    for level in snapshot.bids {
        println!(
            "  {:>8} x {:<6} ({} orders)",
            level.price, level.total_qty, level.order_count
        );
    }
}
