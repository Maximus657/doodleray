// Workshop API client
// Uses Tauri invoke to bypass SSL — all HTTP goes through Rust reqwest

import { invoke } from '@tauri-apps/api/core';

const API_BASE = 'https://doodleraydb-doodleray-ic3y6k-c7350f-94-241-172-101.traefik.me/api';

// Device fingerprint (persisted in localStorage)
function getFingerprint(): string {
  let fp = localStorage.getItem('doodleray_fp');
  if (!fp) {
    fp = crypto.randomUUID();
    localStorage.setItem('doodleray_fp', fp);
  }
  return fp;
}

// Helper: call API through Tauri Rust backend (bypasses SSL issues)
async function apiGet(path: string): Promise<any> {
  const text = await invoke<string>('workshop_api', {
    url: `${API_BASE}${path}`,
    method: 'GET',
    body: null,
  });
  return JSON.parse(text);
}

async function apiPost(path: string, data: any): Promise<any> {
  const text = await invoke<string>('workshop_api', {
    url: `${API_BASE}${path}`,
    method: 'POST',
    body: JSON.stringify(data),
  });
  return JSON.parse(text);
}

let cachedNickname: string | null = null;

export async function registerDevice(): Promise<string> {
  if (cachedNickname) return cachedNickname;
  try {
    const data = await apiPost('/register', { fingerprint: getFingerprint() });
    cachedNickname = data.nickname;
    return data.nickname;
  } catch (e) {
    console.error('Register failed:', e);
    return 'doodleguy_?';
  }
}

export interface APIPreset {
  id: string;
  title: string;
  description: string;
  author: string;
  rules: Array<{
    type: 'domain' | 'exe';
    value: string;
    action: 'proxy' | 'direct' | 'block';
    enabled: boolean;
    comment?: string;
  }>;
  upvotes: number;
  stars: number;
  totalRatings: number;
  hasUpvoted: boolean;
  myRating?: number;
  createdAt: string;
}

export interface APIComment {
  id: number;
  nickname: string;
  text: string;
  stars: number;
  createdAt: string;
}

// GET presets
export async function fetchPresets(sort: string = 'popular'): Promise<APIPreset[]> {
  try {
    return await apiGet(`/presets?sort=${sort}&fp=${getFingerprint()}`);
  } catch (e) {
    console.error('Fetch presets failed:', e);
    return [];
  }
}

// POST preset
export async function publishPreset(title: string, description: string, rules: any[]): Promise<APIPreset | null> {
  try {
    return await apiPost('/presets', { title, description, rules, fingerprint: getFingerprint() });
  } catch (e) {
    console.error('Publish failed:', e);
    return null;
  }
}

// POST upvote
export async function toggleUpvote(presetId: string): Promise<{ upvotes: number; hasUpvoted: boolean } | null> {
  try {
    return await apiPost(`/presets/${presetId}/upvote`, { fingerprint: getFingerprint() });
  } catch (e) {
    console.error('Upvote failed:', e);
    return null;
  }
}

// POST rate
export async function ratePreset(presetId: string, rating: number): Promise<{ stars: number; totalRatings: number; myRating: number } | null> {
  try {
    return await apiPost(`/presets/${presetId}/rate`, { fingerprint: getFingerprint(), rating });
  } catch (e) {
    console.error('Rate failed:', e);
    return null;
  }
}

// GET comments
export async function fetchComments(presetId: string): Promise<APIComment[]> {
  try {
    return await apiGet(`/presets/${presetId}/comments`);
  } catch (e) {
    console.error('Fetch comments failed:', e);
    return [];
  }
}

// POST comment
export async function postComment(presetId: string, text: string, stars: number): Promise<APIComment | null> {
  try {
    return await apiPost(`/presets/${presetId}/comments`, { fingerprint: getFingerprint(), text, stars });
  } catch (e) {
    console.error('Post comment failed:', e);
    return null;
  }
}

// ══════════ Analytics ══════════

async function getAppVersion(): Promise<string> {
  try {
    const { getVersion } = await import('@tauri-apps/api/app');
    return await getVersion();
  } catch {
    return 'unknown';
  }
}

function getOS(): string {
  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes('windows')) return 'windows';
  if (ua.includes('mac')) return 'macos';
  if (ua.includes('linux')) return 'linux';
  return 'unknown';
}

// Report app launch (called once on startup)
export async function reportLaunch(): Promise<void> {
  try {
    const version = await getAppVersion();
    await apiPost('/analytics/launch', {
      device_id: getFingerprint(),
      app_version: version,
      os: getOS(),
    });
  } catch {
    // silent — analytics should never break the app
  }
}

// Start heartbeat (called once on startup, runs every 60s)
let heartbeatInterval: ReturnType<typeof setInterval> | null = null;

export function startHeartbeat(): void {
  if (heartbeatInterval) return; // already running
  
  const sendHeartbeat = async () => {
    try {
      const version = await getAppVersion();
      await apiPost('/analytics/heartbeat', {
        device_id: getFingerprint(),
        app_version: version,
        os: getOS(),
      });
    } catch {
      // silent
    }
  };
  
  // Send immediately, then every 60s
  sendHeartbeat();
  heartbeatInterval = setInterval(sendHeartbeat, 60_000);
}
