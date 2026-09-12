use ferrum_exchange::{Event, MatchingEngine, NewOrder, Price, Side};

#[test]
fn end_to_end_exchange_flow_is_deterministic() {
    let orders = [
        NewOrder::limit(1, Side::Sell, 102, 5, 1),
        NewOrder::limit(2, Side::Sell, 101, 3, 2),
        NewOrder::limit(3, Side::Buy, 102, 7, 3),
    ];
    let run = || {
        let mut engine = MatchingEngine::new();
        let events: Vec<_> = orders
            .into_iter()
            .flat_map(|order| engine.submit(order))
            .collect();
        (events, engine.snapshot(10))
    };
    let first = run();
    let second = run();
    assert_eq!(first, second);
    let fills: Vec<_> = first
        .0
        .iter()
        .filter_map(|event| match event {
            Event::Trade(trade) => Some(trade),
            _ => None,
        })
        .collect();
    assert_eq!(fills.len(), 2);
    assert_eq!(fills[0].price, Price(101));
    assert_eq!(fills[1].price, Price(102));
}
