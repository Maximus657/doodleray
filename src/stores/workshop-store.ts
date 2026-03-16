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
      set({ presets, loading: false });
    } catch {
      set({ loading: false });
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
    if (s.appliedPresets.some((ap) => ap.presetId === id)) return {};
    const applied: AppliedPreset = {
      presetId: preset.id,
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
