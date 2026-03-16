import type {
  DiffResponse,
  Entry,
  FilesResponse,
  Machine,
  RepoDiffResponse,
  Repository,
  Snapshot,
  SnapshotEntry,
  Store,
  SyncPeer,
  Tag,
  TomeObject,
} from "./types";

const API_BASE = process.env.TOME_API_URL ?? "http://localhost:8080";

async function get<T>(path: string): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, { next: { revalidate: 10 } });
  if (!res.ok) {
    throw new Error(`API ${path} → ${res.status} ${res.statusText}`);
  }
  return res.json() as Promise<T>;
}

export const api = {
  repositories: (): Promise<Repository[]> => get("/repositories"),

  repository: (name: string): Promise<Repository> => get(`/repositories/${encodeURIComponent(name)}`),

  snapshots: (name: string): Promise<Snapshot[]> => get(`/repositories/${encodeURIComponent(name)}/snapshots`),

  entries: (id: string, prefix = ""): Promise<Entry[]> =>
    get(`/snapshots/${encodeURIComponent(id)}/entries` + (prefix ? `?prefix=${encodeURIComponent(prefix)}` : "")),

  object: (digest: string): Promise<TomeObject> => get(`/objects/${encodeURIComponent(digest)}`),

  objectEntries: (digest: string): Promise<SnapshotEntry[]> => get(`/objects/${encodeURIComponent(digest)}/entries`),

  history: (name: string, path: string): Promise<SnapshotEntry[]> =>
    get(`/repositories/${encodeURIComponent(name)}/history?path=${encodeURIComponent(path)}`),

  diff: (name: string, s1: string, s2: string, prefix = ""): Promise<DiffResponse> =>
    get(
      `/repositories/${encodeURIComponent(name)}/diff` +
        `?snapshot1=${encodeURIComponent(s1)}&snapshot2=${encodeURIComponent(s2)}&prefix=${encodeURIComponent(prefix)}`,
    ),

  repoDiff: (repo1: string, prefix1: string, repo2: string, prefix2: string): Promise<RepoDiffResponse> => {
    const p = new URLSearchParams({ repo1, repo2 });
    if (prefix1) p.set("prefix1", prefix1);
    if (prefix2) p.set("prefix2", prefix2);
    return get(`/diff?${p.toString()}`);
  },

  files: (
    name: string,
    opts: { prefix?: string; includeDeleted?: boolean; page?: number; perPage?: number },
  ): Promise<FilesResponse> => {
    const p = new URLSearchParams();
    if (opts.prefix) p.set("prefix", opts.prefix);
    if (opts.includeDeleted) p.set("include_deleted", "true");
    if (opts.page && opts.page > 1) p.set("page", String(opts.page));
    if (opts.perPage) p.set("per_page", String(opts.perPage));
    const qs = p.toString();
    return get(`/repositories/${encodeURIComponent(name)}/files${qs ? `?${qs}` : ""}`);
  },

  stores: (): Promise<Store[]> => get("/stores"),

  machines: (): Promise<Machine[]> => get("/machines"),

  tags: (): Promise<Tag[]> => get("/tags"),

  syncPeers: (): Promise<SyncPeer[]> => get("/sync-peers"),
};
