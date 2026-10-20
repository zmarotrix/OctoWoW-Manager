//! OS-specific helpers, using native APIs rather than helper processes where possible.

const GAME_EXES: [&str; 2] = ["wow.exe", "wow_tweaked.exe"];

/// True if the client is running natively (Windows) or under Wine/Proton/CrossOver.
#[cfg(windows)]
pub fn is_game_running() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    // SAFETY: a plain Toolhelp snapshot walk; the entry is zero-initialised with
    // dwSize set, and the handle is closed on every path.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return false;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut entry) != 0;
        let mut found = false;
        while ok {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]).to_ascii_lowercase();
            if GAME_EXES.contains(&name.as_str()) {
                found = true;
                break;
            }
            ok = Process32NextW(snap, &mut entry) != 0;
        }
        CloseHandle(snap);
        found
    }
}

#[cfg(not(windows))]
pub fn is_game_running() -> bool {
    // Wine processes show the Windows exe in their command line.
    std::process::Command::new("ps")
        .args(["-A", "-o", "args="])
        .output()
        .map(|o| {
            let list = String::from_utf8_lossy(&o.stdout).to_ascii_lowercase();
            list.lines().any(|l| GAME_EXES.iter().any(|exe| l.contains(exe)))
        })
        .unwrap_or(false)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DepPolicy {
    AlwaysOff,
    AlwaysOn,
    /// Windows default: only programs that opt in get DEP.
    OptIn,
    /// Everything gets DEP unless excluded in System Properties.
    OptOut,
    #[cfg_attr(windows, allow(dead_code))]
    NotApplicable,
}

#[cfg(windows)]
pub fn dep_policy() -> DepPolicy {
    use windows_sys::Win32::System::SystemInformation::{DEPPolicyAlwaysOff, DEPPolicyAlwaysOn, DEPPolicyOptOut, GetSystemDEPPolicy};
    // SAFETY: takes no arguments and returns a plain enum value.
    match unsafe { GetSystemDEPPolicy() } {
        p if p == DEPPolicyAlwaysOff => DepPolicy::AlwaysOff,
        p if p == DEPPolicyAlwaysOn => DepPolicy::AlwaysOn,
        p if p == DEPPolicyOptOut => DepPolicy::OptOut,
        _ => DepPolicy::OptIn,
    }
}

#[cfg(not(windows))]
pub fn dep_policy() -> DepPolicy {
    DepPolicy::NotApplicable
}

/// The best Vulkan device on this machine, as DXVK would see it.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VulkanDevice {
    pub name: String,
    pub vendor_id: u32,
    pub major: u32,
    pub minor: u32,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VulkanStatus {
    /// False where the check doesn't apply (macOS: the game runs under Wine/CrossOver).
    pub checked: bool,
    pub device: Option<VulkanDevice>,
}

/// Asks the system Vulkan loader for its GPUs. No driver, or no device, means DXVK can't run.
pub fn vulkan_status() -> VulkanStatus {
    if cfg!(target_os = "macos") {
        return VulkanStatus::default();
    }
    let device = std::panic::catch_unwind(probe_vulkan).ok().flatten();
    VulkanStatus { checked: true, device }
}

fn probe_vulkan() -> Option<VulkanDevice> {
    use ash::vk;
    // SAFETY: loads the system Vulkan loader and creates a bare instance that is
    // destroyed before returning; no handles escape this function.
    unsafe {
        let entry = ash::Entry::load().ok()?;
        let app = vk::ApplicationInfo::default().api_version(vk::make_api_version(0, 1, 1, 0));
        let instance = entry.create_instance(&vk::InstanceCreateInfo::default().application_info(&app), None).ok()?;
        let best = instance
            .enumerate_physical_devices()
            .unwrap_or_default()
            .into_iter()
            .map(|d| instance.get_physical_device_properties(d))
            .max_by_key(|p| (p.api_version, p.device_type == vk::PhysicalDeviceType::DISCRETE_GPU));
        instance.destroy_instance(None);
        let p = best?;
        Some(VulkanDevice {
            name: p.device_name_as_c_str().map(|c| c.to_string_lossy().into_owned()).unwrap_or_default(),
            vendor_id: p.vendor_id,
            major: vk::api_version_major(p.api_version),
            minor: vk::api_version_minor(p.api_version),
        })
    }
}

#[cfg(test)]
mod tests {
    /// Prints what DXVK would see on this machine: `cargo test vulkan_probe -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn vulkan_probe() {
        println!("{:?}", super::vulkan_status());
    }
}
