import Darwin
import Foundation
import LibXray
import SystemConfiguration

struct PreparedPacketTunnelConfiguration {
    let xrayConfig: [String: Any]
    let excludedIPv4Addresses: [String]
    let excludedIPv6Addresses: [String]
}

enum PacketTunnelConfigurationError: LocalizedError {
    case missingConfiguration
    case invalidConfiguration
    case missingUplinkInterface
    case unresolvedUplink

    var errorDescription: String? {
        switch self {
        case .missingConfiguration:
            return "DoodleRay VPN configuration is missing. Start the tunnel from the app."
        case .invalidConfiguration:
            return "DoodleRay VPN configuration is invalid."
        case .missingUplinkInterface:
            return "DoodleRay VPN could not determine the active network interface."
        case .unresolvedUplink:
            return "DoodleRay VPN could not resolve the selected server before starting the tunnel."
        }
    }
}

enum PacketTunnelConfiguration {
    static let optionKey = "xrayConfig"
    static let fallbackDirectDNSServer = "77.88.8.8"

    static func decode(options: [String: NSObject]?) throws -> [String: Any] {
        guard let value = options?[optionKey] else {
            throw PacketTunnelConfigurationError.missingConfiguration
        }

        let data: Data
        if let encoded = value as? Data {
            data = encoded
        } else if let text = value as? String, let encoded = text.data(using: .utf8) {
            data = encoded
        } else {
            throw PacketTunnelConfigurationError.invalidConfiguration
        }

        guard data.count <= 1024 * 1024,
              let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              object["outbounds"] is [[String: Any]],
              object["inbounds"] is [[String: Any]]
        else {
            throw PacketTunnelConfigurationError.invalidConfiguration
        }
        return object
    }

    static func resolvingUplinks(in config: [String: Any]) throws -> PreparedPacketTunnelConfiguration {
        var result = config
        guard var outbounds = result["outbounds"] as? [[String: Any]] else {
            throw PacketTunnelConfigurationError.invalidConfiguration
        }

        let remoteProtocols = Set(["vless", "vmess", "trojan", "shadowsocks"])
        var excludedIPv4 = Set<String>()
        var excludedIPv6 = Set<String>()
        var resolvedEndpointCount = 0

        for outboundIndex in outbounds.indices {
            guard let protocolName = outbounds[outboundIndex]["protocol"] as? String,
                  remoteProtocols.contains(protocolName.lowercased()),
                  var settings = outbounds[outboundIndex]["settings"] as? [String: Any]
            else {
                continue
            }

            for endpointKey in ["vnext", "servers"] {
                guard var endpoints = settings[endpointKey] as? [[String: Any]] else {
                    continue
                }
                for endpointIndex in endpoints.indices {
                    guard let host = endpoints[endpointIndex]["address"] as? String,
                          !host.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                    else {
                        continue
                    }

                    let addresses = resolveNumericAddresses(host)
                    guard let selectedAddress = addresses.first else {
                        throw PacketTunnelConfigurationError.unresolvedUplink
                    }
                    endpoints[endpointIndex]["address"] = selectedAddress
                    resolvedEndpointCount += 1
                    if selectedAddress.contains(":") {
                        excludedIPv6.insert(selectedAddress)
                    } else {
                        excludedIPv4.insert(selectedAddress)
                    }
                }
                settings[endpointKey] = endpoints
            }
            outbounds[outboundIndex]["settings"] = settings
        }

        guard resolvedEndpointCount > 0 else {
            throw PacketTunnelConfigurationError.invalidConfiguration
        }
        result["outbounds"] = outbounds
        return PreparedPacketTunnelConfiguration(
            xrayConfig: result,
            excludedIPv4Addresses: excludedIPv4.sorted(),
            excludedIPv6Addresses: excludedIPv6.sorted()
        )
    }

    static func physicalDNSServers() -> [String] {
        guard let dns = SCDynamicStoreCopyValue(
            nil,
            "State:/Network/Global/DNS" as CFString
        ) as? [String: Any],
        let servers = dns["ServerAddresses"] as? [String]
        else {
            return []
        }
        return servers.filter { server in
            var ipv4 = in_addr()
            var ipv6 = in6_addr()
            return server.withCString { pointer in
                inet_pton(AF_INET, pointer, &ipv4) == 1 || inet_pton(AF_INET6, pointer, &ipv6) == 1
            }
        }
    }

    static func primaryPhysicalInterface() -> String? {
        for key in ["State:/Network/Global/IPv4", "State:/Network/Global/IPv6"] {
            guard let network = SCDynamicStoreCopyValue(nil, key as CFString) as? [String: Any],
                  let interface = network["PrimaryInterface"] as? String
            else {
                continue
            }
            let trimmed = interface.trimmingCharacters(in: .whitespacesAndNewlines)
            if !trimmed.isEmpty && !trimmed.hasPrefix("utun") {
                return trimmed
            }
        }
        return nil
    }

    static func injectingLocalDNSResolver(
        _ resolver: String,
        into config: [String: Any]
    ) -> [String: Any] {
        var result = config
        guard var dns = result["dns"] as? [String: Any],
              var servers = dns["servers"] as? [Any]
        else {
            return result
        }
        for index in servers.indices {
            if var server = servers[index] as? [String: Any],
               server["address"] as? String == "localhost"
            {
                server["address"] = resolver
                servers[index] = server
            } else if let server = servers[index] as? String, server == "localhost" {
                servers[index] = resolver
            }
        }
        dns["servers"] = servers
        result["dns"] = dns
        return result
    }

    static func injectingDirectOutboundInterface(
        _ interface: String,
        into config: [String: Any]
    ) -> [String: Any] {
        var result = config
        guard var outbounds = result["outbounds"] as? [[String: Any]] else {
            return result
        }
        for index in outbounds.indices where outbounds[index]["tag"] as? String == "direct"
            && outbounds[index]["protocol"] as? String == "freedom"
        {
            var streamSettings = outbounds[index]["streamSettings"] as? [String: Any] ?? [:]
            var sockopt = streamSettings["sockopt"] as? [String: Any] ?? [:]
            sockopt["interface"] = interface
            streamSettings["sockopt"] = sockopt
            outbounds[index]["streamSettings"] = streamSettings
        }
        result["outbounds"] = outbounds
        return result
    }

    private static func resolveNumericAddresses(_ host: String) -> [String] {
        var hints = addrinfo()
        hints.ai_flags = AI_ADDRCONFIG
        hints.ai_family = AF_UNSPEC
        hints.ai_socktype = SOCK_STREAM
        hints.ai_protocol = IPPROTO_TCP

        var result: UnsafeMutablePointer<addrinfo>?
        guard getaddrinfo(host, nil, &hints, &result) == 0, let first = result else {
            return []
        }
        defer { freeaddrinfo(first) }

        var ipv4: [String] = []
        var ipv6: [String] = []
        var cursor: UnsafeMutablePointer<addrinfo>? = first
        while let entry = cursor {
            defer { cursor = entry.pointee.ai_next }
            guard entry.pointee.ai_family == AF_INET || entry.pointee.ai_family == AF_INET6,
                  let socketAddress = entry.pointee.ai_addr
            else {
                continue
            }

            var buffer = [CChar](repeating: 0, count: Int(NI_MAXHOST))
            guard getnameinfo(
                socketAddress,
                entry.pointee.ai_addrlen,
                &buffer,
                socklen_t(buffer.count),
                nil,
                0,
                NI_NUMERICHOST
            ) == 0 else {
                continue
            }
            let address = String(cString: buffer).split(separator: "%", maxSplits: 1).first.map(String.init) ?? ""
            guard !address.isEmpty else { continue }
            if entry.pointee.ai_family == AF_INET {
                if !ipv4.contains(address) { ipv4.append(address) }
            } else if !ipv6.contains(address) {
                ipv6.append(address)
            }
        }
        return ipv4 + ipv6
    }

    static func injectingTunnelFileDescriptor(
        _ descriptor: Int32,
        into config: [String: Any]
    ) throws -> String {
        guard descriptor >= 3 else {
            throw PacketTunnelConfigurationError.invalidConfiguration
        }

        var result = config
        var environment = result["env"] as? [String: String] ?? [:]
        environment["xray.tun.fd"] = String(descriptor)
        result["env"] = environment

        // App Store builds never write Xray access/error logs to disk. Runtime
        // errors are reported through the provider status only.
        result["log"] = ["loglevel": "warning"]

        let data = try JSONSerialization.data(withJSONObject: result, options: [])
        guard let json = String(data: data, encoding: .utf8) else {
            throw PacketTunnelConfigurationError.invalidConfiguration
        }
        return json
    }

    static func validateXrayConfig(_ configJSON: String) throws {
        let configURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("doodleray-xray-\(UUID().uuidString).json")
        defer { try? FileManager.default.removeItem(at: configURL) }
        guard let data = configJSON.data(using: .utf8) else {
            throw PacketTunnelConfigurationError.invalidConfiguration
        }
        try data.write(to: configURL, options: [.atomic, .completeFileProtection])
        let request = try invocation(method: "testXray", configPath: configURL.path)
        let response = LibXrayInvoke(request)
        guard invocationSucceeded(response) else {
            throw PacketTunnelConfigurationError.invalidConfiguration
        }
    }

    static func invocation(
        method: String,
        configJSON: String? = nil,
        configPath: String? = nil
    ) throws -> String {
        var payload: [String: Any] = [:]
        if let configJSON {
            payload["configJSON"] = configJSON
        }
        if let configPath {
            payload["configPath"] = configPath
        }
        let request: [String: Any] = [
            "apiVersion": 1,
            "method": method,
            "payload": payload,
        ]
        let data = try JSONSerialization.data(withJSONObject: request, options: [])
        guard let json = String(data: data, encoding: .utf8) else {
            throw PacketTunnelConfigurationError.invalidConfiguration
        }
        return json
    }

    static func invocationSucceeded(_ response: String) -> Bool {
        guard let data = response.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else {
            return false
        }
        return object["success"] as? Bool == true
    }

    static func invocationFailureSummary(_ response: String) -> String {
        guard let data = response.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else {
            return "invalid libXray response"
        }
        var summary = (object["error"] as? String)
            ?? (object["message"] as? String)
            ?? "unknown libXray error"
        let redactions: [(String, String)] = [
            (#"https?://[^\s]+"#, "[url]"),
            (#"\b(?:\d{1,3}\.){3}\d{1,3}\b"#, "[address]"),
            (#"\b[0-9A-Fa-f]{8}(?:-[0-9A-Fa-f]{4}){3}-[0-9A-Fa-f]{12}\b"#, "[id]"),
            (#"\b(?:[A-Za-z0-9-]+\.)+[A-Za-z]{2,}\b"#, "[host]"),
            (#"\b[A-Za-z0-9_+/=-]{24,}\b"#, "[value]"),
            (#"(?i)\b(?:password|token|uuid|public_key|short_id|private_key|key)\s*[:=]\s*[^,\s]+"#, "[credential]"),
        ]
        for (pattern, replacement) in redactions {
            summary = summary.replacingOccurrences(
                of: pattern,
                with: replacement,
                options: .regularExpression
            )
        }
        summary = summary.replacingOccurrences(
            of: #"[\u0000-\u001F\u007F]+"#,
            with: " ",
            options: .regularExpression
        )
        if summary.count > 320 {
            summary = String(summary.prefix(320)) + "…"
        }
        return summary
    }
}
