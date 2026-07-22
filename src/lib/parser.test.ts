import { parseMultipleLinks, parseShadowsocksLink, parseTrojanLink, parseVmessLink } from './parser.ts';

const utf8 = new TextEncoder().encode(JSON.stringify({ ps: '日本', add: 'vpn.example', port: 443, id: 'id' }));
const vmess = parseVmessLink(`vmess://${btoa(String.fromCharCode(...utf8))}`);
if (vmess?.name !== '日本') throw new Error(`VMess UTF-8 name was corrupted: ${vmess?.name}`);

const trojan = parseTrojanLink('trojan://p%40ss@vpn.example:443');
if (trojan?.password !== 'p@ss') throw new Error('Trojan password was not decoded');

const shadowsocks = parseShadowsocksLink(`ss://${btoa('aes-256-gcm:p:a')}@vpn.example:443`);
if (shadowsocks?.password !== 'p:a') throw new Error('Shadowsocks password was truncated');

const many = Array.from({ length: 600 }, (_, index) => `vless://id@vpn${index}.example:443`).join('\n');
if (parseMultipleLinks(many).length !== 512) throw new Error('Subscription server cap was not enforced');
