type JsonObject = Record<string, unknown>;

export type RawXrayServerIdentity = {
  protocol: string;
  address: string;
  port: number;
};

function object(value: unknown): JsonObject | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as JsonObject
    : null;
}

function outboundEndpoint(outbound: JsonObject): { address?: string; port?: number } {
  const settings = object(outbound.settings);
  if (!settings) return {};
  const entries = Array.isArray(settings.vnext)
    ? settings.vnext
    : Array.isArray(settings.servers)
      ? settings.servers
      : [];
  const endpoint = object(entries[0]);
  return {
    address: typeof endpoint?.address === 'string' ? endpoint.address : undefined,
    port: typeof endpoint?.port === 'number' ? endpoint.port : undefined,
  };
}

/** Keep the selected proxy transport, but never trust imported DNS, log paths, or routing policy. */
export function sanitizeRawXrayConfig(rawConfig: unknown, server: RawXrayServerIdentity): JsonObject | null {
  let config: JsonObject;
  try {
    config = JSON.parse(JSON.stringify(rawConfig)) as JsonObject;
  } catch {
    return null;
  }

  const outbounds = Array.isArray(config.outbounds)
    ? config.outbounds.map(object).filter((value): value is JsonObject => value !== null)
    : [];
  const protocol = server.protocol.toLowerCase();
  const selected = outbounds.find((outbound) => {
    if (String(outbound.protocol ?? '').toLowerCase() !== protocol) return false;
    const endpoint = outboundEndpoint(outbound);
    return endpoint.address === server.address && endpoint.port === server.port;
  }) ?? outbounds.find((outbound) => String(outbound.protocol ?? '').toLowerCase() === protocol);

  if (!selected) return null;

  const selectedTag = 'doodleray-selected-proxy';
  outbounds.forEach((outbound, index) => {
    if (outbound === selected) outbound.tag = selectedTag;
    else if (outbound.tag === selectedTag) outbound.tag = `doodleray-unused-${index}`;
  });

  config.outbounds = outbounds;
  config.routing = {
    domainStrategy: 'AsIs',
    rules: [{ type: 'field', network: 'tcp,udp', outboundTag: selectedTag }],
  };
  delete config.dns;
  delete config.log;
  delete config.api;
  delete config.metrics;
  delete config.reverse;
  delete config.stats;
  delete config.observatory;
  delete config.burstObservatory;
  return config;
}
