import Darwin
import Foundation
import LibXray
import NetworkExtension
import os

final class PacketTunnelProvider: NEPacketTunnelProvider {
    private let logger = Logger(subsystem: "com.doodleray.doodleray", category: "PacketTunnelProvider")
    private var xrayRunning = false

    override func startTunnel(
        options: [String: NSObject]?,
        completionHandler: @escaping (Error?) -> Void
    ) {
        do {
            let config = try PacketTunnelConfiguration.decode(options: options)
            guard let uplinkInterface = PacketTunnelConfiguration.primaryPhysicalInterface() else {
                throw PacketTunnelConfigurationError.missingUplinkInterface
            }
            let dnsPrepared = PacketTunnelConfiguration.injectingLocalDNSResolver(
                "https://1.1.1.1/dns-query",
                into: config
            )
            let directPrepared = PacketTunnelConfiguration.injectingDirectOutboundInterface(
                uplinkInterface,
                into: dnsPrepared
            )
            let prepared = try PacketTunnelConfiguration.resolvingUplinks(in: directPrepared)
            let validationJSON = try PacketTunnelConfiguration.injectingTunnelFileDescriptor(
                3,
                into: prepared.xrayConfig
            )
            try PacketTunnelConfiguration.validateXrayConfig(validationJSON)
            let settings = makeNetworkSettings(
                excludingIPv4: prepared.excludedIPv4Addresses,
                excludingIPv6: prepared.excludedIPv6Addresses
            )
            logger.notice(
                "Prepared packet tunnel on \(uplinkInterface, privacy: .public) with local DNS and \(prepared.excludedIPv4Addresses.count, privacy: .public) IPv4 and \(prepared.excludedIPv6Addresses.count, privacy: .public) IPv6 uplink exclusions"
            )
            setTunnelNetworkSettings(settings) { [weak self] error in
                guard let self else {
                    completionHandler(PacketTunnelConfigurationError.invalidConfiguration)
                    return
                }
                if let error {
                    self.logger.error("Network settings failed: \(error.localizedDescription, privacy: .public)")
                    completionHandler(error)
                    return
                }

                do {
                    guard let descriptor = self.tunnelFileDescriptor() else {
                        self.logger.error("Packet tunnel file descriptor was not found")
                        throw PacketTunnelConfigurationError.invalidConfiguration
                    }
                    let xrayConfig = try PacketTunnelConfiguration.injectingTunnelFileDescriptor(
                        descriptor,
                        into: prepared.xrayConfig
                    )
                    let request = try PacketTunnelConfiguration.invocation(
                        method: "runXrayFromJson",
                        configJSON: xrayConfig
                    )
                    let response = LibXrayInvoke(request)
                    guard PacketTunnelConfiguration.invocationSucceeded(response) else {
                        let summary = PacketTunnelConfiguration.invocationFailureSummary(response)
                        self.logger.error("libXray rejected the packet tunnel configuration: \(summary, privacy: .public)")
                        throw PacketTunnelConfigurationError.invalidConfiguration
                    }
                    self.xrayRunning = true
                    self.logger.notice("Packet tunnel started")
                    completionHandler(nil)
                } catch {
                    self.logger.error("Packet tunnel startup failed: \(error.localizedDescription, privacy: .public)")
                    self.stopXray()
                    self.setTunnelNetworkSettings(nil) { _ in
                        completionHandler(error)
                    }
                }
            }
        } catch {
            logger.error("Packet tunnel option decoding failed: \(error.localizedDescription, privacy: .public)")
            completionHandler(error)
        }
    }

    override func stopTunnel(
        with reason: NEProviderStopReason,
        completionHandler: @escaping () -> Void
    ) {
        logger.notice("Stopping packet tunnel, reason=\(reason.rawValue, privacy: .public)")
        stopXray()
        setTunnelNetworkSettings(nil) { _ in completionHandler() }
    }

    override func handleAppMessage(_ messageData: Data, completionHandler: ((Data?) -> Void)? = nil) {
        let response: [String: Any] = ["running": xrayRunning]
        completionHandler?(try? JSONSerialization.data(withJSONObject: response, options: []))
    }

    private func makeNetworkSettings(
        excludingIPv4 excludedIPv4Addresses: [String],
        excludingIPv6 excludedIPv6Addresses: [String]
    ) -> NEPacketTunnelNetworkSettings {
        let settings = NEPacketTunnelNetworkSettings(tunnelRemoteAddress: "172.30.255.1")
        settings.mtu = 1408
        settings.tunnelOverheadBytes = 80

        let ipv4 = NEIPv4Settings(addresses: ["172.30.255.2"], subnetMasks: ["255.255.255.252"])
        ipv4.includedRoutes = [.default()]
        ipv4.excludedRoutes = excludedIPv4Addresses.map {
            NEIPv4Route(destinationAddress: $0, subnetMask: "255.255.255.255")
        }
        settings.ipv4Settings = ipv4

        let ipv6 = NEIPv6Settings(
            addresses: ["fdfe:dcba:9876::2"],
            networkPrefixLengths: [126]
        )
        ipv6.includedRoutes = [.default()]
        ipv6.excludedRoutes = excludedIPv6Addresses.map {
            NEIPv6Route(destinationAddress: $0, networkPrefixLength: 128)
        }
        settings.ipv6Settings = ipv6

        let dns = NEDNSSettings(servers: ["1.1.1.1", "9.9.9.9"])
        dns.matchDomains = [""]
        dns.matchDomainsNoSearch = true
        settings.dnsSettings = dns
        return settings
    }

    private func tunnelFileDescriptor() -> Int32? {
        var name = [CChar](repeating: 0, count: Int(IFNAMSIZ))
        for descriptor in Int32(3) ... Int32(1024) {
            _ = name.withUnsafeMutableBytes { $0.initializeMemory(as: UInt8.self, repeating: 0) }
            var length = socklen_t(name.count)
            let result = getsockopt(descriptor, 2, 2, &name, &length)
            if result == 0, String(cString: name).hasPrefix("utun") {
                return descriptor
            }
        }
        return nil
    }

    private func stopXray() {
        guard xrayRunning else { return }
        if let request = try? PacketTunnelConfiguration.invocation(method: "stopXray") {
            _ = LibXrayInvoke(request)
        }
        xrayRunning = false
    }
}
