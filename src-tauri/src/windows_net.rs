use std::ffi::CStr;
use std::fmt;
use std::mem::zeroed;
use std::net::Ipv4Addr;
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_NO_DATA, HANDLE, NO_ERROR};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GetBestRoute2, GetIpInterfaceEntry, InitializeIpInterfaceEntry,
    NotifyIpInterfaceChange, NotifyRouteChange2, NotifyUnicastIpAddressChange, SetIpInterfaceEntry,
    GAA_FLAG_INCLUDE_ALL_INTERFACES, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER,
    GAA_FLAG_SKIP_MULTICAST, IP_ADAPTER_ADDRESSES_LH, MIB_IPFORWARD_ROW2, MIB_IPINTERFACE_ROW,
    MIB_NOTIFICATION_TYPE, MIB_UNICASTIPADDRESS_ROW,
};
use windows_sys::Win32::NetworkManagement::Ndis::NET_LUID_LH;
use windows_sys::Win32::Networking::WinSock::{
    IpDadStatePreferred, ADDRESS_FAMILY, AF_INET, AF_INET6, IN_ADDR, IN_ADDR_0, IN_ADDR_0_0,
    SOCKADDR_IN, SOCKADDR_INET,
};

const AF_UNSPEC: u32 = 0;
const RECOMMENDED_ADAPTER_BUFFER_BYTES: u32 = 15 * 1024;
static INTERFACE_EVENT_SEQ: AtomicU64 = AtomicU64::new(0);
static UNICAST_V4_EVENT_SEQ: AtomicU64 = AtomicU64::new(0);
static ROUTE_V4_EVENT_SEQ: AtomicU64 = AtomicU64::new(0);
static WATCHERS: OnceLock<Result<NetworkWatchHandles, String>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetProbeError {
    NotFound(String),
    Failed(String),
}

impl NetProbeError {
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound(_))
    }
}

impl fmt::Display for NetProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(message) | Self::Failed(message) => f.write_str(message),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterSnapshot {
    pub alias: String,
    pub adapter_name: Option<String>,
    pub ifindex: u32,
    pub luid_value: u64,
    pub oper_status: u32,
    pub mtu: u32,
    pub ipv4_unicast_count: usize,
    pub ipv4_preferred_count: usize,
}

impl AdapterSnapshot {
    pub fn readiness_detail(&self) -> String {
        format!(
            "DoodleRay Tunnel adapter native snapshot: alias={}, ifIndex={}, luid={}, oper={}, mtu={}, ipv4={}/preferred={}",
            self.alias,
            self.ifindex,
            self.luid_value,
            self.oper_status,
            self.mtu,
            self.ipv4_unicast_count,
            self.ipv4_preferred_count
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkEventCursors {
    pub interface: u64,
    pub unicast_v4: u64,
    pub route_v4: u64,
}

#[derive(Debug)]
struct NetworkWatchHandles {
    interface: HANDLE,
    unicast_v4: HANDLE,
    route_v4: HANDLE,
}

unsafe impl Send for NetworkWatchHandles {}
unsafe impl Sync for NetworkWatchHandles {}

impl NetworkEventCursors {
    pub fn adapter_changed_since(self) -> bool {
        let current = network_event_cursors();
        current.interface != self.interface || current.unicast_v4 != self.unicast_v4
    }

    pub fn route_changed_since(self) -> bool {
        network_event_cursors().route_v4 != self.route_v4
    }
}

pub fn ensure_network_watchers() -> Result<String, NetProbeError> {
    match WATCHERS.get_or_init(register_network_watchers) {
        Ok(handles) => Ok(format!(
            "iphelper watchers active: interface={:?}, unicast_v4={:?}, route_v4={:?}",
            handles.interface, handles.unicast_v4, handles.route_v4
        )),
        Err(error) => Err(NetProbeError::Failed(error.clone())),
    }
}

pub fn network_event_cursors() -> NetworkEventCursors {
    NetworkEventCursors {
        interface: INTERFACE_EVENT_SEQ.load(Ordering::SeqCst),
        unicast_v4: UNICAST_V4_EVENT_SEQ.load(Ordering::SeqCst),
        route_v4: ROUTE_V4_EVENT_SEQ.load(Ordering::SeqCst),
    }
}

pub fn find_adapter_by_alias(alias: &str) -> Result<AdapterSnapshot, NetProbeError> {
    let adapters = enumerate_adapters()?;
    adapters
        .into_iter()
        .find(|adapter| adapter_alias_matches(&adapter.alias, alias))
        .ok_or_else(|| {
            NetProbeError::NotFound(format!("{alias} adapter was not visible via IP Helper"))
        })
}

pub fn apply_interface_metric(alias: &str, target_metric: u32) -> Result<String, NetProbeError> {
    let snapshot = find_adapter_by_alias(alias)?;
    let ipv4 = set_interface_metric(
        snapshot.luid_value,
        snapshot.ifindex,
        AF_INET,
        target_metric,
    )
    .map_err(NetProbeError::Failed)?;

    let ipv6 = set_interface_metric(
        snapshot.luid_value,
        snapshot.ifindex,
        AF_INET6,
        target_metric,
    )
    .map(|metric| format!(", ipv6_metric={metric}"))
    .unwrap_or_else(|error| format!(", ipv6_metric=not_applied({error})"));

    Ok(format!(
        "{}; ipv4_metric={}{}",
        snapshot.readiness_detail(),
        ipv4,
        ipv6
    ))
}

pub fn route_canaries_prefer_adapter(
    alias: &str,
    canaries: &[Ipv4Addr],
) -> Result<String, NetProbeError> {
    let snapshot = find_adapter_by_alias(alias)?;
    let mut checked = Vec::new();
    for canary in canaries {
        let route = best_route_for_ipv4(*canary).map_err(NetProbeError::Failed)?;
        if route.interface_index != snapshot.ifindex {
            return Err(NetProbeError::NotFound(format!(
                "DoodleRay Tunnel is not selected for protected route canary {canary}: best_ifIndex={}, expected_ifIndex={}",
                route.interface_index, snapshot.ifindex
            )));
        }
        checked.push(canary.to_string());
    }

    Ok(format!(
        "DoodleRay Tunnel route preferred via native GetBestRoute2: ifIndex={}, canaries={}",
        snapshot.ifindex,
        checked.join(",")
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RouteProbe {
    interface_index: u32,
}

fn set_interface_metric(
    luid_value: u64,
    ifindex: u32,
    family: ADDRESS_FAMILY,
    target_metric: u32,
) -> Result<u32, String> {
    let mut row: MIB_IPINTERFACE_ROW = unsafe { zeroed() };
    unsafe {
        InitializeIpInterfaceEntry(&mut row);
        row.Family = family;
        row.InterfaceLuid = luid_from_value(luid_value);
        row.InterfaceIndex = ifindex;
        let get_error = GetIpInterfaceEntry(&mut row);
        if get_error != NO_ERROR {
            return Err(format!("GetIpInterfaceEntry({family}) failed: {get_error}"));
        }
        row.UseAutomaticMetric = 0;
        row.Metric = target_metric;
        let set_error = SetIpInterfaceEntry(&mut row);
        if set_error != NO_ERROR {
            return Err(format!("SetIpInterfaceEntry({family}) failed: {set_error}"));
        }

        let mut verify: MIB_IPINTERFACE_ROW = zeroed();
        InitializeIpInterfaceEntry(&mut verify);
        verify.Family = family;
        verify.InterfaceLuid = luid_from_value(luid_value);
        verify.InterfaceIndex = ifindex;
        let verify_error = GetIpInterfaceEntry(&mut verify);
        if verify_error != NO_ERROR {
            return Err(format!(
                "GetIpInterfaceEntry({family}) verify failed: {verify_error}"
            ));
        }
        if verify.Metric != target_metric {
            return Err(format!(
                "metric verify failed for family {family}: got {}, expected {target_metric}",
                verify.Metric
            ));
        }
        Ok(verify.Metric)
    }
}

fn best_route_for_ipv4(ip: Ipv4Addr) -> Result<RouteProbe, String> {
    let mut best_route: MIB_IPFORWARD_ROW2 = unsafe { zeroed() };
    let mut best_source: SOCKADDR_INET = unsafe { zeroed() };
    let destination = sockaddr_for_ipv4(ip);
    let error = unsafe {
        GetBestRoute2(
            ptr::null(),
            0,
            ptr::null(),
            &destination,
            0,
            &mut best_route,
            &mut best_source,
        )
    };
    if error != NO_ERROR {
        return Err(format!("GetBestRoute2({ip}) failed: {error}"));
    }
    Ok(RouteProbe {
        interface_index: best_route.InterfaceIndex,
    })
}

fn enumerate_adapters() -> Result<Vec<AdapterSnapshot>, NetProbeError> {
    let mut size = RECOMMENDED_ADAPTER_BUFFER_BYTES;
    for _ in 0..3 {
        let word_count =
            (size as usize + std::mem::size_of::<usize>() - 1) / std::mem::size_of::<usize>();
        let mut buffer = vec![0usize; word_count.max(1)];
        let mut actual_size = (buffer.len() * std::mem::size_of::<usize>()) as u32;
        let error = unsafe {
            GetAdaptersAddresses(
                AF_UNSPEC,
                GAA_FLAG_INCLUDE_ALL_INTERFACES
                    | GAA_FLAG_SKIP_ANYCAST
                    | GAA_FLAG_SKIP_MULTICAST
                    | GAA_FLAG_SKIP_DNS_SERVER,
                ptr::null(),
                buffer.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH,
                &mut actual_size,
            )
        };
        if error == ERROR_BUFFER_OVERFLOW {
            size = actual_size.max(size.saturating_mul(2));
            continue;
        }
        if error == ERROR_NO_DATA {
            return Ok(Vec::new());
        }
        if error != NO_ERROR {
            return Err(NetProbeError::Failed(format!(
                "GetAdaptersAddresses failed: {error}"
            )));
        }

        let mut adapters = Vec::new();
        let mut current = buffer.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH;
        while !current.is_null() {
            let adapter = unsafe { &*current };
            if let Some(snapshot) = unsafe { adapter_snapshot(adapter) } {
                adapters.push(snapshot);
            }
            current = adapter.Next;
        }
        return Ok(adapters);
    }

    Err(NetProbeError::Failed(
        "GetAdaptersAddresses kept requesting a larger buffer".into(),
    ))
}

unsafe fn adapter_snapshot(adapter: &IP_ADAPTER_ADDRESSES_LH) -> Option<AdapterSnapshot> {
    let alias = wide_ptr_to_string(adapter.FriendlyName)?;
    let adapter_name = c_ptr_to_string(adapter.AdapterName);
    let ifindex = adapter.Anonymous1.Anonymous.IfIndex;
    if ifindex == 0 {
        return None;
    }
    let (ipv4_unicast_count, ipv4_preferred_count) = count_ipv4_unicast(adapter);
    Some(AdapterSnapshot {
        alias,
        adapter_name,
        ifindex,
        luid_value: adapter.Luid.Value,
        oper_status: adapter.OperStatus as u32,
        mtu: adapter.Mtu,
        ipv4_unicast_count,
        ipv4_preferred_count,
    })
}

unsafe fn count_ipv4_unicast(adapter: &IP_ADAPTER_ADDRESSES_LH) -> (usize, usize) {
    let mut count = 0usize;
    let mut preferred = 0usize;
    let mut current = adapter.FirstUnicastAddress;
    while !current.is_null() {
        let address = &*current;
        if !address.Address.lpSockaddr.is_null()
            && (*address.Address.lpSockaddr).sa_family == AF_INET
        {
            count += 1;
            if address.DadState == IpDadStatePreferred {
                preferred += 1;
            }
        }
        current = address.Next;
    }
    (count, preferred)
}

fn sockaddr_for_ipv4(ip: Ipv4Addr) -> SOCKADDR_INET {
    let octets = ip.octets();
    SOCKADDR_INET {
        Ipv4: SOCKADDR_IN {
            sin_family: AF_INET,
            sin_port: 0,
            sin_addr: IN_ADDR {
                S_un: IN_ADDR_0 {
                    S_un_b: IN_ADDR_0_0 {
                        s_b1: octets[0],
                        s_b2: octets[1],
                        s_b3: octets[2],
                        s_b4: octets[3],
                    },
                },
            },
            sin_zero: [0; 8],
        },
    }
}

fn luid_from_value(value: u64) -> NET_LUID_LH {
    NET_LUID_LH { Value: value }
}

fn register_network_watchers() -> Result<NetworkWatchHandles, String> {
    let mut interface_handle: HANDLE = ptr::null_mut();
    let interface_error = unsafe {
        NotifyIpInterfaceChange(
            AF_UNSPEC as ADDRESS_FAMILY,
            Some(ip_interface_change_callback),
            ptr::null(),
            1,
            &mut interface_handle,
        )
    };
    if interface_error != NO_ERROR {
        return Err(format!("NotifyIpInterfaceChange failed: {interface_error}"));
    }

    let mut unicast_handle: HANDLE = ptr::null_mut();
    let unicast_error = unsafe {
        NotifyUnicastIpAddressChange(
            AF_INET,
            Some(unicast_v4_change_callback),
            ptr::null(),
            1,
            &mut unicast_handle,
        )
    };
    if unicast_error != NO_ERROR {
        return Err(format!(
            "NotifyUnicastIpAddressChange(AF_INET) failed: {unicast_error}"
        ));
    }

    let mut route_handle: HANDLE = ptr::null_mut();
    let route_error = unsafe {
        NotifyRouteChange2(
            AF_INET,
            Some(route_v4_change_callback),
            ptr::null(),
            1,
            &mut route_handle,
        )
    };
    if route_error != NO_ERROR {
        return Err(format!("NotifyRouteChange2(AF_INET) failed: {route_error}"));
    }

    Ok(NetworkWatchHandles {
        interface: interface_handle,
        unicast_v4: unicast_handle,
        route_v4: route_handle,
    })
}

unsafe extern "system" fn ip_interface_change_callback(
    _caller_context: *const core::ffi::c_void,
    _row: *const MIB_IPINTERFACE_ROW,
    _notification_type: MIB_NOTIFICATION_TYPE,
) {
    INTERFACE_EVENT_SEQ.fetch_add(1, Ordering::SeqCst);
}

unsafe extern "system" fn unicast_v4_change_callback(
    _caller_context: *const core::ffi::c_void,
    _row: *const MIB_UNICASTIPADDRESS_ROW,
    _notification_type: MIB_NOTIFICATION_TYPE,
) {
    UNICAST_V4_EVENT_SEQ.fetch_add(1, Ordering::SeqCst);
}

unsafe extern "system" fn route_v4_change_callback(
    _caller_context: *const core::ffi::c_void,
    _row: *const MIB_IPFORWARD_ROW2,
    _notification_type: MIB_NOTIFICATION_TYPE,
) {
    ROUTE_V4_EVENT_SEQ.fetch_add(1, Ordering::SeqCst);
}

unsafe fn wide_ptr_to_string(ptr: windows_sys::core::PWSTR) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let mut len = 0usize;
    while *ptr.add(len) != 0 {
        len += 1;
    }
    Some(String::from_utf16_lossy(std::slice::from_raw_parts(
        ptr, len,
    )))
}

unsafe fn c_ptr_to_string(ptr: windows_sys::core::PSTR) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr as *const i8)
        .to_str()
        .ok()
        .map(str::to_string)
}

fn adapter_alias_matches(actual: &str, expected: &str) -> bool {
    actual.eq_ignore_ascii_case(expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_alias_matching_is_case_insensitive() {
        assert!(adapter_alias_matches(
            "DoodleRay Tunnel",
            "doodleray tunnel"
        ));
        assert!(!adapter_alias_matches("Other Tunnel", "DoodleRay Tunnel"));
    }

    #[test]
    fn sockaddr_for_ipv4_preserves_octets() {
        let addr = sockaddr_for_ipv4(Ipv4Addr::new(104, 26, 13, 205));
        let octets = unsafe {
            let b = addr.Ipv4.sin_addr.S_un.S_un_b;
            [b.s_b1, b.s_b2, b.s_b3, b.s_b4]
        };
        assert_eq!(octets, [104, 26, 13, 205]);
    }

    #[test]
    fn event_cursors_detect_changes() {
        let before = network_event_cursors();
        INTERFACE_EVENT_SEQ.fetch_add(1, Ordering::SeqCst);
        assert!(before.adapter_changed_since());
    }
}
