use std::cell::RefCell;

#[derive(Debug, Clone)]
pub(crate) struct HidDevicePathWithoutCom {
    pub read: String,
    pub write: String,
    pub com: RefCell<Option<String>>
}

#[derive(Debug, Clone)]
pub(crate) struct HidDevicePath {
    pub read: String,
    pub write: String,
    pub com: String
}