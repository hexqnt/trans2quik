use crate::codec::{decode_lpstr, parse_date, parse_time};
use crate::errors::Trans2QuikError;
use crate::types::{
    IsSell, Mode, OrderEvent, OrderInfo, Status, TradeEvent, TradeInfo, Trans2QuikResult, TransId,
    TransactionInfo,
};
use libc::{c_char, c_double, c_long, c_ulonglong, intptr_t};
use std::sync::{Mutex, OnceLock};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, info};

pub(crate) type ConnectionStatusCallback =
    unsafe extern "C" fn(connection_event: c_long, error_code: c_long, error_message: *mut c_char);

pub(crate) type TransactionReplyCallback = unsafe extern "C" fn(
    result_code: c_long,
    error_code: c_long,
    reply_code: c_long,
    trans_id: c_long,
    order_num: c_ulonglong,
    reply_message: *mut c_char,
    trans_reply_descriptor: intptr_t,
);

pub(crate) type OrderStatusCallback = unsafe extern "C" fn(
    mode: c_long,
    trans_id: c_long,
    order_num: c_ulonglong,
    class_code: *mut c_char,
    sec_code: *mut c_char,
    price: c_double,
    balance: i64,
    value: c_double,
    is_sell: c_long,
    status: c_long,
    order_descriptor: intptr_t,
);

pub(crate) type TradeStatusCallback = unsafe extern "C" fn(
    mode: c_long,
    trade_num: c_ulonglong,
    order_num: c_ulonglong,
    class_code: *mut c_char,
    sec_code: *mut c_char,
    price: c_double,
    quantity: i64,
    is_sell: c_long,
    value: c_double,
    trade_descriptor: intptr_t,
);

type TransactionReplySecCodeFn =
    unsafe extern "C" fn(trans_reply_descriptor: intptr_t) -> *mut c_char;
type TransactionReplyPriceFn = unsafe extern "C" fn(trans_reply_descriptor: intptr_t) -> c_double;
type OrderDateFn = unsafe extern "C" fn(order_descriptor: intptr_t) -> c_long;
type OrderTimeFn = unsafe extern "C" fn(order_descriptor: intptr_t) -> c_long;
type TradeDateFn = unsafe extern "C" fn(trade_descriptor: intptr_t) -> c_long;
type TradeTimeFn = unsafe extern "C" fn(trade_descriptor: intptr_t) -> c_long;

#[derive(Clone, Copy)]
pub(crate) struct CallbackApi {
    pub(crate) transaction_reply_sec_code: TransactionReplySecCodeFn,
    pub(crate) transaction_reply_price: TransactionReplyPriceFn,
    pub(crate) order_date: OrderDateFn,
    pub(crate) order_time: OrderTimeFn,
    pub(crate) trade_date: TradeDateFn,
    pub(crate) trade_time: TradeTimeFn,
}

#[derive(Default)]
struct CallbackState {
    api: Option<CallbackApi>,
    transaction_reply_sender: Option<UnboundedSender<TransactionInfo>>,
    order_status_sender: Option<UnboundedSender<OrderEvent>>,
    trade_status_sender: Option<UnboundedSender<TradeEvent>>,
}

static CALLBACK_STATE: OnceLock<Mutex<CallbackState>> = OnceLock::new();

fn state() -> &'static Mutex<CallbackState> {
    CALLBACK_STATE.get_or_init(|| Mutex::new(CallbackState::default()))
}

fn snapshot<T>(project: impl FnOnce(&CallbackState) -> T) -> Option<T> {
    match state().lock() {
        Ok(guard) => Some(project(&guard)),
        Err(_) => {
            error!("Callback state is poisoned");
            None
        }
    }
}

pub(crate) fn install_api(api: CallbackApi) -> Result<(), Trans2QuikError> {
    let mut guard = state()
        .lock()
        .map_err(|_| Trans2QuikError::CallbackStatePoisoned)?;
    guard.api = Some(api);
    Ok(())
}

pub(crate) fn set_transaction_reply_sender(
    sender: UnboundedSender<TransactionInfo>,
) -> Result<(), Trans2QuikError> {
    let mut guard = state()
        .lock()
        .map_err(|_| Trans2QuikError::CallbackStatePoisoned)?;
    guard.transaction_reply_sender = Some(sender);
    Ok(())
}

pub(crate) fn set_order_status_sender(
    sender: UnboundedSender<OrderEvent>,
) -> Result<(), Trans2QuikError> {
    let mut guard = state()
        .lock()
        .map_err(|_| Trans2QuikError::CallbackStatePoisoned)?;
    guard.order_status_sender = Some(sender);
    Ok(())
}

pub(crate) fn set_trade_status_sender(
    sender: UnboundedSender<TradeEvent>,
) -> Result<(), Trans2QuikError> {
    let mut guard = state()
        .lock()
        .map_err(|_| Trans2QuikError::CallbackStatePoisoned)?;
    guard.trade_status_sender = Some(sender);
    Ok(())
}

fn decode_field(ptr: *mut c_char, field_name: &str) -> Option<String> {
    match decode_lpstr(ptr.cast_const()) {
        Ok(value) => Some(value),
        Err(err) => {
            error!("Failed to decode {field_name}: {err}");
            None
        }
    }
}

fn c_long_to_i32(value: c_long, field_name: &str) -> Option<i32> {
    match i32::try_from(value) {
        Ok(value) => Some(value),
        Err(_) => {
            error!("{field_name} value {value} does not fit into i32");
            None
        }
    }
}

pub(crate) unsafe extern "C" fn connection_status_callback(
    connection_event: c_long,
    error_code: c_long,
    error_message: *mut c_char,
) {
    let error_message = if error_message.is_null() {
        String::from("error_message is null")
    } else {
        match decode_lpstr(error_message.cast_const()) {
            Ok(message) => message,
            Err(err) => {
                error!("Failed to decode error_message: {err}");
                String::from("failed to decode error_message")
            }
        }
    };

    let trans2quik_result = Trans2QuikResult::from(connection_event);
    info!(
        "TRANS2QUIK_CONNECTION_STATUS_CALLBACK -> {:?}, error_code: {}, error_message: {}",
        trans2quik_result, error_code, error_message
    );
}

pub(crate) unsafe extern "C" fn transaction_reply_callback(
    result_code: c_long,
    error_code: c_long,
    reply_code: c_long,
    trans_id: c_long,
    order_num: c_ulonglong,
    reply_message: *mut c_char,
    trans_reply_descriptor: intptr_t,
) {
    let Some((api, sender)) = snapshot(|state| (state.api, state.transaction_reply_sender.clone()))
    else {
        return;
    };

    let Some(api) = api else {
        error!("Callback API is not initialized");
        return;
    };

    let reply_message = match decode_field(reply_message, "reply_message") {
        Some(message) => message,
        None => return,
    };

    // SAFETY: функция и дескриптор получены из валидной таблицы символов Trans2QUIK.
    let sec_code_ptr = unsafe { (api.transaction_reply_sec_code)(trans_reply_descriptor) };
    let sec_code = match decode_field(sec_code_ptr, "sec_code") {
        Some(sec_code) => sec_code,
        None => return,
    };

    // SAFETY: функция и дескриптор получены из валидной таблицы символов Trans2QUIK.
    let price = unsafe { (api.transaction_reply_price)(trans_reply_descriptor) };

    let Some(error_code) = c_long_to_i32(error_code, "error_code") else {
        return;
    };
    let Some(reply_code) = c_long_to_i32(reply_code, "reply_code") else {
        return;
    };

    let transaction_info = TransactionInfo {
        trans2quik_result: Trans2QuikResult::from(result_code),
        error_code,
        reply_code,
        trans_id: TransId::from(trans_id),
        order_num,
        reply_message,
        sec_code,
        price,
    };

    info!(
        "TRANS2QUIK_TRANSACTION_REPLY_CALLBACK -> {:?}, error_code: {}, reply_code: {}, trans_id: {:?}, order_num: {}, reply_message: {}, sec_code: {}, price: {}",
        transaction_info.trans2quik_result,
        transaction_info.error_code,
        transaction_info.reply_code,
        transaction_info.trans_id,
        transaction_info.order_num,
        transaction_info.reply_message,
        transaction_info.sec_code,
        transaction_info.price,
    );

    match sender {
        Some(sender) => {
            if let Err(err) = sender.send(transaction_info) {
                error!("Failed to send transaction callback event: {err}");
            }
        }
        None => {
            error!("TRANSACTION_REPLY_SENDER is not initialized");
        }
    }
}

pub(crate) unsafe extern "C" fn order_status_callback(
    mode: c_long,
    trans_id: c_long,
    order_num: c_ulonglong,
    class_code: *mut c_char,
    sec_code: *mut c_char,
    price: c_double,
    balance: i64,
    value: c_double,
    is_sell: c_long,
    status: c_long,
    order_descriptor: intptr_t,
) {
    let Some((api, sender)) = snapshot(|state| (state.api, state.order_status_sender.clone()))
    else {
        return;
    };

    let Some(api) = api else {
        error!("Callback API is not initialized");
        return;
    };

    let mode = Mode::from(mode);
    if matches!(mode, Mode::LastOrderReceived) {
        info!("TRANS2QUIK_ORDER_STATUS_CALLBACK -> snapshot end");
        match sender {
            Some(sender) => {
                if let Err(err) = sender.send(OrderEvent::SnapshotEnd) {
                    error!("Failed to send order callback event: {err}");
                }
            }
            None => {
                error!("ORDER_STATUS_SENDER is not initialized");
            }
        }
        return;
    }

    let class_code = match decode_field(class_code, "class_code") {
        Some(class_code) => class_code,
        None => return,
    };
    let sec_code = match decode_field(sec_code, "sec_code") {
        Some(sec_code) => sec_code,
        None => return,
    };

    // SAFETY: функция и дескриптор получены из валидной таблицы символов Trans2QUIK.
    let date_raw = unsafe { (api.order_date)(order_descriptor) };
    let date = match parse_date(date_raw) {
        Ok(date) => date,
        Err(err) => {
            error!("Failed to parse order date: {err}");
            return;
        }
    };

    // SAFETY: функция и дескриптор получены из валидной таблицы символов Trans2QUIK.
    let time_raw = unsafe { (api.order_time)(order_descriptor) };
    let time = match parse_time(time_raw) {
        Ok(time) => time,
        Err(err) => {
            error!("Failed to parse order time: {err}");
            return;
        }
    };

    let order_info = OrderInfo {
        mode,
        trans_id: TransId::from(trans_id),
        order_num,
        class_code,
        sec_code,
        price,
        balance,
        value,
        is_sell: IsSell::from(is_sell),
        status: Status::from(status),
        date,
        time,
    };

    info!(
        "TRANS2QUIK_ORDER_STATUS_CALLBACK -> mode: {:?}, trans_id: {:?}, order_num: {}, class_code: {}, sec_code: {}, price: {}, balance: {}, value: {}, is_sell: {:?}, status: {:?}, date: {}, time: {}",
        order_info.mode,
        order_info.trans_id,
        order_info.order_num,
        order_info.class_code,
        order_info.sec_code,
        order_info.price,
        order_info.balance,
        order_info.value,
        order_info.is_sell,
        order_info.status,
        order_info.date,
        order_info.time,
    );

    match sender {
        Some(sender) => {
            if let Err(err) = sender.send(OrderEvent::Data(order_info)) {
                error!("Failed to send order callback event: {err}");
            }
        }
        None => {
            error!("ORDER_STATUS_SENDER is not initialized");
        }
    }
}

pub(crate) unsafe extern "C" fn trade_status_callback(
    mode: c_long,
    trade_num: c_ulonglong,
    order_num: c_ulonglong,
    class_code: *mut c_char,
    sec_code: *mut c_char,
    price: c_double,
    quantity: i64,
    is_sell: c_long,
    value: c_double,
    trade_descriptor: intptr_t,
) {
    let Some((api, sender)) = snapshot(|state| (state.api, state.trade_status_sender.clone()))
    else {
        return;
    };

    let Some(api) = api else {
        error!("Callback API is not initialized");
        return;
    };

    let mode = Mode::from(mode);
    if matches!(mode, Mode::LastOrderReceived) {
        info!("TRANS2QUIK_TRADE_STATUS_CALLBACK -> snapshot end");
        match sender {
            Some(sender) => {
                if let Err(err) = sender.send(TradeEvent::SnapshotEnd) {
                    error!("Failed to send trade callback event: {err}");
                }
            }
            None => {
                error!("TRADE_STATUS_SENDER is not initialized");
            }
        }
        return;
    }

    let class_code = match decode_field(class_code, "class_code") {
        Some(class_code) => class_code,
        None => return,
    };
    let sec_code = match decode_field(sec_code, "sec_code") {
        Some(sec_code) => sec_code,
        None => return,
    };

    // SAFETY: функция и дескриптор получены из валидной таблицы символов Trans2QUIK.
    let date_raw = unsafe { (api.trade_date)(trade_descriptor) };
    let date = match parse_date(date_raw) {
        Ok(date) => date,
        Err(err) => {
            error!("Failed to parse trade date: {err}");
            return;
        }
    };

    // SAFETY: функция и дескриптор получены из валидной таблицы символов Trans2QUIK.
    let time_raw = unsafe { (api.trade_time)(trade_descriptor) };
    let time = match parse_time(time_raw) {
        Ok(time) => time,
        Err(err) => {
            error!("Failed to parse trade time: {err}");
            return;
        }
    };

    let trade_info = TradeInfo {
        mode,
        trade_num,
        order_num,
        class_code,
        sec_code,
        price,
        quantity,
        is_sell: IsSell::from(is_sell),
        value,
        date,
        time,
    };

    info!(
        "TRANS2QUIK_TRADE_STATUS_CALLBACK -> mode: {:?}, trade_num: {}, order_num: {}, class_code: {}, sec_code: {}, price: {}, quantity: {}, is_sell: {:?}, value: {}, date: {}, time: {}",
        trade_info.mode,
        trade_info.trade_num,
        trade_info.order_num,
        trade_info.class_code,
        trade_info.sec_code,
        trade_info.price,
        trade_info.quantity,
        trade_info.is_sell,
        trade_info.value,
        trade_info.date,
        trade_info.time,
    );

    match sender {
        Some(sender) => {
            if let Err(err) = sender.send(TradeEvent::Data(trade_info)) {
                error!("Failed to send trade callback event: {err}");
            }
        }
        None => {
            error!("TRADE_STATUS_SENDER is not initialized");
        }
    }
}
