//! NFC types, re-exported from the client framework.

pub use bloop_client_framework::nfc::NfcReader;
#[cfg(feature = "hardware-emulation")]
pub use bloop_client_framework::nfc::NfcReaderRequest;
pub use bloop_protocol::NfcUid;
