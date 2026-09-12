# Ferrum Exchange

A deterministic limit-order-book simulator written in Rust. It demonstrates the engineering concerns that matter in electronic trading: correctness, predictable data representation, price-time priority, pre-trade risk, replayability, and performance measurement.

This is deliberately a compact exchange core rather than a pretend profitable strategy. The interesting question is not “does a backtest line go up?” but “can this system preserve market invariants under every order sequence?”

## What it demonstrates

- **Price-time priority:** best price wins; FIFO ordering breaks ties at a level.
- **Limit and market orders:** GTC orders rest; IOC remainders are cancelled.
- **Correct execution pricing:** aggressive orders trade at the resting maker price.
- **Book operations:** multi-level sweeping, partial fills, cancellation, and depth snapshots.
- **Pre-trade controls:** order-size, worst-case position, and notional limits.
- **Deterministic replay:** a small CSV feed parser with useful line-numbered errors.
- **Engineering hygiene:** no unsafe code, no runtime dependencies, unit/integration tests, Clippy, formatting, and CI.

## Quick start

```bash
cargo run -- demo
cargo run -- replay data/sample_orders.csv
cargo test
cargo clippy --all-targets -- -D warnings
cargo run --release -- benchmark 1000000
```

Prices use integer ticks. If one tick is $0.01, `10001` means $100.01. This avoids floating-point rounding on the matching path.

## Architecture

```text
CSV replay / CLI
       │
       ▼
 pre-trade risk ── reject
       │ accept
       ▼
 matching engine ──► ordered execution events
       │
       ├── bids: BTreeMap<Price, VecDeque<Order>>
       └── asks: BTreeMap<Price, VecDeque<Order>>
```

`BTreeMap` provides sorted price discovery in logarithmic time. Each price maps to a `VecDeque`, making FIFO insertion and removal at the head constant time. An order-id index locates the correct side and level for cancellation. The engine is synchronous and single-writer by design: sequencing is explicit, state transitions are atomic, and callers can partition instruments across engine instances.

### Core invariants

1. A completed call never leaves a crossed book.
2. Executions occur at the maker's price.
3. Better prices execute before worse prices.
4. Earlier orders execute first within one price level.
5. A resting order ID is unique.
6. Quantities are conserved across fills, rests, and IOC cancellation.

These rules are encoded in tests rather than left as comments.

## Performance

The built-in benchmark is intentionally honest: it reports end-to-end throughput for a repeatable synthetic workload. It is not presented as a production-grade latency result; serious latency work would pin CPU affinity, warm caches, collect a distribution with HDR histograms, isolate allocation, and record hardware/compiler configuration.

The current implementation optimizes for readable correctness. Natural next steps are intrusive queues for O(1) arbitrary cancellation, sharded symbol ownership, binary market-data decoding, write-ahead event journaling, and property/fuzz testing.

## Repository map

```text
src/model.rs    Domain types and execution events
src/book.rs     Matching engine and depth snapshots
src/risk.rs     Pre-trade limits and position accounting
src/replay.rs   Deterministic CSV market replay
src/main.rs     Demo, replay, and benchmark CLI
tests/          End-to-end invariant tests
data/           Small reproducible order stream
```

## Interview discussion points

- Why integer ticks are preferable to floating point for price.
- Why the matching engine is a single writer, and how to scale by symbol.
- The latency/cancellation trade-off of `VecDeque` plus an order location index.
- How deterministic inputs and ordered events enable incident replay.
- Where risk belongs when exchange, broker, and strategy concerns are separated.
- How to benchmark tail latency without making misleading claims.

## Scope and limitations

This is an educational simulator, not a venue or a trading recommendation. It currently models one instrument per engine and omits persistence, networking, authentication, self-trade prevention, modify-order semantics, auctions, and exchange-specific order types. Those omissions are explicit so reviewers can distinguish implemented behavior from future work.

## License

MIT

