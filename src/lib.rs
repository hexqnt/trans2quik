//! Rust bindings for `Trans2QUIK.dll`.
//!
//! The crate exposes a compact, strongly-typed API for:
//! - establishing and checking terminal connectivity;
//! - sending synchronous and asynchronous transactions;
//! - subscribing to order and trade callback streams;
//! - decoding callback payloads into domain events.
//!
#[cfg(not(windows))]
compile_error!("trans2quik supports only Windows targets (uses Trans2QUIK.dll).");

mod callbacks;
mod codec;
mod errors;
mod terminal;
mod types;

pub use errors::Trans2QuikError;
pub use terminal::Terminal;
pub use types::{
    IsSell, Mode, OrderEvent, OrderInfo, Status, TradeEvent, TradeInfo, Trans2QuikResult, TransId,
    TransactionInfo,
};
