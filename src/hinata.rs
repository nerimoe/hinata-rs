use std::collections::hash_map::Entry;
use std::collections::HashMap;
use async_trait::async_trait;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::mpsc;
use std::thread;
use std::thread::{JoinHandle};
use std::time::Duration;
use hidapi::{HidApi, HidDevice, HidError};
use tokio::task::spawn_blocking;
use crate::error::Error;
use crate::pn532::{ Pn532, Pn532Command, Pn532Direction, Pn532Packet, Pn532Port};

const HINATA_VID: u16 = 0xF822;
const USAGE_PAGE_READ: u16 = 1;
const USAGE_PAGE_WRITE: u16 = 0x06;


#[derive(Debug)]
struct Info {
    firmware_timestamp: u32,
    firmware_commit_hash: Option<[u8; 4]>,
    chip_id: Option<[u8; 4]>
}

#[derive(Debug)]
struct Config {
    sega_brightness: u8,
    sega_rapid_scan: bool,
}

// --- Communication Internals ---

enum InMessage {
    SendPacket(Vec<u8>),
    SendPacketAndSubscribe(Vec<u8>, Subscription),
    Subscribe(u8, Subscription),
    UnSubscribe(u8)
}

#[derive(Debug)]
enum OutMessage {
    Response(Vec<u8>),
    DeviceDisconnect,
}

enum UnSubscribePolicy {
    Count(usize),
    Never,
    SpecificIsOn(usize, u8),
    SpecificNotOn(usize, u8)
}

impl UnSubscribePolicy {
    pub fn need_dispose(&self, msg: &OutMessage, count: usize) -> bool {
        if let OutMessage::Response(packet) = msg {
            match self {
                UnSubscribePolicy::Count(n) => count >= *n,
                UnSubscribePolicy::Never => false,
                UnSubscribePolicy::SpecificIsOn(index, byte) => if let Some(b) = packet.get(*index) {
                    if b == byte {
                        true
                    } else {
                        false
                    }
                } else {
                    true
                },
                UnSubscribePolicy::SpecificNotOn(index, byte) => if let Some(b) = packet.get(*index) {
                    if b != byte {
                        true
                    } else {
                        false
                    }
                } else {
                    true
                }
            }
        } else {
            true
        }
    }
}

struct Subscription {
    sender: Sender<OutMessage>,
    policy: UnSubscribePolicy,
    count: usize
}

impl Subscription {
    fn new(policy: UnSubscribePolicy) -> (Self, Receiver<OutMessage>) {
        let (sender, receiver) = mpsc::channel::<OutMessage>(32);
        (
            Self {
                sender,
                policy,
                count: 0,
            },
            receiver
        )
    }
    fn send(&mut self, msg: OutMessage) -> bool {
        self.count = self.count + 1;
        let mut need_dispose = self.policy.need_dispose(&msg, self.count);
        if let Err(_) = self.sender.blocking_send(msg) { need_dispose = true }
        need_dispose
    }

    fn send_no_check(&self, msg: OutMessage) {
        let _ = self.sender.blocking_send(msg);
    }
}

// --- Device Implementation ---

#[derive(Debug)]
pub struct HinataDevice {
    info: Info,
    config: Config,
    loop_handler: Option<JoinHandle<()>>,
    instance_id: String,

    tx: Sender<InMessage>,
}

#[async_trait]
impl Pn532Port for HinataDevice {
    async fn request(&mut self, pn532_cmd: Pn532Command, payload: &[u8]) -> Result<Vec<u8>, Error> {
        let (subscription, mut rx) = Subscription::new(UnSubscribePolicy::SpecificNotOn(4, 0));
        let packet = Pn532Packet::new(Pn532Direction::HostToPn532, pn532_cmd, payload.to_vec());
        let mut send = vec![1, 0xE2];
        send.extend_from_slice(&packet.to_bytes());

        let _ = self.tx.send(InMessage::SendPacketAndSubscribe(send, subscription)).await;

        let standard_ack = [0, 0, 0xFF, 0, 0xFF, 0];

        let ack = Self::receive_packet(&mut rx, Duration::from_millis(1000)).await?;
        if &ack[1..7] != &standard_ack {return Err(Error::Protocol("ack error".to_string()))}

        let res = Self::receive_packet(&mut rx, Duration::from_millis(1000)).await?;
        let res_packet = Pn532Packet::from_bytes(&res[1..]).map_err(|e| {Error::Protocol(e)})?;

        if res_packet.direction != Pn532Direction::Pn532ToHost { return Err(Error::Protocol("Direction mismatch".to_string()))};
        if res_packet.command != packet.command { return Err(Error::Protocol("Command mismatch".to_string()))};

        Ok(res_packet.payload)
    }
}

impl HinataDevice {

    async fn receive_packet(rx: &mut Receiver<OutMessage>, timeout: Duration) -> Result<Vec<u8>, Error> {
        tokio::select!{
            message = rx.recv() => {
                if let Some(data) = message {
                    match data {
                        OutMessage::Response(data) => Ok(data),
                        OutMessage::DeviceDisconnect => Err(Error::Disconnected("Device disconnected".into()))
                    }
                } else {
                    Err(Error::Disconnected("Subscribe channel disconnected".into()))
                }
            },
            _timeout = tokio::time::sleep(timeout) => { Err(Error::Timeout("Wait response timeout".into())) }

        }
    }

    async fn request_without_response(&mut self, cmd: u8, payload: &[u8]) {
        let mut packet = vec![1, cmd];
        packet.extend_from_slice(payload);
        let _ = self.tx.send(InMessage::SendPacket(packet)).await;
    }
    async fn request(&mut self, cmd: u8, payload: &[u8]) -> Result<Vec<u8>, Error> {
        let mut packet = vec![1, cmd];
        packet.extend_from_slice(payload);
        let (subscription, mut rx) = Subscription::new(UnSubscribePolicy::Count(1));
        let _ = self.tx.send(InMessage::SendPacketAndSubscribe(packet, subscription)).await;
        let res = Self::receive_packet(&mut rx, Duration::from_millis(1000)).await?;
        Ok(res)
    }

    pub fn pn532(&'_ mut self) -> Pn532<'_, Self> {
        Pn532::new(self)
    }

    pub async fn get_firmware_timestamp(&mut self) -> Result<u32, Error> {
        if self.info.firmware_timestamp > 0 {return Ok(self.info.firmware_timestamp)}
        let raw = self.request(1, &[]).await?;
        let str = String::from_utf8(raw[..10].to_vec())?;
        let num = str.parse::<u32>()?;
        self.info.firmware_timestamp = num;
        Ok(num)
    }

    pub async fn set_led(&mut self, r: u8, g: u8, b: u8) { self.request_without_response(0x07, &[r, g, b]).await; }

    pub async fn reset_led(&mut self) {
        self.request_without_response(0xEA, &[]).await
    }

    pub async fn enter_bootloader(&mut self) { self.request_without_response(0xF0, &[]).await }

    pub async fn get_chip_id(&mut self) -> Result<[u8; 4], Error> {
        let timestamp = self.get_firmware_timestamp().await?;
        if timestamp < 2025051301 { return Err(Error::NotSupport("Firmware version too old".into())) };
        let chip_id = if let Some(id) = self.info.chip_id {
            id
        } else {
            let res = self.request(0xE6, &[]).await?;
            let array = Self::get_four_bytes(&res[1..])?;
            self.info.chip_id = Some(array);
            array
        };
        Ok(chip_id)
    }

    fn get_four_bytes(data: &[u8]) -> Result<[u8; 4], Error> {
        let array: [u8; 4] = data.get(..4)
            .and_then(|slice| slice.try_into().ok())
            .ok_or(Error::Protocol("buffer size error".into()))?;
        Ok(array)
    }

    pub async fn get_firmware_commit_hash(&mut self) -> Result<[u8; 4], Error> {
        let timestamp = self.get_firmware_timestamp().await?;
        if timestamp < 2025051301 { return Err(Error::NotSupport("Firmware version too old".into())) };
        let commit_hash = if let Some(hash) = self.info.firmware_commit_hash {
            hash
        } else {
            let res = self.request(0xE5, &[]).await?;
            let array = Self::get_four_bytes(&res[1..])?;
            self.info.firmware_commit_hash = Some(array);
            array
        };
        Ok(commit_hash)
    }
}

#[derive(Debug)]
pub struct HinataDeviceBuilder {
    read: HidDevice,
    write: HidDevice,
    instance_id: String
}

impl HinataDeviceBuilder {
    pub async fn build(self, debug: bool) -> HinataDevice {
        let (main_to_sub_tx, main_to_sub_rx): (Sender<InMessage>, Receiver<InMessage>) = mpsc::channel(255); // 给辅助线程发送消息
        // let (sub_to_main_tx, sub_to_main_rx): (Sender<OutMessage>, Receiver<OutMessage>) = mpsc::channel(255); // 从辅助线程接收消息
        let handler = thread::spawn(move || {
            Self::io_loop(self.read, self.write, main_to_sub_rx, debug)
        });

        HinataDevice {
            info: Info { firmware_timestamp: 0, firmware_commit_hash: None, chip_id: None },
            config: Config { sega_brightness: 0, sega_rapid_scan: false },
            instance_id: self.instance_id,
            loop_handler: Some(handler),
            tx: main_to_sub_tx,
        }
    }

    fn handle_hid_error(subscribes: &mut HashMap<u8, Subscription>, _: HidError) {
        subscribes.drain().for_each(|(_, channel)| {
            channel.send_no_check(OutMessage::DeviceDisconnect);
        });
    }

    fn io_loop(hid_in: HidDevice, hid_out: HidDevice, mut message_in: Receiver<InMessage>, debug: bool) {
        let mut buf = [0; 64];
        let mut subscribes: HashMap<u8, Subscription> = HashMap::new();
        let write = |data: &[u8], subs: &mut HashMap<u8, Subscription>| {
            match hid_out.write(&data) {
                Ok(_) => if debug {println!("DEBUG: -> {:02X?}", data)},
                Err(e) => Self::handle_hid_error(subs, e)
            }
        };
        loop {
            loop {
                match message_in.try_recv() {
                    Ok(mes) => {
                        match mes {
                            InMessage::SendPacket(data) => write(&data, &mut subscribes),
                            InMessage::SendPacketAndSubscribe(data, subscription) => {
                                let key = if data[1] == 1 { 50 } else { data[1] }; // 处理GETTIMESTAMP
                                subscribes.insert(key, subscription);
                                write(&data, &mut subscribes);
                            }
                            InMessage::Subscribe(cmd, subscription) => {
                                subscribes.insert(cmd, subscription);
                            }
                            InMessage::UnSubscribe(cmd) => {
                                let _ = subscribes.remove(&cmd);
                            }
                        }
                    }
                    Err(e) => match e {
                        mpsc::error::TryRecvError::Empty => break,
                        mpsc::error::TryRecvError::Disconnected => { return },
                    }
                }
            }
            match hid_in.read_timeout(&mut buf, 16) {
                Ok(len) => if len > 0 {
                    if let Entry::Occupied(mut entry) = subscribes.entry(buf[1]) {
                        if entry.get_mut().send(OutMessage::Response(buf[1..].to_vec())) {
                            entry.remove();
                        }
                    }
                    if debug {println!("DEBUG: <- {:02X?}", &buf)}
                }
                Err(e) => Self::handle_hid_error(&mut subscribes, e)
            }
        }
    }

    pub fn get_instance_id(&self) -> String {
        self.instance_id.to_string()
    }
}

// --- Device Discovery ---

pub async fn find_devices() -> Result<Vec<HinataDeviceBuilder>, Error> {
    spawn_blocking(|| find_devices_inner().map_err(|_| Error::NotFound("Device not found".to_string()))).await.map_err(|e| Error::Other(e.to_string()))?
}

fn find_devices_inner() -> Result<Vec<HinataDeviceBuilder>, HidError> {
    struct PreDeviceBuilder {
        read: Option<HidDevice>,
        write: Option<HidDevice>
    }

    let hid = HidApi::new()?;

    let mut devices: HashMap<String, PreDeviceBuilder> = HashMap::new();
    for device in hid.device_list() {
        if device.vendor_id() != HINATA_VID {
            continue;
        }

        // println!("found device: {:?}, instance: {:?}", device, windows_get_instance_id(device.path().to_string_lossy().as_ref()));

        if device.usage_page() == USAGE_PAGE_READ {
            if let Some(key) = get_instance(device.path().to_string_lossy().as_ref()) {
                if let Some(builder) = devices.get_mut(&key) {
                    builder.read = Some(device.open_device(&hid)?)
                } else {
                    devices.insert(key, PreDeviceBuilder { read: Some(device.open_device(&hid)?), write: None });
                }
            };
        } else if device.usage_page() == USAGE_PAGE_WRITE {
            if let Some(key) = get_instance(device.path().to_string_lossy().as_ref()) {
                if let Some(builder) = devices.get_mut(&key) {
                    builder.write = Some(device.open_device(&hid)?)
                } else {
                    devices.insert(key, PreDeviceBuilder { read: None, write: Some(device.open_device(&hid)?) });
                }
            };
        }
    }
    Ok(
        devices.into_iter().filter_map(|(instance, builder)| {
            builder.read.zip(builder.write).map(|(read, write)| {
                HinataDeviceBuilder {
                    read,
                    write,
                    instance_id: instance,
                }
            })
        }).collect()
    )
}

// Generate with Gemini 3
#[cfg(target_os = "windows")]
fn get_instance(path: &str) -> Option<String> {
    // Windows Path 典型结构:
    // \\?\HID#VID_xxxx&PID_xxxx&MI_xx#<Instance_ID>#{GUID}

    let parts: Vec<&str> = path.split('#').collect();

    // 结构校验：必须至少有3个 '#' 分隔出的部分 (Index 0, 1, 2, 3)
    // parts[0]: \\?\HID
    // parts[1]: HWID (VID_...&PID_...)
    // parts[2]: Instance ID (8&899cbf4&0&0002) <- 目标在这里
    // parts[3]: GUID ({...})
    if parts.len() < 3 {
        return None;
    }

    let instance_id_full = parts[2];

    // 找到最后一个 '&' 的位置
    // 对于复合设备，最后一段通常是接口索引，如 &0000, &0001
    if let Some(last_amp_index) = instance_id_full.rfind('&') {
        // 截取前半部分： "8&899cbf4&0"
        return Some(instance_id_full[..last_amp_index].to_string());
    }

    // 如果没有 '&'，说明可能不是复合设备或者结构特殊，直接返回完整ID作为Key
    Some(instance_id_full.to_string())
}

#[cfg(target_os = "linux")]
fn get_instance(path: &str) -> Option<String> {
    Some(path.to_string())
}