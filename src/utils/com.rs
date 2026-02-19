use regex::Regex;
use serde::Deserialize;

use winreg::enums::*;
use winreg::RegKey;
use wmi::WMIConnection;

use windows::core::{GUID, PCWSTR};
use windows::Win32::Devices::DeviceAndDriverInstallation::*;
use windows::Win32::Devices::Properties::{DEVPKEY_Device_ClassGuid, DEVPROPTYPE, DEVPROP_TYPE_GUID};
use crate::error::{Error, HinataResult};

/// WMI 实体结构
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct PnPEntity {
    device_id: String,
    name: Option<String>,
    caption: Option<String>,
}

// Windows 标准的 Ports 类 GUID: {4d36e978-e325-11ce-bfc1-08002be10318}
const GUID_DEVCLASS_PORTS: GUID = GUID::from_u128(0x4d36e978_e325_11ce_bfc1_08002be10318);

pub fn get_com_instance_id_by_hid_instance_id(instance_id: &str) -> HinataResult<String> {
    unsafe {
        // 1. 准备字符串
        let input_id_wide: Vec<u16> = instance_id
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        // 2. 定位起始节点 (Locate)
        let mut current_dev_node: u32 = 0;
        let ret = CM_Locate_DevNodeW(
            &mut current_dev_node,
            PCWSTR::from_raw(input_id_wide.as_ptr()),
            CM_LOCATE_DEVNODE_NORMAL,
        );

        if ret != CR_SUCCESS {
            return Err(Error::NotFound(format!("Could not locate device node: {}", instance_id)));
        }

        // 3. 开始向上爬树循环 (最多爬 3-4 层足够了，防止死循环)
        // 层级关系通常是: HID Collection -> USB Interface -> USB Composite -> USB Hub
        for _depth in 0..5 {
            // A. 获取当前节点的父节点
            let mut parent_dev_node: u32 = 0;
            let ret = CM_Get_Parent(&mut parent_dev_node, current_dev_node, 0);

            if ret != CR_SUCCESS {
                // 到顶了或者出错了，停止
                break;
            }

            // B. 检查这个父节点的所有子节点（寻找 Serial Port）
            if let Some(serial_id) = find_child_port(parent_dev_node) {
                return Ok(serial_id);
            }

            // C. 没找到，继续向上爬
            current_dev_node = parent_dev_node;
        }
    }

    Err(Error::NotFound("Associated Serial device (Class Ports) not found in ancestry tree".to_string()))
}

/// 辅助函数：检查指定父节点的所有直接子节点，看有没有 Ports 类型的
unsafe fn find_child_port(parent_node: u32) -> Option<String> {
    unsafe {
        let mut child_node: u32 = 0;
        // 获取第一个子节点
        if CM_Get_Child(&mut child_node, parent_node, 0) != CR_SUCCESS {
            return None;
        }

        let mut current_node = child_node;

        // 遍历所有兄弟节点
        loop {
            // --- 检查 Class GUID ---
            let mut buffer = [0u8; 16];
            let mut buffer_size = buffer.len() as u32;
            let mut property_type: u32 = 0;

            let mut dev_prop_type = DEVPROPTYPE(property_type);

            // 获取 ClassGuid 属性
            let ret = CM_Get_DevNode_PropertyW(
                current_node,
                &DEVPKEY_Device_ClassGuid,
                &mut dev_prop_type,
                Some(buffer.as_mut_ptr()),
                &mut buffer_size,
                0
            );

            // 如果是 Ports 设备
            if ret == CR_SUCCESS
                && dev_prop_type == DEVPROP_TYPE_GUID
                && *(buffer.as_ptr() as *const GUID) == GUID_DEVCLASS_PORTS
            {
                // 获取它的 Instance ID
                let mut id_buffer = [0u16; 256];
                if CM_Get_Device_IDW(current_node, &mut id_buffer, 0) == CR_SUCCESS {
                    let device_id = String::from_utf16_lossy(&id_buffer);
                    return Some(device_id.trim_matches(char::from(0)).to_string());
                }
            }

            // --- 移动到下一个兄弟 ---
            let mut next_node: u32 = 0;
            if CM_Get_Sibling(&mut next_node, current_node, 0) != CR_SUCCESS {
                break; // 遍历结束
            }
            current_node = next_node;
        }

        None
    }
}

/// 根据 VID 和 PID 获取当前连接设备的 COM 端口号和实例 ID
pub fn get_com_port_by_vid_pid(vid: u16, pid: u16) -> HinataResult<(String, String)> {
    // 保持原有的初始化方式
    let wmi_con = WMIConnection::new()
        .map_err(|e| Error::Other(format!("Failed to create WMI connection: {:?}", e)))?;

    let query = format!(
        "SELECT DeviceID, Name, Caption FROM Win32_PnPEntity WHERE DeviceID LIKE '%VID_{:04X}%PID_{:04X}%'",
        vid, pid
    );

    let results: Vec<PnPEntity> = wmi_con.raw_query(&query)
        .map_err(|e| Error::Other(format!("WMI Query failed: {:?}", e)))?;

    if results.is_empty() {
        return Err(Error::NotFound(format!("Device with VID:{:04X} PID:{:04X} not found", vid, pid)));
    }

    // 优化：在循环外编译正则，提高效率
    let re = Regex::new(r"\(COM(\d+)\)")
        .map_err(|e| Error::Parse(format!("Failed to compile regex: {}", e)))?;

    for device in results {
        // 策略1：优先尝试读取注册表真实的 PortName
        if let Ok(real_port) = get_com_port_by_instance_id(&device.device_id) {
            println!("{}", device.device_id);
            return Ok((real_port, device.device_id));
        }

        // 策略2：回退逻辑，解析显示的 Name 或 Caption
        let name = device.name.as_deref().or(device.caption.as_deref()).unwrap_or("");

        if let Some(caps) = re.captures(name) {
            if let Some(match_item) = caps.get(1) {
                let port_name = format!("COM{}", match_item.as_str());
                return Ok((port_name, device.device_id));
            }
        }
    }

    Err(Error::NotFound("Device found but unable to identify COM port".to_string()))
}

/// 辅助函数：直接从注册表读取 PortName
pub fn get_com_port_by_instance_id(instance_id: &str) -> HinataResult<String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key_path = format!("SYSTEM\\CurrentControlSet\\Enum\\{}\\Device Parameters", instance_id);
    let key = hklm.open_subkey(&key_path)?;

    let port_name: String = key.get_value("PortName")?;

    Ok(port_name)
}

/// 检查指定的 COM 端口是否当前正在被系统使用
pub fn get_device_on_port(port_name: &str) -> HinataResult<Option<String>> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    // SERIALCOMM 只是映射表，有时会有延迟，但它是检查占用的最快方法
    let serial_comm = hklm.open_subkey("HARDWARE\\DEVICEMAP\\SERIALCOMM")?;

    let port_upper = port_name.to_uppercase();

    for (_name, value) in serial_comm.enum_values().filter_map(|x| x.ok()) {
        let value_str: String = value.to_string().to_uppercase();
        if value_str == port_upper {
            // 如果 SERIALCOMM 里有，我们再去反查是谁占用的
            return find_device_id_by_port_name_wmi(&port_upper);
        }
    }

    Ok(None)
}

/// 反向查找：通过端口名 (如 COM3) 查找 DeviceID
fn find_device_id_by_port_name_wmi(port_name: &str) -> HinataResult<Option<String>> {
    let wmi_con = WMIConnection::new()
        .map_err(|e| Error::Other(format!("Failed to create WMI connection: {:?}", e)))?;

    let query = format!("SELECT DeviceID, Name FROM Win32_PnPEntity WHERE Name LIKE '%({})%'", port_name);
    let results: Vec<PnPEntity> = wmi_con.raw_query(&query)
        .map_err(|e| Error::Other(format!("WMI Query for port name failed: {:?}", e)))?;

    if let Some(device) = results.first() {
        return Ok(Some(device.device_id.clone()));
    }
    Ok(None)
}

/// 核心修改函数：修改 PortName 同时修复 FriendlyName
pub fn set_device_com_port(device_id: &str, new_port: &str) -> HinataResult<()> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    // 1. 修改实际通讯参数 PortName
    let params_path = format!("SYSTEM\\CurrentControlSet\\Enum\\{}\\Device Parameters", device_id);
    let (params_key, _) = hklm.create_subkey(&params_path)?;

    params_key.set_value("PortName", &new_port)?;

    // 2. 修改显示名称 FriendlyName
    // 路径是 SYSTEM\CurrentControlSet\Enum\{DeviceID}
    let device_path = format!("SYSTEM\\CurrentControlSet\\Enum\\{}", device_id);
    let (device_key, _) = hklm.create_subkey(&device_path)?;

    // 安全读取 FriendlyName，如果读取失败或者没有该值，我们选择忽略而不是报错中断流程
    // 因为 PortName 已经修改成功，FriendlyName 只是为了显示美观
    if let Ok(current_friendly_name) = device_key.get_value::<String, _>("FriendlyName") {
        if !current_friendly_name.is_empty() {
            let re = Regex::new(r"\(COM\d+\)")
                .map_err(|e| Error::Parse(format!("Failed to compile regex for FriendlyName: {}", e)))?;
            let replacement = format!("({})", new_port);
            let new_friendly_name = re.replace(&current_friendly_name, replacement.as_str());

            if new_friendly_name != current_friendly_name {
                device_key.set_value("FriendlyName", &new_friendly_name.to_string())?;
            }
        }
    }

    Ok(())
}

pub fn force_set_usb_port(vid: u16, pid: u16, target_port: &str) -> HinataResult<()> {
    let target_port = target_port.to_uppercase();

    // 1. 查找目标设备
    let (current_port, device_id) = get_com_port_by_vid_pid(vid, pid)
        .map_err(|e| Error::Other(format!("Failed to find target USB device: {}", e)))?;

    // 2. 幂等性检查：如果端口已经正确
    if current_port == target_port {
        // 强制刷新 FriendlyName 以防万一
        set_device_com_port(&device_id, &target_port)?;
        return Ok(());
    }

    // 3. 冲突检测：检查目标端口是否被占用
    if let Some(occupying_device_id) = get_device_on_port(&target_port)? {
        // 如果占用者就是设备自己（可能是注册表状态不一致），直接覆盖
        if occupying_device_id == device_id {
            set_device_com_port(&device_id, &target_port)?;
            return Ok(());
        }

        // 4. 寻找避让端口：扫描 COM200-255 寻找空闲位置
        let mut fallback_port = String::new();
        for i in (200..=255).rev() {
            let candidate = format!("COM{}", i);
            // 确保候选端口既没有被占用，也不是我们要抢占的目标端口
            if get_device_on_port(&candidate)?.is_none() && candidate != target_port {
                fallback_port = candidate;
                break;
            }
        }

        if fallback_port.is_empty() {
            return Err(Error::NotFound("No free COM port found (COM200-255) for relocation".to_string()));
        }

        // 5. 移动占用者
        set_device_com_port(&occupying_device_id, &fallback_port)
            .map_err(|e| Error::Other(format!("Failed to relocate occupying device {} to {}: {}", occupying_device_id, fallback_port, e)))?;
    }

    // 6. 将目标设备设置到目标端口
    set_device_com_port(&device_id, &target_port)
        .map_err(|e| Error::Other(format!("Failed to set device {} to {}: {}", device_id, target_port, e)))?;

    Ok(())
}

#[test]
fn get_port_test() {
    let com_serial = get_com_instance_id_by_hid_instance_id("HID\\VID_F822&PID_0147&MI_02&Col01\\8&38333037&0&0000").unwrap();
    let port = get_com_port_by_instance_id(&com_serial).unwrap();
    println!("{}, {}", com_serial, port);
}
