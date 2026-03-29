use crate::callbacks::{
    self, CallbackApi, ConnectionStatusCallback, OrderStatusCallback, TradeStatusCallback,
    TransactionReplyCallback,
};
use crate::codec::{MESSAGE_BUFFER_LEN, decode_c_buffer};
use crate::errors::Trans2QuikError;
use crate::types::{OrderEvent, TradeEvent, Trans2QuikResult, TransactionInfo};
use libc::{c_char, c_double, c_long, intptr_t};
use libloading::{Library, Symbol};
use std::ffi::CString;
use tokio::sync::mpsc::UnboundedSender;
use tracing::info;

type ConnectFn = unsafe extern "C" fn(*mut c_char, *mut c_long, *mut c_char, c_long) -> c_long;
type DisconnectFn = unsafe extern "C" fn(*mut c_long, *mut c_char, c_long) -> c_long;
type IsConnectedFn = unsafe extern "C" fn(*mut c_long, *mut c_char, c_long) -> c_long;
type SendSyncTransactionFn = unsafe extern "C" fn(
    trans_str_ptr: *mut c_char,
    reply_code_ptr: *mut c_long,
    trans_id_ptr: *mut c_long,
    order_num_ptr: *mut c_double,
    result_message_ptr: *mut c_char,
    result_message_len: c_long,
    error_code_ptr: *mut c_long,
    error_message_ptr: *mut c_char,
    error_message_len: c_long,
) -> c_long;
type SendAsyncTransactionFn =
    unsafe extern "C" fn(*mut c_char, *mut c_long, *mut c_char, c_long) -> c_long;
type SetConnectionStatusCallbackFn =
    unsafe extern "C" fn(ConnectionStatusCallback, *mut c_long, *mut c_char, c_long) -> c_long;
type SetTransactionsReplyCallbackFn =
    unsafe extern "C" fn(TransactionReplyCallback, *mut c_long, *mut c_char, c_long) -> c_long;
type SubscribeFn = unsafe extern "C" fn(*mut c_char, *mut c_char) -> c_long;
type StartOrdersFn = unsafe extern "C" fn(OrderStatusCallback);
type StartTradesFn = unsafe extern "C" fn(TradeStatusCallback);
type UnsubscribeFn = unsafe extern "C" fn() -> c_long;
type TransactionReplySecCodeFn = unsafe extern "C" fn(intptr_t) -> *mut c_char;
type TransactionReplyPriceFn = unsafe extern "C" fn(intptr_t) -> c_double;
type DescriptorDateTimeFn = unsafe extern "C" fn(intptr_t) -> c_long;

#[derive(Clone, Copy)]
struct Trans2QuikApi {
    connect: ConnectFn,
    disconnect: DisconnectFn,
    is_quik_connected: IsConnectedFn,
    is_dll_connected: IsConnectedFn,
    send_sync_transaction: SendSyncTransactionFn,
    send_async_transaction: SendAsyncTransactionFn,
    set_connection_status_callback: SetConnectionStatusCallbackFn,
    set_transactions_reply_callback: SetTransactionsReplyCallbackFn,
    subscribe_orders: SubscribeFn,
    subscribe_trades: SubscribeFn,
    start_orders: StartOrdersFn,
    start_trades: StartTradesFn,
    unsubscribe_orders: UnsubscribeFn,
    unsubscribe_trades: UnsubscribeFn,
    transaction_reply_sec_code: TransactionReplySecCodeFn,
    transaction_reply_price: TransactionReplyPriceFn,
    order_date: DescriptorDateTimeFn,
    order_time: DescriptorDateTimeFn,
    trade_date: DescriptorDateTimeFn,
    trade_time: DescriptorDateTimeFn,
}

impl Trans2QuikApi {
    fn load(library: &Library) -> Result<Self, Trans2QuikError> {
        macro_rules! load {
            ($symbol:literal, $ty:ty) => {
                load_symbol::<$ty>(library, concat!($symbol, "\0").as_bytes(), $symbol)?
            };
        }

        Ok(Self {
            connect: load!("TRANS2QUIK_CONNECT", ConnectFn),
            disconnect: load!("TRANS2QUIK_DISCONNECT", DisconnectFn),
            is_quik_connected: load!("TRANS2QUIK_IS_QUIK_CONNECTED", IsConnectedFn),
            is_dll_connected: load!("TRANS2QUIK_IS_DLL_CONNECTED", IsConnectedFn),
            send_sync_transaction: load!("TRANS2QUIK_SEND_SYNC_TRANSACTION", SendSyncTransactionFn),
            send_async_transaction: load!(
                "TRANS2QUIK_SEND_ASYNC_TRANSACTION",
                SendAsyncTransactionFn
            ),
            set_connection_status_callback: load!(
                "TRANS2QUIK_SET_CONNECTION_STATUS_CALLBACK",
                SetConnectionStatusCallbackFn
            ),
            set_transactions_reply_callback: load!(
                "TRANS2QUIK_SET_TRANSACTIONS_REPLY_CALLBACK",
                SetTransactionsReplyCallbackFn
            ),
            subscribe_orders: load!("TRANS2QUIK_SUBSCRIBE_ORDERS", SubscribeFn),
            subscribe_trades: load!("TRANS2QUIK_SUBSCRIBE_TRADES", SubscribeFn),
            start_orders: load!("TRANS2QUIK_START_ORDERS", StartOrdersFn),
            start_trades: load!("TRANS2QUIK_START_TRADES", StartTradesFn),
            unsubscribe_orders: load!("TRANS2QUIK_UNSUBSCRIBE_ORDERS", UnsubscribeFn),
            unsubscribe_trades: load!("TRANS2QUIK_UNSUBSCRIBE_TRADES", UnsubscribeFn),
            transaction_reply_sec_code: load!(
                "TRANS2QUIK_TRANSACTION_REPLY_SEC_CODE",
                TransactionReplySecCodeFn
            ),
            // Исправление бага: здесь должен быть именно TRANS2QUIK_TRANSACTION_REPLY_PRICE.
            transaction_reply_price: load!(
                "TRANS2QUIK_TRANSACTION_REPLY_PRICE",
                TransactionReplyPriceFn
            ),
            order_date: load!("TRANS2QUIK_ORDER_DATE", DescriptorDateTimeFn),
            order_time: load!("TRANS2QUIK_ORDER_TIME", DescriptorDateTimeFn),
            trade_date: load!("TRANS2QUIK_TRADE_DATE", DescriptorDateTimeFn),
            trade_time: load!("TRANS2QUIK_TRADE_TIME", DescriptorDateTimeFn),
        })
    }

    fn callback_api(self) -> CallbackApi {
        CallbackApi {
            transaction_reply_sec_code: self.transaction_reply_sec_code,
            transaction_reply_price: self.transaction_reply_price,
            order_date: self.order_date,
            order_time: self.order_time,
            trade_date: self.trade_date,
            trade_time: self.trade_time,
        }
    }
}

/// Terminal handle over a loaded `Trans2QUIK.dll` instance.
pub struct Terminal {
    path_to_quik: CString,
    _library: Library,
    api: Trans2QuikApi,
}

impl Terminal {
    /// Loads the DLL and resolves all required FFI symbols.
    pub fn new(path_to_lib: &str, path_to_quik: &str) -> Result<Self, Trans2QuikError> {
        let path_to_quik = CString::new(path_to_quik)?;

        // SAFETY: путь передан пользователем и используется только для загрузки DLL.
        let library = unsafe { Library::new(path_to_lib)? };
        let api = Trans2QuikApi::load(&library)?;
        callbacks::install_api(api.callback_api())?;

        Ok(Self {
            path_to_quik,
            _library: library,
            api,
        })
    }

    /// Registers a channel for asynchronous transaction reply events.
    pub fn set_transaction_reply_sender(
        &self,
        sender: UnboundedSender<TransactionInfo>,
    ) -> Result<(), Trans2QuikError> {
        callbacks::set_transaction_reply_sender(sender)
    }

    /// Registers a channel for order stream events.
    pub fn set_order_status_sender(
        &self,
        sender: UnboundedSender<OrderEvent>,
    ) -> Result<(), Trans2QuikError> {
        callbacks::set_order_status_sender(sender)
    }

    /// Registers a channel for trade stream events.
    pub fn set_trade_status_sender(
        &self,
        sender: UnboundedSender<TradeEvent>,
    ) -> Result<(), Trans2QuikError> {
        callbacks::set_trade_status_sender(sender)
    }

    /// Fluent variant of [`Self::set_transaction_reply_sender`].
    pub fn with_transaction_reply_sender(
        &self,
        sender: UnboundedSender<TransactionInfo>,
    ) -> Result<&Self, Trans2QuikError> {
        self.set_transaction_reply_sender(sender)?;
        Ok(self)
    }

    /// Fluent variant of [`Self::set_order_status_sender`].
    pub fn with_order_status_sender(
        &self,
        sender: UnboundedSender<OrderEvent>,
    ) -> Result<&Self, Trans2QuikError> {
        self.set_order_status_sender(sender)?;
        Ok(self)
    }

    /// Fluent variant of [`Self::set_trade_status_sender`].
    pub fn with_trade_status_sender(
        &self,
        sender: UnboundedSender<TradeEvent>,
    ) -> Result<&Self, Trans2QuikError> {
        self.set_trade_status_sender(sender)?;
        Ok(self)
    }

    /// Establishes connection between the DLL and QUIK terminal.
    pub fn connect(&self) -> Result<Trans2QuikResult, Trans2QuikError> {
        let path_ptr = self.path_to_quik.as_ptr().cast_mut();
        Ok(self.call_with_error_buffer(
            "TRANS2QUIK_CONNECT",
            |error_code, error_message, error_message_len| {
                // SAFETY: сигнатура и указатели соответствуют контракту DLL.
                unsafe {
                    (self.api.connect)(path_ptr, error_code, error_message, error_message_len)
                }
            },
        ))
    }

    /// Disconnects the DLL from QUIK terminal.
    pub fn disconnect(&self) -> Result<Trans2QuikResult, Trans2QuikError> {
        Ok(self.call_with_error_buffer(
            "TRANS2QUIK_DISCONNECT",
            |error_code, error_message, error_message_len| {
                // SAFETY: сигнатура и указатели соответствуют контракту DLL.
                unsafe { (self.api.disconnect)(error_code, error_message, error_message_len) }
            },
        ))
    }

    /// Checks whether QUIK terminal is connected to QUIK server.
    pub fn is_quik_connected(&self) -> Result<Trans2QuikResult, Trans2QuikError> {
        Ok(self.call_with_error_buffer(
            "TRANS2QUIK_IS_QUIK_CONNECTED",
            |error_code, error_message, error_message_len| {
                // SAFETY: сигнатура и указатели соответствуют контракту DLL.
                unsafe {
                    (self.api.is_quik_connected)(error_code, error_message, error_message_len)
                }
            },
        ))
    }

    /// Checks whether the DLL is connected to QUIK terminal.
    pub fn is_dll_connected(&self) -> Result<Trans2QuikResult, Trans2QuikError> {
        Ok(self.call_with_error_buffer(
            "TRANS2QUIK_IS_DLL_CONNECTED",
            |error_code, error_message, error_message_len| {
                // SAFETY: сигнатура и указатели соответствуют контракту DLL.
                unsafe { (self.api.is_dll_connected)(error_code, error_message, error_message_len) }
            },
        ))
    }

    /// Sends a synchronous transaction string.
    ///
    /// The call returns only after Trans2QUIK receives a server response
    /// (or connection is lost).
    pub fn send_sync_transaction(
        &self,
        transaction_str: &str,
    ) -> Result<Trans2QuikResult, Trans2QuikError> {
        let trans_str = CString::new(transaction_str)?;
        let trans_str_ptr = trans_str.as_ptr().cast_mut();

        let mut reply_code: c_long = 0;
        let mut trans_id: c_long = 0;
        let mut order_num: c_double = 0.0;

        let mut result_message = [0 as c_char; MESSAGE_BUFFER_LEN];
        let mut error_code: c_long = 0;
        let mut error_message = [0 as c_char; MESSAGE_BUFFER_LEN];

        // SAFETY: сигнатура и указатели соответствуют контракту DLL.
        let function_result = unsafe {
            (self.api.send_sync_transaction)(
                trans_str_ptr,
                &mut reply_code,
                &mut trans_id,
                &mut order_num,
                result_message.as_mut_ptr(),
                result_message.len() as c_long,
                &mut error_code,
                error_message.as_mut_ptr(),
                error_message.len() as c_long,
            )
        };

        let result_message = decode_c_buffer(&result_message);
        let error_message = decode_c_buffer(&error_message);
        let trans2quik_result = Trans2QuikResult::from(function_result);

        info!(
            "TRANS2QUIK_SEND_SYNC_TRANSACTION -> {:?}, reply_code: {}, trans_id: {}, order_num: {}, result_message: {}, error_code: {}, error_message: {}",
            trans2quik_result,
            reply_code,
            trans_id,
            order_num,
            result_message,
            error_code,
            error_message,
        );

        Ok(trans2quik_result)
    }

    /// Sends an asynchronous transaction string.
    ///
    /// The call returns immediately, while the result is delivered through
    /// the transaction reply callback channel.
    pub fn send_async_transaction(
        &self,
        transaction_str: &str,
    ) -> Result<Trans2QuikResult, Trans2QuikError> {
        let trans_str = CString::new(transaction_str)?;
        let trans_str_ptr = trans_str.as_ptr().cast_mut();

        Ok(self.call_with_error_buffer(
            "TRANS2QUIK_SEND_ASYNC_TRANSACTION",
            |error_code, error_message, error_message_len| {
                // SAFETY: сигнатура и указатели соответствуют контракту DLL.
                unsafe {
                    (self.api.send_async_transaction)(
                        trans_str_ptr,
                        error_code,
                        error_message,
                        error_message_len,
                    )
                }
            },
        ))
    }

    /// Installs the connection status callback in Trans2QUIK.
    pub fn set_connection_status_callback(&self) -> Result<Trans2QuikResult, Trans2QuikError> {
        Ok(self.call_with_error_buffer(
            "TRANS2QUIK_SET_CONNECTION_STATUS_CALLBACK",
            |error_code, error_message, error_message_len| {
                // SAFETY: сигнатура и указатели соответствуют контракту DLL.
                unsafe {
                    (self.api.set_connection_status_callback)(
                        callbacks::connection_status_callback,
                        error_code,
                        error_message,
                        error_message_len,
                    )
                }
            },
        ))
    }

    /// Installs the transaction reply callback in Trans2QUIK.
    ///
    /// According to Trans2QUIK API contract, asynchronous callback-based flow
    /// should not be mixed with synchronous processing of the same transaction stream.
    pub fn set_transactions_reply_callback(&self) -> Result<Trans2QuikResult, Trans2QuikError> {
        Ok(self.call_with_error_buffer(
            "TRANS2QUIK_SET_TRANSACTIONS_REPLY_CALLBACK",
            |error_code, error_message, error_message_len| {
                // SAFETY: сигнатура и указатели соответствуют контракту DLL.
                unsafe {
                    (self.api.set_transactions_reply_callback)(
                        callbacks::transaction_reply_callback,
                        error_code,
                        error_message,
                        error_message_len,
                    )
                }
            },
        ))
    }

    /// Subscribes to order callbacks for `(class_code, sec_code)`.
    pub fn subscribe_orders(
        &self,
        class_code: &str,
        sec_code: &str,
    ) -> Result<Trans2QuikResult, Trans2QuikError> {
        self.subscribe_impl(
            "TRANS2QUIK_SUBSCRIBE_ORDERS",
            self.api.subscribe_orders,
            class_code,
            sec_code,
        )
    }

    /// Subscribes to trade callbacks for `(class_code, sec_code)`.
    pub fn subscribe_trades(
        &self,
        class_code: &str,
        sec_code: &str,
    ) -> Result<Trans2QuikResult, Trans2QuikError> {
        self.subscribe_impl(
            "TRANS2QUIK_SUBSCRIBE_TRADES",
            self.api.subscribe_trades,
            class_code,
            sec_code,
        )
    }

    /// Starts order callback stream delivery.
    ///
    /// Call [`Self::set_order_status_sender`] before starting the stream.
    pub fn start_orders(&self) {
        // SAFETY: callback имеет корректную ABI и lifetime `'static`.
        unsafe { (self.api.start_orders)(callbacks::order_status_callback) }
    }

    /// Starts trade callback stream delivery.
    ///
    /// Call [`Self::set_trade_status_sender`] before starting the stream.
    pub fn start_trades(&self) {
        // SAFETY: callback имеет корректную ABI и lifetime `'static`.
        unsafe { (self.api.start_trades)(callbacks::trade_status_callback) }
    }

    /// Stops order callback stream and clears server-side subscription list.
    pub fn unsubscribe_orders(&self) -> Result<Trans2QuikResult, Trans2QuikError> {
        Ok(self.call_without_args("TRANS2QUIK_UNSUBSCRIBE_ORDERS", self.api.unsubscribe_orders))
    }

    /// Stops trade callback stream and clears server-side subscription list.
    pub fn unsubscribe_trades(&self) -> Result<Trans2QuikResult, Trans2QuikError> {
        Ok(self.call_without_args("TRANS2QUIK_UNSUBSCRIBE_TRADES", self.api.unsubscribe_trades))
    }

    fn call_with_error_buffer<F>(&self, function_name: &str, func: F) -> Trans2QuikResult
    where
        F: FnOnce(*mut c_long, *mut c_char, c_long) -> c_long,
    {
        let mut error_code: c_long = 0;
        let mut error_message = [0 as c_char; MESSAGE_BUFFER_LEN];

        let function_result = func(
            &mut error_code,
            error_message.as_mut_ptr(),
            error_message.len() as c_long,
        );

        let error_message = decode_c_buffer(&error_message);
        let trans2quik_result = Trans2QuikResult::from(function_result);

        info!(
            "{} -> {:?}, error_code: {}, error_message: {}",
            function_name, trans2quik_result, error_code, error_message
        );

        trans2quik_result
    }

    fn call_without_args(&self, function_name: &str, func: UnsubscribeFn) -> Trans2QuikResult {
        // SAFETY: сигнатура функции проверена при загрузке символа.
        let function_result = unsafe { func() };
        let trans2quik_result = Trans2QuikResult::from(function_result);

        info!("{} -> {:?}", function_name, trans2quik_result);

        trans2quik_result
    }

    fn subscribe_impl(
        &self,
        function_name: &str,
        func: SubscribeFn,
        class_code: &str,
        sec_code: &str,
    ) -> Result<Trans2QuikResult, Trans2QuikError> {
        let class_code = CString::new(class_code)?;
        let sec_code = CString::new(sec_code)?;

        // SAFETY: сигнатура функции проверена при загрузке символа.
        let function_result =
            unsafe { func(class_code.as_ptr().cast_mut(), sec_code.as_ptr().cast_mut()) };
        let trans2quik_result = Trans2QuikResult::from(function_result);

        info!(
            "{} -> {:?}, class_code: {}, sec_code: {}",
            function_name,
            trans2quik_result,
            class_code.to_string_lossy(),
            sec_code.to_string_lossy(),
        );

        Ok(trans2quik_result)
    }
}

fn load_symbol<T>(
    library: &Library,
    name: &'static [u8],
    debug_name: &'static str,
) -> Result<T, Trans2QuikError>
where
    T: Copy,
{
    // SAFETY: `name` — null-terminated имя символа, ABI и тип проверяются пользователем API.
    unsafe {
        let symbol: Symbol<T> = library
            .get(name)
            .map_err(|err| Trans2QuikError::symbol_load(debug_name, err))?;
        Ok(*symbol)
    }
}
