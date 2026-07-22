import Foundation
import LibXray

enum SmokeError: LocalizedError {
    case invalidArguments
    case invocationFailed(String)
    case probeFailed(String)
    case unexpectedResponse(String)

    var errorDescription: String? {
        switch self {
        case .invalidArguments:
            return "Expected an unused SOCKS port and HTTP target port."
        case let .invocationFailed(message):
            return "libXray failed to start: \(message)"
        case let .probeFailed(message):
            return "Loopback proxy probe failed: \(message)"
        case let .unexpectedResponse(response):
            return "Loopback proxy returned unexpected content: \(response)"
        }
    }
}

func invoke(_ method: String, payload: [String: Any] = [:]) throws -> [String: Any] {
    let request: [String: Any] = [
        "apiVersion": 1,
        "method": method,
        "payload": payload,
    ]
    let requestData = try JSONSerialization.data(withJSONObject: request)
    guard let requestJSON = String(data: requestData, encoding: .utf8) else {
        throw SmokeError.invocationFailed("request encoding failed")
    }

    let responseText = LibXrayInvoke(requestJSON)
    guard let responseData = responseText.data(using: .utf8),
          let response = try JSONSerialization.jsonObject(with: responseData) as? [String: Any]
    else {
        throw SmokeError.invocationFailed("invalid response")
    }
    return response
}

func run() throws {
    guard CommandLine.arguments.count == 3,
          let socksPort = Int(CommandLine.arguments[1]),
          let targetPort = Int(CommandLine.arguments[2]),
          (1 ... 65_535).contains(socksPort),
          (1 ... 65_535).contains(targetPort)
    else {
        throw SmokeError.invalidArguments
    }

    let temporaryDirectory = FileManager.default.temporaryDirectory
        .appendingPathComponent("doodleray-libxray-smoke-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: temporaryDirectory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: temporaryDirectory) }

    let expectedBody = "doodleray-libxray-loopback-ok\n"
    try expectedBody.write(
        to: temporaryDirectory.appendingPathComponent("probe.txt"),
        atomically: true,
        encoding: .utf8
    )

    let server = Process()
    server.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
    server.arguments = [
        "-m", "http.server", String(targetPort),
        "--bind", "127.0.0.1",
        "--directory", temporaryDirectory.path,
    ]
    server.standardOutput = FileHandle.nullDevice
    server.standardError = FileHandle.nullDevice
    try server.run()
    defer {
        if server.isRunning {
            server.terminate()
            server.waitUntilExit()
        }
    }

    let config: [String: Any] = [
        "log": ["loglevel": "warning"],
        "inbounds": [[
            "tag": "smoke-socks",
            "listen": "127.0.0.1",
            "port": socksPort,
            "protocol": "socks",
            "settings": ["udp": true],
        ]],
        "outbounds": [[
            "tag": "direct",
            "protocol": "freedom",
        ]],
    ]
    let configData = try JSONSerialization.data(withJSONObject: config)
    guard let configJSON = String(data: configData, encoding: .utf8) else {
        throw SmokeError.invocationFailed("configuration encoding failed")
    }

    var started = false
    defer {
        if started {
            _ = try? invoke("stopXray")
        }
    }

    let startResponse = try invoke("runXrayFromJson", payload: ["configJSON": configJSON])
    guard startResponse["success"] as? Bool == true else {
        throw SmokeError.invocationFailed(startResponse["error"] as? String ?? "unknown error")
    }
    started = true

    Thread.sleep(forTimeInterval: 0.6)
    let probe = Process()
    let output = Pipe()
    let errors = Pipe()
    probe.executableURL = URL(fileURLWithPath: "/usr/bin/curl")
    probe.arguments = [
        "--fail", "--silent", "--show-error",
        "--connect-timeout", "5", "--max-time", "10",
        "--socks5-hostname", "127.0.0.1:\(socksPort)",
        "http://127.0.0.1:\(targetPort)/probe.txt",
    ]
    probe.standardOutput = output
    probe.standardError = errors
    try probe.run()
    probe.waitUntilExit()

    let body = String(data: output.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
    let errorText = String(data: errors.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
    guard probe.terminationStatus == 0 else {
        throw SmokeError.probeFailed(errorText.trimmingCharacters(in: .whitespacesAndNewlines))
    }
    guard body == expectedBody else {
        throw SmokeError.unexpectedResponse(body)
    }

    let stopResponse = try invoke("stopXray")
    guard stopResponse["success"] as? Bool == true else {
        throw SmokeError.invocationFailed("stopXray failed")
    }
    started = false
    print("PASS  libXray starts, proxies loopback TCP, and stops without changing host routes.")
}

do {
    try run()
} catch {
    FileHandle.standardError.write(Data("FAIL  \(error.localizedDescription)\n".utf8))
    exit(1)
}
