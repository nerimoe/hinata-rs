use std::cell::{OnceCell};

#[derive(Debug, Clone)]
pub(crate) struct HidDevicePathWithoutCom {
    pub read: String,
    pub write: String,
    pub com: OnceCell<String>
}

#[derive(Debug, Clone)]
pub(crate) struct HidDevicePath {
    pub read: String,
    pub write: String,
    pub com: String
}