# libhinata-rs

A rust library for communicating with HINATA and HINATA Lite 

example:
```rust
#[tokio::main]
async fn main() {
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
```