use chrono::{NaiveDate, NaiveTime};
use libc::c_long;

/// Callback mode received from Trans2QUIK for order/trade streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// New event after subscription start.
    NewOrder,
    /// Event from the initial snapshot batch.
    InitialOrder,
    /// Marker that initial snapshot batch is complete.
    LastOrderReceived,
    /// Value outside known Trans2QUIK mode range.
    Unknown,
}

impl From<c_long> for Mode {
    fn from(code: c_long) -> Self {
        match code {
            0 => Self::NewOrder,
            1 => Self::InitialOrder,
            2 => Self::LastOrderReceived,
            _ => Self::Unknown,
        }
    }
}

/// Transaction identifier attached to an order/trade callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransId {
    /// Known non-zero transaction id.
    Id(c_long),
    /// Missing/unknown transaction id reported by Trans2QUIK.
    Unknown(c_long),
}

impl From<c_long> for TransId {
    fn from(id: c_long) -> Self {
        match id {
            0 => Self::Unknown(id),
            _ => Self::Id(id),
        }
    }
}

/// Side of an order/trade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsSell {
    /// Buy side.
    Buy,
    /// Sell side.
    Sell,
    /// Value outside known Trans2QUIK side range.
    Unknown,
}

impl From<c_long> for IsSell {
    fn from(code: c_long) -> Self {
        match code {
            0 => Self::Buy,
            1 => Self::Sell,
            _ => Self::Unknown,
        }
    }
}

/// Order execution status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Active order.
    Active,
    /// Canceled order.
    Canceled,
    /// Fully executed order.
    Executed,
    /// Value outside known Trans2QUIK status range.
    Unknown,
}

impl From<c_long> for Status {
    fn from(code: c_long) -> Self {
        match code {
            1 => Self::Active,
            2 => Self::Canceled,
            3 => Self::Executed,
            _ => Self::Unknown,
        }
    }
}

/// Return code produced by `Trans2QUIK.dll` functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Trans2QuikResult {
    Success = 0,
    Failed = 1,
    TerminalNotFound = 2,
    DllVersionNotSupported = 3,
    AlreadyConnectedToQuik = 4,
    WrongSyntax = 5,
    QuikNotConnected = 6,
    DllNotConnected = 7,
    QuikConnected = 8,
    QuikDisconnected = 9,
    DllConnected = 10,
    DllDisconnected = 11,
    MemoryAllocationError = 12,
    WrongConnectionHandle = 13,
    WrongInputParams = 14,
    Unknown,
}

impl From<c_long> for Trans2QuikResult {
    fn from(code: c_long) -> Self {
        match code {
            0 => Self::Success,
            1 => Self::Failed,
            2 => Self::TerminalNotFound,
            3 => Self::DllVersionNotSupported,
            4 => Self::AlreadyConnectedToQuik,
            5 => Self::WrongSyntax,
            6 => Self::QuikNotConnected,
            7 => Self::DllNotConnected,
            8 => Self::QuikConnected,
            9 => Self::QuikDisconnected,
            10 => Self::DllConnected,
            11 => Self::DllDisconnected,
            12 => Self::MemoryAllocationError,
            13 => Self::WrongConnectionHandle,
            14 => Self::WrongInputParams,
            _ => Self::Unknown,
        }
    }
}

/// Order payload from Trans2QUIK callback data.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderInfo {
    pub mode: Mode,
    pub trans_id: TransId,
    pub order_num: u64,
    pub class_code: String,
    pub sec_code: String,
    pub price: f64,
    pub balance: i64,
    pub value: f64,
    pub is_sell: IsSell,
    pub status: Status,
    pub date: NaiveDate,
    pub time: NaiveTime,
}

/// Event stream item for order subscriptions.
#[derive(Debug, Clone, PartialEq)]
pub enum OrderEvent {
    /// A regular order callback payload.
    Data(OrderInfo),
    /// Marker that the initial order snapshot is fully received.
    SnapshotEnd,
}

/// Trade payload from Trans2QUIK callback data.
#[derive(Debug, Clone, PartialEq)]
pub struct TradeInfo {
    pub mode: Mode,
    pub trade_num: u64,
    pub order_num: u64,
    pub class_code: String,
    pub sec_code: String,
    pub price: f64,
    pub quantity: i64,
    pub is_sell: IsSell,
    pub value: f64,
    pub date: NaiveDate,
    pub time: NaiveTime,
}

/// Event stream item for trade subscriptions.
#[derive(Debug, Clone, PartialEq)]
pub enum TradeEvent {
    /// A regular trade callback payload.
    Data(TradeInfo),
    /// Marker that the initial trade snapshot is fully received.
    SnapshotEnd,
}

/// Asynchronous transaction reply callback payload.
#[derive(Debug, Clone, PartialEq)]
pub struct TransactionInfo {
    pub trans2quik_result: Trans2QuikResult,
    pub error_code: i32,
    pub reply_code: i32,
    pub trans_id: TransId,
    pub order_num: u64,
    pub reply_message: String,
    pub sec_code: String,
    pub price: f64,
}

#[cfg(test)]
mod tests {
    use super::Trans2QuikResult;

    #[test]
    fn trans2quik_result_conversion_matches_reference() {
        assert_eq!(Trans2QuikResult::from(0), Trans2QuikResult::Success);
        assert_eq!(Trans2QuikResult::from(1), Trans2QuikResult::Failed);
        assert_eq!(
            Trans2QuikResult::from(2),
            Trans2QuikResult::TerminalNotFound
        );
        assert_eq!(
            Trans2QuikResult::from(3),
            Trans2QuikResult::DllVersionNotSupported
        );
        assert_eq!(
            Trans2QuikResult::from(4),
            Trans2QuikResult::AlreadyConnectedToQuik
        );
        assert_eq!(Trans2QuikResult::from(5), Trans2QuikResult::WrongSyntax);
        assert_eq!(
            Trans2QuikResult::from(6),
            Trans2QuikResult::QuikNotConnected
        );
        assert_eq!(Trans2QuikResult::from(7), Trans2QuikResult::DllNotConnected);
        assert_eq!(Trans2QuikResult::from(8), Trans2QuikResult::QuikConnected);
        assert_eq!(
            Trans2QuikResult::from(9),
            Trans2QuikResult::QuikDisconnected
        );
        assert_eq!(Trans2QuikResult::from(10), Trans2QuikResult::DllConnected);
        assert_eq!(
            Trans2QuikResult::from(11),
            Trans2QuikResult::DllDisconnected
        );
        assert_eq!(
            Trans2QuikResult::from(12),
            Trans2QuikResult::MemoryAllocationError
        );
        assert_eq!(
            Trans2QuikResult::from(13),
            Trans2QuikResult::WrongConnectionHandle
        );
        assert_eq!(
            Trans2QuikResult::from(14),
            Trans2QuikResult::WrongInputParams
        );
        assert_eq!(Trans2QuikResult::from(999), Trans2QuikResult::Unknown);
    }
}
