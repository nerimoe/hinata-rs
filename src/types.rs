use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub(crate) struct HidDevicePathWithoutCom {
    pub read: String,
    pub write: String,
    pub com: OnceLock<String>
}

#[derive(Debug, Clone)]
pub(crate) struct HidDevicePath {
    pub read: String,
    pub write: String,
    pub com: String
}