# libhinata-rs

https://github.com/nerimoe/hinata-rs

A rust library for communicating with HINATA and HINATA Lite

## Usage
```toml
# Cargo.toml
[dependencies]
tokio = { version = "1.49.0", features = ["full"] }
hinata = { git = "https://github.com/nerimoe/hinata-rs" }
```

```rust
// main.rs

#[tokio::main]
async fn main() {
    if let Ok(builders) = hinata::find_devices().await {

        let mut devices = Vec::new();

        for builder in builders {
            devices.push(builder.build(false).await)
        }

        if let Some(device) = devices.get_mut(0) {
            println!("{:?}", device.get_firmware_timestamp().await);
            println!("{:?}", device.pn532().in_list_passive_target(0, 1, &[]).await); // Poll ISO14443-A Card
        }

    }
}
```