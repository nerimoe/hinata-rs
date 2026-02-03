use tokio::task::spawn_blocking;
use error::Error;
use crate::hinata::builder::{find_devices_inner, HinataDeviceBuilder};

mod message;
mod builder;
pub mod device;
pub mod card;
pub mod pn532;
pub mod error;

pub async fn find_devices() -> Result<Vec<HinataDeviceBuilder>, Error> {
    spawn_blocking(|| find_devices_inner().map_err(|_| Error::NotFound("Device not found".to_string()))).await.map_err(|e| Error::Other(e.to_string()))?
}