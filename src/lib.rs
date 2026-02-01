pub mod hinata;
pub mod spad0;
pub mod card;
pub mod pn532;
pub mod error;

#[cfg(test)]
mod tests {
    use tokio::task::spawn_blocking;
    use crate::hinata;
    use crate::hinata::find_devices;
    #[tokio::test]
    async fn pair_device() {
        let builders = find_devices().await.unwrap();
        let mut devices = Vec::new();
        for builder in builders {
            devices.push(builder.build(false).await)
        }

        println!("{:?}", devices);
    }

    #[tokio::test]
    async fn pair_device_no_panic() {
        if let Ok(builders) = hinata::find_devices().await {

            let mut devices = Vec::new();

            for builder in builders {
                devices.push(builder.build(false).await)
            }

            if let Some(device) = devices.get_mut(0) {
                println!("{:?}", device.get_firmware_timestamp().await)
            }
        }
    }
}
