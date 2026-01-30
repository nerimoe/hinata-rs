pub mod hinata;
pub mod spad0;
pub mod card;
pub mod pn532;
pub mod error;

#[cfg(test)]
mod tests {
    use tokio::task::spawn_blocking;
    use crate::hinata::{Hinata};
    #[tokio::test]
    async fn pair_device() {
        let builders = spawn_blocking(|| {
            let api = Hinata::new();
            api.find_devices()
        }).await.unwrap().unwrap();
        let mut devices = Vec::new();
        for x in builders {
            devices.push(x.build(false).await)
        }
        println!("{:?}", devices);
    }
}
