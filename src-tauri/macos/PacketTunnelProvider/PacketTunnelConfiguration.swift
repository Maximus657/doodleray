import Foundation

enum PacketTunnelConfigurationError: LocalizedError {
    case missingConfiguration
    case invalidConfiguration

    var errorDescription: String? {
        switch self {
        case .missingConfiguration:
            return "DoodleRay VPN configuration is missing. Start the tunnel from the app."
        case .invalidConfiguration:
            return "DoodleRay VPN configuration is invalid."
        }
    }
}

enum PacketTunnelConfiguration {
    static let optionKey = "xrayConfig"

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

    static func injectingTunnelFileDescriptor(_ descriptor: Int32, into config: [String: Any]) throws -> String {
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

    static func invocation(method: String, configJSON: String? = nil) throws -> String {
        var payload: [String: Any] = [:]
        if let configJSON {
            payload["configJSON"] = configJSON
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
}
