#[cfg(target_os = "windows")]
pub fn parse_hid_path(path: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = path.split('#').collect();
    if parts.len() < 3 { return None; }
    let (Some(part_1), Some(part_2)) = (parts.get(1), parts.get(2)) else {
        return None;
    };
    if let Some(last_amp_index) = part_2.rfind('&') {
        Some((format!("HID\\{}\\{}", part_1, part_2), part_2[..last_amp_index].to_string()))
    } else {
        None
    }
}

#[cfg(not(target_os = "windows"))]
pub fn get_instance(path: &str) -> Option<(String, String)> {
    Some((path.to_string(), path.to_string()))
}
