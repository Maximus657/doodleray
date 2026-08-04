use crate::ConnectRequest;
use std::collections::HashSet;

pub(crate) const DEFAULT_DIRECT_DOMAIN_SUFFIXES: &[&str] = &[
    "2ip.ru",
    "vk.com",
    "vk.ru",
    "ok.ru",
    "mail.ru",
    "yandex.ru",
    "yandex.com",
    "yandex.net",
    "ya.ru",
    "dzen.ru",
    "rutube.ru",
    "gosuslugi.ru",
    "mos.ru",
    "nalog.gov.ru",
    "sberbank.ru",
    "sber.ru",
    "tbank.ru",
    "tinkoff.ru",
    "alfabank.ru",
];

const DEFAULT_DIRECT_XRAY_DOMAIN_REGEXES: &[&str] = &[
    r"regexp:.*\.ru$",
    r"regexp:.*\.su$",
    r"regexp:.*\.xn--p1ai$",
    r"regexp:.*\.xn--p1acf$",
    r"regexp:.*\.moscow$",
    r"regexp:.*\.xn--80adxhks$",
];

const STEAM_DIRECT_XRAY_DOMAINS: &[&str] = &[
    "domain:steampowered.com",
    "domain:steamcommunity.com",
    "domain:steamgames.com",
    "domain:steamusercontent.com",
    "domain:steamcontent.com",
    "domain:steamstatic.com",
    "full:steamcdn-a.akamaihd.net",
];

fn with_steam_direct_domains(mut domains: Vec<String>) -> Vec<String> {
    domains.extend(
        STEAM_DIRECT_XRAY_DOMAINS
            .iter()
            .map(|domain| (*domain).to_string()),
    );
    domains.sort();
    domains.dedup();
    domains
}

pub(crate) fn routing_policy_xray_domains(req: &ConnectRequest) -> Vec<String> {
    with_steam_direct_domains(match req.routing_policy.as_ref() {
        Some(policy) if policy.mode == "split" => policy.direct_domains.clone(),
        Some(_) => Vec::new(),
        None => default_direct_xray_domains(),
    })
}

pub(crate) fn routing_policy_xray_dns_domains(req: &ConnectRequest) -> Vec<String> {
    with_steam_direct_domains(match req.routing_policy.as_ref() {
        Some(policy) if policy.mode == "split" && !policy.local_dns_domains.is_empty() => {
            policy.local_dns_domains.clone()
        }
        Some(policy) if policy.mode == "split" => policy.direct_domains.clone(),
        Some(_) => Vec::new(),
        None => default_direct_xray_domains(),
    })
}

fn default_direct_xray_domains() -> Vec<String> {
    DEFAULT_DIRECT_XRAY_DOMAIN_REGEXES
        .iter()
        .map(|value| (*value).to_string())
        .chain(
            DEFAULT_DIRECT_DOMAIN_SUFFIXES
                .iter()
                .map(|value| format!("domain:{}", value)),
        )
        .collect()
}

pub(crate) fn xray_rule_has_default_direct_domains(rule: &serde_json::Value) -> bool {
    rule.get("outboundTag").and_then(|value| value.as_str()) == Some("direct")
        && rule
            .get("domain")
            .and_then(|value| value.as_array())
            .map(|domains| {
                domains
                    .iter()
                    .any(|value| value.as_str() == Some("domain:2ip.ru"))
            })
            .unwrap_or(false)
}

fn ensure_xray_direct_outbound(config: &mut serde_json::Value) {
    let direct_outbound = serde_json::json!({
        "tag": "direct",
        "protocol": "freedom"
    });

    let Some(outbounds) = config
        .get_mut("outbounds")
        .and_then(|value| value.as_array_mut())
    else {
        config["outbounds"] = serde_json::json!([direct_outbound]);
        return;
    };

    let has_direct = outbounds
        .iter()
        .any(|outbound| outbound.get("tag").and_then(|value| value.as_str()) == Some("direct"));
    if !has_direct {
        outbounds.push(direct_outbound);
    }
}

fn ensure_xray_dns_outbound(config: &mut serde_json::Value) {
    let dns_outbound = serde_json::json!({ "tag": "dns-out", "protocol": "dns" });
    let Some(outbounds) = config
        .get_mut("outbounds")
        .and_then(serde_json::Value::as_array_mut)
    else {
        config["outbounds"] = serde_json::json!([dns_outbound]);
        return;
    };
    if !outbounds
        .iter()
        .any(|outbound| outbound.get("tag").and_then(serde_json::Value::as_str) == Some("dns-out"))
    {
        outbounds.push(dns_outbound);
    }
}

fn ensure_xray_api_outbound(config: &mut serde_json::Value) {
    let api_outbound = serde_json::json!({ "tag": "api", "protocol": "blackhole" });
    let Some(outbounds) = config
        .get_mut("outbounds")
        .and_then(serde_json::Value::as_array_mut)
    else {
        config["outbounds"] = serde_json::json!([api_outbound]);
        return;
    };
    if !outbounds
        .iter()
        .any(|outbound| outbound.get("tag").and_then(serde_json::Value::as_str) == Some("api"))
    {
        outbounds.push(api_outbound);
    }
}

fn is_managed_xray_candidate(outbound: &serde_json::Value) -> bool {
    outbound
        .get("tag")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|tag| tag.starts_with("entry-"))
        && outbound.get("protocol").and_then(serde_json::Value::as_str) == Some("vless")
        && outbound
            .get("streamSettings")
            .and_then(|stream| stream.get("network"))
            .and_then(serde_json::Value::as_str)
            == Some("xhttp")
        && outbound
            .get("streamSettings")
            .and_then(|stream| stream.get("security"))
            .and_then(serde_json::Value::as_str)
            == Some("tls")
}

pub(crate) fn is_managed_xray_balancer_config(config: &serde_json::Value) -> bool {
    let candidate_count = config
        .get("outbounds")
        .and_then(serde_json::Value::as_array)
        .map(|outbounds| {
            outbounds
                .iter()
                .filter(|outbound| is_managed_xray_candidate(outbound))
                .count()
        })
        .unwrap_or_default();
    if candidate_count < 2 {
        return false;
    }

    let has_balancer = config
        .get("routing")
        .and_then(|routing| routing.get("balancers"))
        .and_then(serde_json::Value::as_array)
        .is_some_and(|balancers| {
            balancers.iter().any(|balancer| {
                balancer.get("tag").and_then(serde_json::Value::as_str) == Some("balancer")
                    && balancer
                        .get("selector")
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(|selector| {
                            selector.len() == 1 && selector[0].as_str() == Some("entry-")
                        })
                    && balancer
                        .get("strategy")
                        .and_then(|strategy| strategy.get("type"))
                        .and_then(serde_json::Value::as_str)
                        == Some("leastPing")
            })
        });
    let has_observatory = config.get("burstObservatory").is_some_and(|observatory| {
        observatory
            .get("subjectSelector")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|selector| selector.len() == 1 && selector[0].as_str() == Some("entry-"))
            && observatory
                .get("pingConfig")
                .and_then(|ping| ping.get("destination"))
                .and_then(serde_json::Value::as_str)
                == Some("https://connectivitycheck.gstatic.com/generate_204")
    });
    has_balancer && has_observatory
}

fn constrain_xray_config_to_managed_policy(config: &mut serde_json::Value, req: &ConnectRequest) {
    if req.routing_policy.is_none() {
        return;
    }

    if is_managed_xray_balancer_config(config) {
        if let Some(outbounds) = config
            .get_mut("outbounds")
            .and_then(serde_json::Value::as_array_mut)
        {
            outbounds.retain(|outbound| {
                let tag = outbound
                    .get("tag")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                let protocol = outbound
                    .get("protocol")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                is_managed_xray_candidate(outbound)
                    || (tag == "direct" && protocol == "freedom")
                    || tag == "dns-out"
                    || tag == "api"
                    || protocol == "blackhole"
            });
        }
        if let Some(routing) = config
            .get_mut("routing")
            .and_then(serde_json::Value::as_object_mut)
        {
            if let Some(balancers) = routing
                .get_mut("balancers")
                .and_then(serde_json::Value::as_array_mut)
            {
                balancers.retain(|balancer| {
                    balancer.get("tag").and_then(serde_json::Value::as_str) == Some("balancer")
                });
            }
            routing.insert("rules".into(), serde_json::json!([]));
        }
        return;
    }

    let mut allowed_tags = HashSet::new();
    if let Some(outbounds) = config
        .get_mut("outbounds")
        .and_then(serde_json::Value::as_array_mut)
    {
        outbounds.retain(|outbound| {
            let tag = outbound
                .get("tag")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let protocol = outbound
                .get("protocol")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let keep = tag == "proxy"
                || (tag == "direct" && protocol == "freedom")
                || tag == "dns-out"
                || tag == "api"
                || protocol == "blackhole";
            if keep && !tag.is_empty() {
                allowed_tags.insert(tag.to_string());
            }
            keep
        });
        if let Some(index) = outbounds.iter().position(|outbound| {
            outbound.get("tag").and_then(serde_json::Value::as_str) == Some("proxy")
        }) {
            if index != 0 {
                let proxy = outbounds.remove(index);
                outbounds.insert(0, proxy);
            }
        }
    }

    if let Some(routing) = config
        .get_mut("routing")
        .and_then(serde_json::Value::as_object_mut)
    {
        routing.remove("balancers");
        if let Some(rules) = routing
            .get_mut("rules")
            .and_then(serde_json::Value::as_array_mut)
        {
            rules.retain(|rule| {
                if rule.get("balancerTag").is_some() {
                    return false;
                }
                rule.get("outboundTag")
                    .and_then(serde_json::Value::as_str)
                    .is_none_or(|tag| allowed_tags.contains(tag))
            });
        }
    }
}

fn apply_xray_routing_policy(
    config: &mut serde_json::Value,
    req: &ConnectRequest,
    include_legacy_default_split: bool,
) {
    let managed_balancer = is_managed_xray_balancer_config(config);
    if !config
        .get("routing")
        .map(|value| value.is_object())
        .unwrap_or(false)
    {
        config["routing"] = serde_json::json!({});
    }
    if !config["routing"]
        .get("rules")
        .map(|value| value.is_array())
        .unwrap_or(false)
    {
        config["routing"]["rules"] = serde_json::json!([]);
    }

    let Some(rules) = config["routing"]["rules"].as_array_mut() else {
        return;
    };

    rules.retain(|rule| {
        let inbound = rule
            .get("inboundTag")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|tags| {
                tags.iter()
                    .any(|tag| matches!(tag.as_str(), Some("dns-direct" | "dns-remote")))
            });
        if inbound {
            return false;
        }
        if req.routing_policy.is_none() {
            return true;
        }
        rule.get("outboundTag").and_then(serde_json::Value::as_str) != Some("direct")
    });

    if !rules.iter().any(|rule| {
        rule.get("inboundTag")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|tags| tags.iter().any(|tag| tag.as_str() == Some("api")))
    }) {
        rules.insert(
            0,
            serde_json::json!({
                "type": "field",
                "inboundTag": ["api"],
                "outboundTag": "api"
            }),
        );
    }

    let insert_at = rules
        .iter()
        .position(|rule| {
            rule.get("inboundTag")
                .and_then(|value| value.as_array())
                .map(|tags| tags.iter().any(|tag| tag.as_str() == Some("api")))
                .unwrap_or(false)
        })
        .map(|index| index + 1)
        .unwrap_or(0);
    let mut additions = Vec::new();
    let managed_dns = req.routing_policy.is_some() || include_legacy_default_split;
    let dns_domains = if managed_dns {
        routing_policy_xray_dns_domains(req)
    } else {
        Vec::new()
    };
    if !dns_domains.is_empty() {
        additions.push(serde_json::json!({
            "type": "field",
            "inboundTag": ["dns-direct"],
            "outboundTag": "direct"
        }));
    }
    if managed_dns {
        let mut rule = serde_json::json!({
            "type": "field",
            "inboundTag": ["dns-remote"]
        });
        rule[if managed_balancer {
            "balancerTag"
        } else {
            "outboundTag"
        }] = serde_json::json!(if managed_balancer {
            "balancer"
        } else {
            "proxy"
        });
        additions.push(rule);
    }
    // Resolver-originated DNS traffic must be classified before the generic
    // port-53 interception rule. Otherwise a local resolver query is sent back
    // into dns-out recursively and direct domains such as .ru never resolve.
    if !rules.iter().any(|rule| {
        rule.get("port").and_then(serde_json::Value::as_str) == Some("53")
            && rule.get("outboundTag").and_then(serde_json::Value::as_str) == Some("dns-out")
    }) {
        additions.push(serde_json::json!({
            "type": "field",
            "port": "53",
            "outboundTag": "dns-out"
        }));
    }
    let direct_domains = if req.routing_policy.is_some() || include_legacy_default_split {
        routing_policy_xray_domains(req)
    } else {
        Vec::new()
    };
    if !direct_domains.is_empty()
        && !rules
            .iter()
            .any(|rule| req.routing_policy.is_none() && xray_rule_has_default_direct_domains(rule))
    {
        additions.push(serde_json::json!({
            "type": "field",
            "domain": direct_domains,
            "outboundTag": "direct"
        }));
    }
    if include_legacy_default_split
        && req.routing_policy.is_none()
        && !rules.iter().any(|rule| {
            rule.get("outboundTag").and_then(serde_json::Value::as_str) == Some("direct")
                && rule
                    .get("ip")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|values| {
                        values
                            .iter()
                            .any(|value| value.as_str() == Some("geoip:private"))
                    })
        })
    {
        additions.push(serde_json::json!({
            "type": "field",
            "ip": ["geoip:private"],
            "outboundTag": "direct"
        }));
    }
    if let Some(policy) = req
        .routing_policy
        .as_ref()
        .filter(|policy| policy.mode == "split")
    {
        if !policy.direct_ip_ranges.is_empty() {
            additions.push(serde_json::json!({
                "type": "field",
                "ip": policy.direct_ip_ranges,
                "outboundTag": "direct"
            }));
        }
    }
    for (offset, rule) in additions.into_iter().enumerate() {
        rules.insert(insert_at + offset, rule);
    }
    if req.routing_policy.is_some() {
        let mut rule = serde_json::json!({
            "type": "field",
            "network": "tcp,udp"
        });
        rule[if managed_balancer {
            "balancerTag"
        } else {
            "outboundTag"
        }] = serde_json::json!(if managed_balancer {
            "balancer"
        } else {
            "proxy"
        });
        rules.push(rule);
    }
}

fn xray_dns_config(req: &ConnectRequest) -> serde_json::Value {
    let mut servers = Vec::new();
    let direct_domains = routing_policy_xray_dns_domains(req);
    if !direct_domains.is_empty() {
        servers.push(serde_json::json!({
            "address": "localhost",
            "domains": direct_domains,
            "skipFallback": true,
            "tag": "dns-direct"
        }));
    }
    servers.push(serde_json::json!({
        "address": "https://1.1.1.1/dns-query",
        "tag": "dns-remote"
    }));
    serde_json::json!({
        "servers": servers,
        "queryStrategy": "UseIPv4",
        "disableFallbackIfMatch": true
    })
}

fn xray_tunnel_dns_config() -> serde_json::Value {
    serde_json::json!({
        "queryStrategy": "UseIPv4",
        "servers": [{
            "address": "https://1.1.1.1/dns-query",
            "tag": "dns-remote"
        }]
    })
}

fn xray_engine_transport(transport: &str) -> bool {
    matches!(transport, "xhttp" | "ws")
}

fn xray_engine_protocol(protocol: &str) -> bool {
    matches!(protocol, "vless" | "vmess" | "trojan" | "shadowsocks")
}

/// Every protocol either engine's config builder actually implements. A
/// request outside this set must be rejected before it reaches sing-box's
/// outbound builder, whose fallback for an unrecognized protocol blocks
/// (fails closed) rather than silently sending traffic unproxied.
pub(crate) fn is_supported_proxy_protocol(protocol: &str) -> bool {
    xray_engine_protocol(protocol) || matches!(protocol, "hysteria2" | "tuic" | "wireguard")
}

pub(crate) fn uses_xray_engine(req: &ConnectRequest) -> bool {
    req.raw_xray_config.is_some()
        || xray_engine_transport(req.transport.as_str())
        || xray_engine_protocol(req.protocol.as_str())
}

fn xray_transport_host(req: &ConnectRequest) -> String {
    req.host
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .or(req.sni.as_ref().filter(|value| !value.trim().is_empty()))
        .cloned()
        .unwrap_or_else(|| req.server_address.clone())
}

fn xray_tls_settings(req: &ConnectRequest) -> serde_json::Value {
    let mut settings = serde_json::json!({
        "serverName": req.sni.clone().unwrap_or(req.server_address.clone()),
        "fingerprint": req.fingerprint.clone().unwrap_or("chrome".into())
    });
    if let Some(ref alpn) = req.alpn {
        if !alpn.is_empty() {
            settings["alpn"] = serde_json::json!(alpn);
        }
    }
    settings
}

fn xray_reality_settings(req: &ConnectRequest) -> serde_json::Value {
    serde_json::json!({
        "serverName": req.sni.clone().unwrap_or(req.server_address.clone()),
        "publicKey": req.public_key.clone().unwrap_or_default(),
        "shortId": req.short_id.clone().unwrap_or_default(),
        "fingerprint": req.fingerprint.clone().unwrap_or("chrome".into())
    })
}

fn apply_xray_stream_security_settings(
    stream_settings: &mut serde_json::Value,
    req: &ConnectRequest,
) {
    if req.security == "reality" {
        stream_settings["realitySettings"] = xray_reality_settings(req);
    } else if req.security == "tls" {
        stream_settings["tlsSettings"] = xray_tls_settings(req);
    }
}

fn normalize_xray_transport_settings(config: &mut serde_json::Value) {
    let Some(outbounds) = config
        .get_mut("outbounds")
        .and_then(|value| value.as_array_mut())
    else {
        return;
    };

    for outbound in outbounds {
        let Some(stream_settings) = outbound
            .get_mut("streamSettings")
            .and_then(|value| value.as_object_mut())
        else {
            continue;
        };
        let Some(ws_settings) = stream_settings
            .get_mut("wsSettings")
            .and_then(|value| value.as_object_mut())
        else {
            continue;
        };

        let header_host = {
            let headers = ws_settings
                .get_mut("headers")
                .and_then(|value| value.as_object_mut());
            headers.and_then(|headers| headers.remove("Host").or_else(|| headers.remove("host")))
        };

        if let Some(host) = header_host {
            ws_settings.entry("host").or_insert(host);
        }

        let remove_headers = ws_settings
            .get("headers")
            .and_then(|value| value.as_object())
            .map(|headers| headers.is_empty())
            .unwrap_or(false);
        if remove_headers {
            ws_settings.remove("headers");
        }
    }
}

/// Take a raw xray JSON config (from DoodleVPN subscription) and inject
/// DoodleRay's inbounds (SOCKS, HTTP, stats API) so it uses the correct ports.
/// Preserves all outbounds, routing, observatory, balancing etc. from the original.
pub(crate) fn inject_xray_inbounds(
    mut config: serde_json::Value,
    req: &ConnectRequest,
) -> serde_json::Value {
    // Replace or add inbounds with DoodleRay's SOCKS/HTTP/API ports
    let inbounds = serde_json::json!([
        {
            "tag": "socks-in",
            "port": req.socks_port,
            "listen": "127.0.0.1",
            "protocol": "socks",
            "settings": { "udp": true, "ip": "127.0.0.1" },
            "sniffing": {
                "enabled": true,
                "destOverride": ["http", "tls", "quic", "fakedns"],
                "routeOnly": true
            }
        },
        {
            "tag": "http-in",
            "port": req.http_port,
            "listen": "127.0.0.1",
            "protocol": "http"
        },
        {
            "tag": "api",
            "port": req.api_port,
            "listen": "127.0.0.1",
            "protocol": "dokodemo-door",
            "settings": { "address": "127.0.0.1" }
        }
    ]);
    config["inbounds"] = inbounds;
    if req.routing_policy.is_some() {
        config["dns"] = xray_dns_config(req);
    } else if config.get("dns").is_none() {
        config["dns"] = xray_tunnel_dns_config();
    }

    // Ensure stats/api/policy exist for traffic monitoring
    if config.get("stats").is_none() {
        config["stats"] = serde_json::json!({});
    }
    if config.get("api").is_none() {
        config["api"] = serde_json::json!({
            "tag": "api",
            "services": ["StatsService"]
        });
    }
    if config.get("policy").is_none() {
        config["policy"] = serde_json::json!({
            "system": {
                "statsInboundUplink": true,
                "statsInboundDownlink": true,
                "statsOutboundUplink": true,
                "statsOutboundDownlink": true
            }
        });
    }

    normalize_xray_transport_settings(&mut config);
    sanitize_xray_routing_rules(&mut config);
    constrain_xray_config_to_managed_policy(&mut config, req);
    ensure_xray_direct_outbound(&mut config);
    ensure_xray_dns_outbound(&mut config);
    ensure_xray_api_outbound(&mut config);

    // Make sure routing rules include the API rule
    if let Some(routing) = config.get_mut("routing") {
        if let Some(rules) = routing.get_mut("rules") {
            if let Some(rules_arr) = rules.as_array_mut() {
                let has_api_rule = rules_arr.iter().any(|r| {
                    r.get("inboundTag")
                        .and_then(|t| t.as_array())
                        .map(|arr| arr.iter().any(|v| v.as_str() == Some("api")))
                        .unwrap_or(false)
                });
                if !has_api_rule {
                    rules_arr.insert(
                        0,
                        serde_json::json!({
                            "type": "field",
                            "inboundTag": ["api"],
                            "outboundTag": "api"
                        }),
                    );
                }
            }
        }
    }

    apply_xray_routing_policy(&mut config, req, false);

    config
}

pub(crate) fn sanitize_xray_routing_rules(config: &mut serde_json::Value) {
    let Some(rules) = config
        .get_mut("routing")
        .and_then(|routing| routing.get_mut("rules"))
        .and_then(|rules| rules.as_array_mut())
    else {
        return;
    };

    for rule in rules.iter_mut() {
        remove_unsupported_xray_rule_values(
            rule.get_mut("domain"),
            &[
                "geosite:category-bittorrent",
                "geosite:torrent",
                "geosite:twitch-ads",
                "geosite:whitelist",
                "geosite:faceit",
            ],
        );
        remove_unsupported_xray_rule_values(rule.get_mut("ip"), &["geoip:direct"]);
        remove_empty_xray_rule_array(rule, "domain");
        remove_empty_xray_rule_array(rule, "ip");
    }

    rules.retain(has_effective_xray_rule_fields);
}

fn remove_unsupported_xray_rule_values(
    value: Option<&mut serde_json::Value>,
    unsupported: &[&str],
) {
    let Some(values) = value.and_then(|v| v.as_array_mut()) else {
        return;
    };

    values.retain(|item| {
        item.as_str()
            .map(|s| !unsupported.iter().any(|bad| s.eq_ignore_ascii_case(bad)))
            .unwrap_or(true)
    });
}

pub(crate) fn remove_empty_xray_rule_array(rule: &mut serde_json::Value, key: &str) {
    let should_remove = rule
        .get(key)
        .and_then(|value| value.as_array())
        .map(|values| values.is_empty())
        .unwrap_or(false);

    if should_remove {
        if let Some(rule_object) = rule.as_object_mut() {
            rule_object.remove(key);
        }
    }
}

pub(crate) fn has_effective_xray_rule_fields(rule: &serde_json::Value) -> bool {
    [
        "domain",
        "ip",
        "port",
        "sourcePort",
        "network",
        "source",
        "user",
        "inboundTag",
        "protocol",
        "attrs",
    ]
    .iter()
    .any(|key| has_effective_xray_rule_field(rule.get(*key)))
}

fn has_effective_xray_rule_field(value: Option<&serde_json::Value>) -> bool {
    match value {
        Some(serde_json::Value::Array(values)) => !values.is_empty(),
        Some(serde_json::Value::String(value)) => !value.is_empty(),
        Some(serde_json::Value::Null) | None => false,
        Some(_) => true,
    }
}

/// Build the xray-core JSON config for transports owned by xray-core.
pub(crate) fn build_xray_config(req: &ConnectRequest) -> serde_json::Value {
    let flow_value =
        if req.transport == "tcp" || req.transport == "xhttp" || req.transport.is_empty() {
            req.flow.clone().unwrap_or_default()
        } else {
            String::new()
        };

    // Build xray outbound settings based on protocol
    let outbound_settings = match req.protocol.as_str() {
        "vmess" => serde_json::json!({
            "vnext": [{
                "address": req.server_address,
                "port": req.server_port,
                "users": [{
                    "id": req.uuid.clone().unwrap_or_default(),
                    "security": "auto"
                }]
            }]
        }),
        "trojan" => serde_json::json!({
            "servers": [{
                "address": req.server_address,
                "port": req.server_port,
                "password": req.password.clone().unwrap_or_default()
            }]
        }),
        "shadowsocks" => serde_json::json!({
            "servers": [{
                "address": req.server_address,
                "port": req.server_port,
                "password": req.password.clone().unwrap_or_default(),
                "method": req.encryption.clone().unwrap_or("aes-256-gcm".into())
            }]
        }),
        _ => serde_json::json!({
            "vnext": [{
                "address": req.server_address,
                "port": req.server_port,
                "users": [{
                    "id": req.uuid.clone().unwrap_or_default(),
                    "encryption": "none",
                    "flow": flow_value
                }]
            }]
        }),
    };

    let mut stream_settings = match req.transport.as_str() {
        "xhttp" => serde_json::json!({
            "network": "xhttp",
            "security": req.security,
            "xhttpSettings": {
                "path": req.path.clone().unwrap_or("/xhttp".into())
            }
        }),
        "ws" => serde_json::json!({
            "network": "ws",
            "security": req.security,
            "wsSettings": {
                "path": req.path.clone().unwrap_or("/".into()),
                "host": xray_transport_host(req)
            }
        }),
        _ => serde_json::json!({
            "network": "tcp",
            "security": req.security
        }),
    };
    apply_xray_stream_security_settings(&mut stream_settings, req);

    // Build routing rules from Workshop rules
    let mut routing_rules = Vec::new();

    // Custom domain rules from Workshop
    let mut proxy_domains = Vec::new();
    let mut direct_domains = Vec::new();
    let mut block_domains = Vec::new();

    for rule in &req.routing_rules {
        if rule.rule_type == "domain" {
            let domain_val = if rule.value.starts_with("*.") {
                // Wildcard → xray "domain:" prefix
                serde_json::Value::String(format!("domain:{}", rule.value.trim_start_matches("*.")))
            } else {
                serde_json::Value::String(format!("domain:{}", rule.value))
            };
            match rule.action.as_str() {
                "proxy" => proxy_domains.push(domain_val),
                "direct" => direct_domains.push(domain_val),
                "block" => block_domains.push(domain_val),
                _ => {}
            }
        }
    }

    // Add custom routing rules
    if !proxy_domains.is_empty() {
        routing_rules.push(serde_json::json!({
            "type": "field",
            "domain": proxy_domains,
            "outboundTag": "proxy"
        }));
    }
    if !direct_domains.is_empty() {
        routing_rules.push(serde_json::json!({
            "type": "field",
            "domain": direct_domains,
            "outboundTag": "direct"
        }));
    }
    if !block_domains.is_empty() {
        routing_rules.push(serde_json::json!({
            "type": "field",
            "domain": block_domains,
            "outboundTag": "block"
        }));
    }

    // API routing rule — must be FIRST
    let mut final_rules = vec![serde_json::json!({
        "type": "field",
        "inboundTag": ["api"],
        "outboundTag": "api"
    })];
    // DNS port 53 rule — so TUN mode DNS queries get resolved by xray instead of going to "direct"
    final_rules.insert(
        1,
        serde_json::json!({
            "type": "field",
            "port": "53",
            "outboundTag": "dns-out"
        }),
    );
    final_rules.extend(routing_rules);

    let mut config = serde_json::json!({
        "log": { "loglevel": "warning" },
        "stats": {},
        "api": {
            "tag": "api",
            "services": ["StatsService"]
        },
        "policy": {
            "system": {
                "statsInboundUplink": true,
                "statsInboundDownlink": true,
                "statsOutboundUplink": true,
                "statsOutboundDownlink": true
            }
        },
        "dns": xray_dns_config(req),
        "inbounds": [
            {
                "tag": "socks-in",
                "port": req.socks_port,
                "listen": "127.0.0.1",
                "protocol": "socks",
                "settings": { "udp": true, "ip": "127.0.0.1" },
                "sniffing": {
                    "enabled": true,
                    "destOverride": ["http", "tls", "quic", "fakedns"],
                    "routeOnly": true
                }
            },
            {
                "tag": "http-in",
                "port": req.http_port,
                "listen": "127.0.0.1",
                "protocol": "http"
                },
                {
                    "tag": "api",
                    "port": req.api_port,
                    "listen": "127.0.0.1",
                    "protocol": "dokodemo-door",
                    "settings": { "address": "127.0.0.1" }
            }
        ],
        "outbounds": [
            {
                "tag": "proxy",
                "protocol": req.protocol,
                "settings": outbound_settings,
                "streamSettings": stream_settings
            },
            {
                "tag": "direct",
                "protocol": "freedom"
            },
            {
                "tag": "block",
                "protocol": "blackhole",
                "settings": { "response": { "type": "http" } }
            },
            {
                "tag": "dns-out",
                "protocol": "dns"
            },
            {
                "tag": "api",
                "protocol": "blackhole"
            }
        ],
        "routing": {
            "domainStrategy": "AsIs",
            "rules": final_rules
        }
    });
    apply_xray_routing_policy(&mut config, req, true);
    config
}
