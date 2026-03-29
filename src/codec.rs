use crate::errors::{DateTimeError, DecodeLpstrError};
use chrono::{NaiveDate, NaiveTime};
use encoding_rs::WINDOWS_1251;
use libc::{c_char, c_long};
use std::ffi::CStr;
use std::slice;

pub(crate) const MESSAGE_BUFFER_LEN: usize = 256;

/// Decodes a null-terminated Trans2QUIK buffer (windows-1251) into `String`.
pub(crate) fn decode_c_buffer(buffer: &[c_char]) -> String {
    let bytes = c_chars_to_bytes(buffer);
    let null_pos = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());

    let (decoded, _, _) = WINDOWS_1251.decode(&bytes[..null_pos]);
    decoded.into_owned()
}

/// Decodes a Trans2QUIK `LPSTR` (windows-1251) into `String`.
pub(crate) fn decode_lpstr(ptr: *const c_char) -> Result<String, DecodeLpstrError> {
    if ptr.is_null() {
        return Err(DecodeLpstrError::NullPointer);
    }

    // SAFETY: `ptr` проверен на null, контракт Trans2QUIK гарантирует C-строку.
    let c_str = unsafe { CStr::from_ptr(ptr) };
    let bytes = c_str.to_bytes();

    let (decoded, _, had_errors) = WINDOWS_1251.decode(bytes);
    if had_errors {
        return Err(DecodeLpstrError::DecodeError);
    }

    Ok(decoded.into_owned())
}

/// Parses a Trans2QUIK date in `yyyymmdd` format.
pub(crate) fn parse_date(date: c_long) -> Result<NaiveDate, DateTimeError> {
    if date <= 0 {
        return Err(DateTimeError::InvalidDate);
    }

    let date = i32::try_from(date).map_err(|_| DateTimeError::InvalidDate)?;
    let date_str = format!("{date:08}");

    NaiveDate::parse_from_str(&date_str, "%Y%m%d").map_err(DateTimeError::from)
}

/// Parses a Trans2QUIK time in `hhmmss` format.
pub(crate) fn parse_time(time: c_long) -> Result<NaiveTime, DateTimeError> {
    if time <= 0 {
        return Err(DateTimeError::InvalidTime);
    }

    let time = i32::try_from(time).map_err(|_| DateTimeError::InvalidTime)?;
    let time_str = format!("{time:06}");

    NaiveTime::parse_from_str(&time_str, "%H%M%S").map_err(DateTimeError::from)
}

fn c_chars_to_bytes(c_chars: &[c_char]) -> &[u8] {
    // SAFETY: `c_char` и `u8` имеют одинаковый размер в байтах.
    unsafe { slice::from_raw_parts(c_chars.as_ptr().cast::<u8>(), c_chars.len()) }
}

#[cfg(test)]
mod tests {
    use super::{decode_c_buffer, parse_date, parse_time};
    use libc::c_char;

    #[test]
    fn decode_c_buffer_stops_at_nul() {
        let buf = [b'A' as c_char, b'B' as c_char, 0, b'X' as c_char];
        assert_eq!(decode_c_buffer(&buf), "AB");
    }

    #[test]
    fn parse_date_accepts_yyyymmdd() {
        let date = parse_date(20250331).expect("date must parse");
        assert_eq!(date.to_string(), "2025-03-31");
    }

    #[test]
    fn parse_time_accepts_hhmmss() {
        let time = parse_time(93005).expect("time must parse");
        assert_eq!(time.to_string(), "09:30:05");
    }
}
