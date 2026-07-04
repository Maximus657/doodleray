import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import * as api from '../lib/workshop-api';

// ========== Types — Routing Rules ==========

export interface RoutingRule {
  id: string;
  type: 'domain' | 'exe';
  value: string;
  action: 'proxy' | 'direct' | 'block';
  enabled: boolean;
  comment?: string;
}

export interface RoutingPreset {
  id: string;
  title: string;
  description: string;
  author: string;
  rules: RoutingRule[];
  stars: number;
  totalRatings: number;
  myRating?: number;
  upvotes: number;
  hasUpvoted: boolean;
  createdAt: string;
}

export interface PresetComment {
  id: string | number;
  presetId?: string;
  nickname: string;
  text: string;
  stars: number;
  date: string;
}

export interface AppliedPreset {
  presetId: string;
  title: string;
  description: string;
  author: string;
  rules: RoutingRule[];
  appliedAt: string;
}

const BUILTIN_PRESETS: RoutingPreset[] = [
  {
    id: 'builtin-gaming-min-ping',
    title: '🎮 Геймерский — минимальный пинг',
    description: 'Игры, лаунчеры, PUBG и античит идут напрямую — всё остальное через VPN как обычно.',
    author: 'DoodleRay',
    rules: [
      { id: 'builtin-gaming-steam-domain', type: 'domain', value: 'steampowered.com', action: 'direct', enabled: true, comment: 'Steam' },
      { id: 'builtin-gaming-steam-community', type: 'domain', value: 'steamcommunity.com', action: 'direct', enabled: true, comment: 'Steam Community' },
      { id: 'builtin-gaming-steam-cdn', type: 'domain', value: 'steamcdn-a.akamaihd.net', action: 'direct', enabled: true, comment: 'Steam CDN' },
      { id: 'builtin-gaming-epic', type: 'domain', value: 'epicgames.com', action: 'direct', enabled: true, comment: 'Epic Games' },
      { id: 'builtin-gaming-unreal', type: 'domain', value: 'unrealengine.com', action: 'direct', enabled: true, comment: 'Unreal Engine' },
      { id: 'builtin-gaming-riot', type: 'domain', value: 'riotgames.com', action: 'direct', enabled: true, comment: 'Riot Games' },
      { id: 'builtin-gaming-lol', type: 'domain', value: 'leagueoflegends.com', action: 'direct', enabled: true, comment: 'League of Legends' },
      { id: 'builtin-gaming-blizzard', type: 'domain', value: 'blizzard.com', action: 'direct', enabled: true, comment: 'Blizzard' },
      { id: 'builtin-gaming-battlenet', type: 'domain', value: 'battle.net', action: 'direct', enabled: true, comment: 'Battle.net' },
      { id: 'builtin-gaming-ea', type: 'domain', value: 'ea.com', action: 'direct', enabled: true, comment: 'EA Games' },
      { id: 'builtin-gaming-ubisoft', type: 'domain', value: 'ubisoft.com', action: 'direct', enabled: true, comment: 'Ubisoft' },
      { id: 'builtin-gaming-steam-exe', type: 'exe', value: 'steam.exe', action: 'direct', enabled: true, comment: 'Steam клиент' },
      { id: 'builtin-gaming-cs2', type: 'exe', value: 'cs2.exe', action: 'direct', enabled: true, comment: 'Counter-Strike 2' },
      { id: 'builtin-gaming-valorant', type: 'exe', value: 'valorant.exe', action: 'direct', enabled: true, comment: 'Valorant' },
      { id: 'builtin-gaming-dota2', type: 'exe', value: 'dota2.exe', action: 'direct', enabled: true, comment: 'Dota 2' },
      { id: 'builtin-gaming-genshin', type: 'exe', value: 'GenshinImpact.exe', action: 'direct', enabled: true, comment: 'Genshin Impact' },
      { id: 'builtin-gaming-valorant-shipping', type: 'exe', value: 'VALORANT-Win64-Shipping.exe', action: 'direct', enabled: true, comment: 'Valorant' },
      { id: 'builtin-gaming-fortnite-shipping', type: 'exe', value: 'FortniteClient-Win64-Shipping.exe', action: 'direct', enabled: true, comment: 'Fortnite' },
      { id: 'builtin-gaming-pubg-main', type: 'exe', value: 'TslGame.exe', action: 'direct', enabled: true, comment: 'PUBG' },
      { id: 'builtin-gaming-pubg-zk', type: 'exe', value: 'TslGame_ZK.exe', action: 'direct', enabled: true, comment: 'PUBG protected process' },
      { id: 'builtin-gaming-pubg-be', type: 'exe', value: 'TslGame_BE.exe', action: 'direct', enabled: true, comment: 'PUBG BattlEye launcher' },
      { id: 'builtin-gaming-pubg-exec', type: 'exe', value: 'ExecPubg.exe', action: 'direct', enabled: true, comment: 'PUBG Steam launcher' },
      { id: 'builtin-gaming-battleye', type: 'exe', value: 'BEService.exe', action: 'direct', enabled: true, comment: 'BattlEye service' },
      // No explicit 'proxy' rules here: in whole-computer mode everything not
      // listed goes through the VPN anyway, so they only add noise.
    ],
    stars: 5,
    totalRatings: 1,
    myRating: undefined,
    upvotes: 999,
    hasUpvoted: false,
    createdAt: '2026-06-03T00:00:00.000Z',
  },
];

export function isGamingMinPingPreset(preset: { id?: string; presetId?: string; title: string }) {
  const id = preset.id ?? preset.presetId;
  const title = preset.title.toLowerCase();
  return id === 'builtin-gaming-direct' ||
    id === 'builtin-gaming-min-ping' ||
    (title.includes('геймер') && title.includes('пинг'));
}

function routingRuleKey(rule: Pick<RoutingRule, 'type' | 'value'>) {
  return `${rule.type}:${rule.value.trim().toLowerCase()}`;
}

function mergeRoutingRules(primary: RoutingRule[], additions: RoutingRule[]) {
  const seen = new Set(primary.map(routingRuleKey));
  const merged = [...primary];
  for (const rule of additions) {
    const key = routingRuleKey(rule);
    if (seen.has(key)) continue;
    seen.add(key);
    merged.push(rule);
  }
  return merged;
}

function mergeBuiltinPresets(apiPresets: RoutingPreset[]): RoutingPreset[] {
  let mergedIntoApiGamingPreset = false;
  const mergedApiPresets = apiPresets.map((preset) => {
    if (!isGamingMinPingPreset(preset)) return preset;
    mergedIntoApiGamingPreset = true;
    return {
      ...preset,
      description: 'Игры, лаунчеры, PUBG и античит идут напрямую — всё остальное через VPN как обычно.',
      rules: mergeRoutingRules(preset.rules, BUILTIN_PRESETS[0].rules),
    };
  });

  if (mergedIntoApiGamingPreset) {
    return mergedApiPresets;
  }

  const apiIds = new Set(apiPresets.map((preset) => preset.id));
  return [...BUILTIN_PRESETS.filter((preset) => !apiIds.has(preset.id)), ...apiPresets];
}

function appliedPresetMatchesPreset(applied: AppliedPreset, preset: RoutingPreset) {
  return applied.presetId === preset.id || (isGamingMinPingPreset(applied) && isGamingMinPingPreset(preset));
}

function normalizeAppliedPreset(applied: AppliedPreset): AppliedPreset {
  if (!isGamingMinPingPreset({ id: applied.presetId, title: applied.title })) {
    return applied;
  }

  return {
    ...applied,
    presetId: 'builtin-gaming-min-ping',
    title: '🎮 Геймерский — минимальный пинг',
    description: 'Игры, лаунчеры, PUBG и античит идут напрямую — всё остальное через VPN как обычно.',
    rules: mergeRoutingRules(applied.rules, BUILTIN_PRESETS[0].rules),
  };
}

function appliedPresetKey(applied: AppliedPreset) {
  return isGamingMinPingPreset(applied) ? 'builtin-gaming-min-ping' : applied.presetId;
}

function normalizeAppliedPresets(appliedPresets: AppliedPreset[]) {
  return appliedPresets.reduce<AppliedPreset[]>((merged, applied) => {
    const normalized = normalizeAppliedPreset(applied);
    const key = appliedPresetKey(normalized);
    const existingIndex = merged.findIndex((item) => appliedPresetKey(item) === key);

    if (existingIndex === -1) {
      merged.push(normalized);
      return merged;
    }

    const existing = merged[existingIndex];
    merged[existingIndex] = {
      ...existing,
      ...normalized,
      appliedAt: existing.appliedAt,
      rules: mergeRoutingRules(existing.rules, normalized.rules),
    };
    return merged;
  }, []);
}

interface WorkshopState {
  myRules: RoutingRule[];
  appliedPresets: AppliedPreset[];
  presets: RoutingPreset[];
  sortBy: 'popular' | 'newest' | 'top-rated';
  comments: Record<string, PresetComment[]>;
  loading: boolean;
  nickname: string;

  // My custom rules
  addRule: (rule: RoutingRule) => void;
  removeRule: (id: string) => void;
  toggleRule: (id: string) => void;
  setRuleAction: (id: string, action: RoutingRule['action']) => void;

  // Applied presets
  removeAppliedPreset: (presetId: string) => void;
  toggleAppliedRule: (presetId: string, ruleId: string) => void;
  setAppliedRuleAction: (presetId: string, ruleId: string, action: RoutingRule['action']) => void;
  removeAppliedRule: (presetId: string, ruleId: string) => void;

  // Presets (API-backed)
  setSortBy: (sort: 'popular' | 'newest' | 'top-rated') => void;
  loadPresets: () => Promise<void>;
  applyPreset: (id: string) => void;
  publishPreset: (preset: RoutingPreset) => void;
  ratePreset: (id: string, rating: number) => void;
  upvotePreset: (id: string) => void;

  // Comments (API-backed)
  loadComments: (presetId: string) => Promise<void>;
  addComment: (presetId: string, text: string, stars: number) => void;

  // Init
  init: () => Promise<void>;
  
  // All active rules (computed helper)
  getAllActiveRules: () => RoutingRule[];
}

export const useWorkshopStore = create<WorkshopState>()(persist((set, get) => ({
  myRules: [],
  appliedPresets: [],
  presets: [],
  sortBy: 'popular',
  comments: {},
  loading: false,
  nickname: '',

  // ── Init: register device + load presets ──
  init: async () => {
    set((s) => ({ appliedPresets: normalizeAppliedPresets(s.appliedPresets) }));
    const nickname = await api.registerDevice();
    set({ nickname });
    await get().loadPresets();
  },

  // ── My Rules (local) ──
  addRule: (rule) => set((s) => ({ myRules: [...s.myRules, rule] })),
  removeRule: (id) => set((s) => ({ myRules: s.myRules.filter((r) => r.id !== id) })),
  toggleRule: (id) => set((s) => ({ myRules: s.myRules.map((r) => r.id === id ? { ...r, enabled: !r.enabled } : r) })),
  setRuleAction: (id, action) => set((s) => ({ myRules: s.myRules.map((r) => r.id === id ? { ...r, action } : r) })),

  // ── Sort ──
  setSortBy: (sort) => {
    set({ sortBy: sort });
    get().loadPresets();
  },

  // ── Load presets from API ──
  loadPresets: async () => {
    set({ loading: true });
    try {
      const data = await api.fetchPresets(get().sortBy);
      const presets: RoutingPreset[] = data.map((p) => ({
        id: p.id,
        title: p.title,
        description: p.description,
        author: p.author,
        rules: p.rules.map((r, i) => ({ ...r, id: (r as any).id || `${p.id}_r${i}`, enabled: r.enabled ?? true })) as RoutingRule[],
        stars: p.stars,
        totalRatings: p.totalRatings,
        myRating: p.myRating,
        upvotes: p.upvotes,
        hasUpvoted: p.hasUpvoted,
        createdAt: p.createdAt,
      }));
      set({ presets: mergeBuiltinPresets(presets), loading: false });
    } catch {
      set({ presets: mergeBuiltinPresets([]), loading: false });
    }
  },

  // ── Applied preset management ──
  removeAppliedPreset: (presetId) => set((s) => ({
    appliedPresets: s.appliedPresets.filter((ap) => ap.presetId !== presetId),
  })),
  toggleAppliedRule: (presetId, ruleId) => set((s) => ({
    appliedPresets: s.appliedPresets.map((ap) =>
      ap.presetId === presetId
        ? { ...ap, rules: ap.rules.map((r) => r.id === ruleId ? { ...r, enabled: !r.enabled } : r) }
        : ap
    ),
  })),
  setAppliedRuleAction: (presetId, ruleId, action) => set((s) => ({
    appliedPresets: s.appliedPresets.map((ap) =>
      ap.presetId === presetId
        ? { ...ap, rules: ap.rules.map((r) => r.id === ruleId ? { ...r, action } : r) }
        : ap
    ),
  })),
  removeAppliedRule: (presetId, ruleId) => set((s) => ({
    appliedPresets: s.appliedPresets.map((ap) =>
      ap.presetId === presetId
        ? { ...ap, rules: ap.rules.filter((r) => r.id !== ruleId) }
        : ap
    ).filter((ap) => ap.rules.length > 0),
  })),

  // ── Apply preset (save as preset card, not flat rules) ──
  applyPreset: (id) => set((s) => {
    const preset = s.presets.find((p) => p.id === id);
    if (!preset) return {};
    // Don't add if already applied
    if (s.appliedPresets.some((ap) => appliedPresetMatchesPreset(ap, preset))) return {};
    const applied: AppliedPreset = {
      presetId: isGamingMinPingPreset(preset) ? 'builtin-gaming-min-ping' : preset.id,
      title: preset.title,
      description: preset.description,
      author: preset.author,
      rules: preset.rules.map((r) => ({ ...r, id: crypto.randomUUID() })),
      appliedAt: new Date().toISOString(),
    };
    return { appliedPresets: [...s.appliedPresets, applied] };
  }),

  // ── Publish preset to API ──
  publishPreset: async (preset) => {
    const result = await api.publishPreset(preset.title, preset.description, preset.rules);
    if (result) {
      // Reload from API
      get().loadPresets();
    }
  },

  // ── Rate preset via API ──
  ratePreset: async (id, rating) => {
    // Optimistic update
    set((s) => ({
      presets: s.presets.map((p) => {
        if (p.id !== id) return p;
        const wasRated = p.myRating !== undefined;
        const newTotal = wasRated ? p.totalRatings : p.totalRatings + 1;
        const oldSum = p.stars * p.totalRatings;
        const newSum = wasRated ? oldSum - (p.myRating || 0) + rating : oldSum + rating;
        return { ...p, stars: Math.round((newSum / newTotal) * 10) / 10, totalRatings: newTotal, myRating: rating };
      }),
    }));
    // Send to API
    const result = await api.ratePreset(id, rating);
    if (result) {
      set((s) => ({
        presets: s.presets.map((p) => p.id === id ? { ...p, stars: result.stars, totalRatings: result.totalRatings, myRating: result.myRating } : p),
      }));
    }
  },

  // ── Upvote via API ──
  upvotePreset: async (id) => {
    // Optimistic
    set((s) => ({
      presets: s.presets.map((p) => p.id === id ? { ...p, upvotes: p.hasUpvoted ? p.upvotes - 1 : p.upvotes + 1, hasUpvoted: !p.hasUpvoted } : p),
    }));
    const result = await api.toggleUpvote(id);
    if (result) {
      set((s) => ({
        presets: s.presets.map((p) => p.id === id ? { ...p, upvotes: result.upvotes, hasUpvoted: result.hasUpvoted } : p),
      }));
    }
  },

  // ── Load comments from API ──
  loadComments: async (presetId) => {
    const data = await api.fetchComments(presetId);
    const comments: PresetComment[] = data.map((c) => ({
      id: c.id,
      nickname: c.nickname,
      text: c.text,
      stars: c.stars,
      date: new Date(c.createdAt).toLocaleDateString('ru-RU'),
    }));
    set((s) => ({ comments: { ...s.comments, [presetId]: comments } }));
  },

  // ── Add comment via API ──
  addComment: async (presetId, text, stars) => {
    const result = await api.postComment(presetId, text, stars);
    if (result) {
      // Reload comments
      get().loadComments(presetId);
      // Reload presets to get updated ratings
      get().loadPresets();
    }
  },

  // ── Helper: get all active rules (custom + applied presets) ──
  getAllActiveRules: () => {
    const s = get();
    const presetRules = s.appliedPresets.flatMap((ap) => ap.rules.filter((r) => r.enabled));
    const customRules = s.myRules.filter((r) => r.enabled);
    return [...presetRules, ...customRules];
  },
}),
{
  name: 'workshop-storage',
  partialize: (state) => ({
    myRules: state.myRules,
    appliedPresets: state.appliedPresets,
  }),
}
));
