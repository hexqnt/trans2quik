# trans2quik

Library for importing transactions and placing orders in QUIK via `Trans2QUIK.dll`.

## Usage example

```rust
use tokio::sync::mpsc;
use trans2quik::{OrderEvent, Terminal, TradeEvent, TransactionInfo};

fn setup_terminal() -> Result<Terminal, Box<dyn std::error::Error>> {
    let dll_path = r"C:\QUIK\trans2quik.dll";
    let quik_path = r"C:\QUIK";
    let class_code = "TQBR";
    let sec_code = "SBER";

    let terminal = Terminal::new(dll_path, quik_path)?;

    let (tx_transactions, _rx_transactions) = mpsc::unbounded_channel::<TransactionInfo>();
    let (tx_orders, _rx_orders) = mpsc::unbounded_channel::<OrderEvent>();
    let (tx_trades, _rx_trades) = mpsc::unbounded_channel::<TradeEvent>();

    terminal
        .with_transaction_reply_sender(tx_transactions)?
        .with_order_status_sender(tx_orders)?
        .with_trade_status_sender(tx_trades)?;

    terminal.connect()?;
    terminal.is_dll_connected()?;
    terminal.is_quik_connected()?;
    terminal.set_connection_status_callback()?;
    terminal.set_transactions_reply_callback()?;
    terminal.subscribe_orders(class_code, sec_code)?;
    terminal.subscribe_trades(class_code, sec_code)?;
    terminal.start_orders();
    terminal.start_trades();

    Ok(terminal)
}
```
