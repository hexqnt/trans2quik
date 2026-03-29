use chrono::ParseError as ChronoParseError;
use libloading::Error as LibloadingError;
use std::error;
use std::ffi::NulError;
use std::fmt;

/// Error type returned by the public API.
#[derive(Debug)]
pub enum Trans2QuikError {
    /// Failed to load the dynamic library itself.
    LibLoading(LibloadingError),
    /// Failed to resolve a required symbol from the dynamic library.
    SymbolLoad {
        /// Symbol name expected by the binding.
        symbol: &'static str,
        /// Original loader error.
        source: LibloadingError,
    },
    /// Input string contains an interior NUL byte and cannot be passed to C ABI.
    NulError(NulError),
    /// Internal callback registry mutex is poisoned.
    CallbackStatePoisoned,
}

impl Trans2QuikError {
    pub(crate) fn symbol_load(symbol: &'static str, source: LibloadingError) -> Self {
        Self::SymbolLoad { symbol, source }
    }
}

impl fmt::Display for Trans2QuikError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LibLoading(err) => write!(f, "Library loading error: {err}"),
            Self::SymbolLoad { symbol, source } => {
                write!(f, "Failed to load symbol {symbol}: {source}")
            }
            Self::NulError(err) => write!(f, "Nul byte in input string: {err}"),
            Self::CallbackStatePoisoned => write!(f, "Callback state is poisoned"),
        }
    }
}

impl error::Error for Trans2QuikError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::LibLoading(err) => Some(err),
            Self::SymbolLoad { source, .. } => Some(source),
            Self::NulError(err) => Some(err),
            Self::CallbackStatePoisoned => None,
        }
    }
}

impl From<LibloadingError> for Trans2QuikError {
    fn from(err: LibloadingError) -> Self {
        Self::LibLoading(err)
    }
}

impl From<NulError> for Trans2QuikError {
    fn from(err: NulError) -> Self {
        Self::NulError(err)
    }
}

#[derive(Debug)]
pub(crate) enum DecodeLpstrError {
    NullPointer,
    DecodeError,
}

impl fmt::Display for DecodeLpstrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NullPointer => write!(f, "Null pointer string"),
            Self::DecodeError => write!(f, "Failed to decode windows-1251 string"),
        }
    }
}

impl error::Error for DecodeLpstrError {}

#[derive(Debug)]
pub(crate) enum DateTimeError {
    InvalidDate,
    InvalidTime,
    ParseError(ChronoParseError),
}

impl fmt::Display for DateTimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDate => write!(f, "Invalid trade date"),
            Self::InvalidTime => write!(f, "Invalid trade time"),
            Self::ParseError(err) => write!(f, "Failed to parse date/time: {err}"),
        }
    }
}

impl error::Error for DateTimeError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::ParseError(err) => Some(err),
            Self::InvalidDate | Self::InvalidTime => None,
        }
    }
}

impl From<ChronoParseError> for DateTimeError {
    fn from(err: ChronoParseError) -> Self {
        Self::ParseError(err)
    }
}
