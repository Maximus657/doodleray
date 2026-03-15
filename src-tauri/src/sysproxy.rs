/// Windows system proxy helper — sets/unsets the HTTP proxy via registry
use winreg::enums::*;
use winreg::RegKey;

const INTERNET_SETTINGS: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";

pub fn set_system_proxy(http_port: u16) -> Result<(), String> {
    let socks_port = http_port - 1;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(INTERNET_SETTINGS)
        .map_err(|e| format!("Failed to open registry: {}", e))?;
    
    let proxy_addr = format!(
        "http=127.0.0.1:{};https=127.0.0.1:{};socks=127.0.0.1:{}", 
        http_port, http_port, socks_port
    );
    key.set_value("ProxyServer", &proxy_addr)
        .map_err(|e| format!("Failed to set ProxyServer: {}", e))?;
    
    key.set_value("ProxyEnable", &1u32)
        .map_err(|e| format!("Failed to set ProxyEnable: {}", e))?;
    
    // Bypass list: local addresses + ALL major game platforms
    // Games will connect DIRECTLY without VPN (no ping increase)
    let bypass = vec![
        // Local
        "localhost", "127.*", "10.*", "192.168.*", "172.16.*", "<local>",
        // Riot Games (Valorant, LoL, etc.)
        "*.riotgames.com", "*.leagueoflegends.com", "*.playvalorant.com",
        "*.riotcdn.net", "*.riotgames.net", "*.lolesports.com",
        // Steam
        "*.steampowered.com", "*.steamcommunity.com", "*.steamgames.com",
        "*.steamcontent.com", "*.steamstatic.com", "*.steam-chat.com",
        "*.valvesoftware.com",
        // Epic Games
        "*.epicgames.com", "*.unrealengine.com", "*.fortnite.com",
        "*.epicgames.dev", "*.on.epicgames.com",
        // Blizzard / Activision
        "*.blizzard.com", "*.battle.net", "*.battlenet.com.cn",
        "*.blizzard.cn", "*.activision.com",
        // EA / Origin
        "*.ea.com", "*.origin.com", "*.tnt-ea.com",
        // Ubisoft
        "*.ubisoft.com", "*.ubi.com",
        // Minecraft / Xbox
        "*.minecraft.net", "*.mojang.com", "*.xbox.com",
        "*.xboxlive.com", "*.microsoftonline.com",
        // Faceit / ESEA
        "*.faceit.com", "*.esea.net",
        // General gaming
        "*.gaijin.net", "*.wargaming.net", "*.worldoftanks.com",
        "*.roblox.com", "*.rbxcdn.com",
        // Anti-cheat
        "*.easyanticheat.net", "*.battleye.com", "*.vanguard.riotgames.com",
    ];
    
    key.set_value("ProxyOverride", &bypass.join(";"))
        .map_err(|e| format!("Failed to set ProxyOverride: {}", e))?;

    notify_proxy_change();
    
    Ok(())
}

pub fn unset_system_proxy() -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(INTERNET_SETTINGS)
        .map_err(|e| format!("Failed to open registry: {}", e))?;
    
    // Disable proxy
    key.set_value("ProxyEnable", &0u32)
        .map_err(|e| format!("Failed to disable proxy: {}", e))?;
    
    notify_proxy_change();
    
    Ok(())
}

fn notify_proxy_change() {
    // Call InternetSetOption to notify Windows of proxy change
    #[allow(non_snake_case)]
    unsafe {
        #[link(name = "wininet")]
        extern "system" {
            fn InternetSetOptionW(
                hInternet: *mut std::ffi::c_void,
                dwOption: u32,
                lpBuffer: *mut std::ffi::c_void,
                dwBufferLength: u32,
            ) -> i32;
        }
        const INTERNET_OPTION_SETTINGS_CHANGED: u32 = 39;
        const INTERNET_OPTION_REFRESH: u32 = 37;
        InternetSetOptionW(std::ptr::null_mut(), INTERNET_OPTION_SETTINGS_CHANGED, std::ptr::null_mut(), 0);
        InternetSetOptionW(std::ptr::null_mut(), INTERNET_OPTION_REFRESH, std::ptr::null_mut(), 0);
    }
}
