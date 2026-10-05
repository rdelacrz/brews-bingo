use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use zeroize::Zeroizing;

pub(crate) fn link_token() -> Zeroizing<String> {
    let mut bytes = Zeroizing::new([0u8; 32]);
    bytes[..16].copy_from_slice(uuid::Uuid::now_v7().as_bytes());
    bytes[16..].copy_from_slice(uuid::Uuid::now_v7().as_bytes());
    Zeroizing::new(URL_SAFE_NO_PAD.encode(bytes.as_slice()))
}

pub(crate) fn enrollment_url() -> Zeroizing<String> {
    Zeroizing::new(format!("https://example.test/enroll#{}", *link_token()))
}
