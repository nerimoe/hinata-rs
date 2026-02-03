use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::thread;
use hidapi::{HidApi, HidDevice, HidError};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use crate::device::{Config, HinataDevice, Info};
use crate::message::{InMessage, OutMessage, Subscription};

const HINATA_VID: u16 = 0xF822;
const USAGE_PAGE_READ: u16 = 1;
const USAGE_PAGE_WRITE: u16 = 0x06;

#[derive(Debug)]
struct HidConnection {
    #[cfg(target_os = "macos")]
    inner: HidDevice,

    #[cfg(not(target_os = "macos"))]
    read: HidDevice,
    #[cfg(not(target_os = "macos"))]
    write: HidDevice,
}

impl HidConnection {
    fn write(&self, data: &[u8]) -> Result<usize, HidError> {
        #[cfg(target_os = "macos")]
        { self.inner.write(data) }

        #[cfg(not(target_os = "macos"))]
        { self.write.write(data) }
    }

    fn read_timeout(&mut self, buf: &mut [u8], timeout_ms: i32) -> Result<usize, HidError> {
        #[cfg(target_os = "macos")]
        { self.inner.read_timeout(buf, timeout_ms) }

        #[cfg(not(target_os = "macos"))]
        { self.read.read_timeout(buf, timeout_ms) }
    }
}

// =========================================================================
// 2. 统一的 Builder 和 核心逻辑
// =========================================================================

#[derive(Debug)]
pub struct HinataDeviceBuilder {
    connection: HidConnection,
    instance_id: String,
}

impl HinataDeviceBuilder {
    pub async fn build(self, debug: bool) -> HinataDevice {
        let (main_to_sub_tx, main_to_sub_rx): (Sender<InMessage>, Receiver<InMessage>) = mpsc::channel(255);

        // 这里的 move 会把 self.connection (包含 underlying HidDevices) 移动进线程
        let handler = thread::spawn(move || {
            Self::io_loop(self.connection, main_to_sub_rx, debug)
        });

        HinataDevice::new(
            Info { firmware_timestamp: 0, firmware_commit_hash: None, chip_id: None },
            Config { sega_brightness: 0, sega_rapid_scan: false },
            Some(handler),
            self.instance_id,
            main_to_sub_tx,
        )
    }

    pub fn get_instance_id(&self) -> String {
        self.instance_id.to_string()
    }

    fn handle_hid_error(subscribes: &mut HashMap<u8, Subscription>, _: HidError) {
        subscribes.drain().for_each(|(_, channel)| {
            let _ = channel.send_no_check(OutMessage::DeviceDisconnect);
        });
    }
    fn io_loop(mut connection: HidConnection, mut message_in: Receiver<InMessage>, debug: bool) {
        let mut buf = [0; 64];
        let mut subscribes: HashMap<u8, Subscription> = HashMap::new();

        loop {
            loop {
                match message_in.try_recv() {
                    Ok(mes) => {
                        let mut data_to_write = None;

                        match mes {
                            InMessage::SendPacket(data) => {
                                data_to_write = Some(data);
                            },
                            InMessage::SendPacketAndSubscribe(data, subscription) => {
                                let key = if data[1] == 1 { 50 } else { data[1] };
                                subscribes.insert(key, subscription);
                                data_to_write = Some(data);
                            }
                            InMessage::Subscribe(cmd, subscription) => {
                                subscribes.insert(cmd, subscription);
                            }
                            InMessage::UnSubscribe(cmd) => {
                                subscribes.remove(&cmd);
                            }
                        }

                        if let Some(data) = data_to_write {
                            match connection.write(&data) {
                                Ok(_) => if debug { println!("DEBUG: -> {:02X?}", data) },
                                Err(e) => Self::handle_hid_error(&mut subscribes, e),
                            }
                        }
                    }
                    Err(e) => match e {
                        mpsc::error::TryRecvError::Empty => break, // 没消息了，去读 HID
                        mpsc::error::TryRecvError::Disconnected => return, // 主线程断开，退出
                    }
                }
            }

            match connection.read_timeout(&mut buf, 16) {
                Ok(len) => if len > 0 {
                    if let Entry::Occupied(mut entry) = subscribes.entry(buf[1]) {
                        if entry.get_mut().send(OutMessage::Response(buf[1..].to_vec())) {
                            entry.remove();
                        }
                    }
                    if debug { println!("DEBUG: <- {:02X?}", &buf[..len]) }
                },
                Err(e) => Self::handle_hid_error(&mut subscribes, e)
            }
        }
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn get_instance(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('#').collect();
    if parts.len() < 3 { return None; }
    let instance_id_full = parts[2];
    if let Some(last_amp_index) = instance_id_full.rfind('&') {
        return Some(instance_id_full[..last_amp_index].to_string());
    }
    Some(instance_id_full.to_string())
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn get_instance(path: &str) -> Option<String> {
    Some(path.to_string())
}

// =========================================================================
// 4. 设备发现逻辑 (仍然需要区分 OS，因为配对逻辑不同)
// =========================================================================

#[cfg(not(target_os = "macos"))]
pub(crate) fn find_devices_inner() -> Result<Vec<HinataDeviceBuilder>, HidError> {
    struct PreDeviceBuilder {
        read: Option<HidDevice>,
        write: Option<HidDevice>
    }

    let hid = HidApi::new()?;
    let mut devices: HashMap<String, PreDeviceBuilder> = HashMap::new();

    for device in hid.device_list() {
        if device.vendor_id() != HINATA_VID { continue; }

        if let Some(key) = get_instance(&device.path().to_string_lossy()) {
            let entry = devices.entry(key).or_insert(PreDeviceBuilder { read: None, write: None });

            if device.usage_page() == USAGE_PAGE_READ {
                entry.read = Some(device.open_device(&hid)?);
            } else if device.usage_page() == USAGE_PAGE_WRITE {
                entry.write = Some(device.open_device(&hid)?);
            }
        }
    }

    Ok(devices.into_iter().filter_map(|(instance, builder)| {
        builder.read.zip(builder.write).map(|(read, write)| {
            HinataDeviceBuilder {
                connection: HidConnection { read, write }, // 使用统一封装
                instance_id: instance,
            }
        })
    }).collect())
}

#[cfg(target_os = "macos")]
pub(crate) fn find_devices_inner() -> Result<Vec<HinataDeviceBuilder>, HidError> {
    let hid = HidApi::new()?;
    let mut devices = Vec::new();

    for device in hid.device_list() {
        // MacOS 通常只需要匹配 Usage Page Write 对应的设备即可打开 IOConnect
        if device.vendor_id() == HINATA_VID && device.usage_page() == USAGE_PAGE_WRITE {
            if let Some(instance) = get_instance(&device.path().to_string_lossy()) {
                if let Ok(rw) = device.open_device(&hid) {
                    devices.push(HinataDeviceBuilder {
                        connection: HidConnection { inner: rw }, // 使用统一封装
                        instance_id: instance,
                    });
                };
            };
        }
    }
    Ok(devices)
}