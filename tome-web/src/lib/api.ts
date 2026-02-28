import type { Blob, DiffResponse, Entry, FilesResponse, Repository, Snapshot, SnapshotEntry } from "./types";

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

  blob: (digest: string): Promise<Blob> => get(`/blobs/${encodeURIComponent(digest)}`),

  blobEntries: (digest: string): Promise<SnapshotEntry[]> => get(`/blobs/${encodeURIComponent(digest)}/entries`),

  history: (name: string, path: string): Promise<SnapshotEntry[]> =>
    get(`/repositories/${encodeURIComponent(name)}/history?path=${encodeURIComponent(path)}`),

  diff: (name: string, s1: string, s2: string, prefix = ""): Promise<DiffResponse> =>
    get(
      `/repositories/${encodeURIComponent(name)}/diff` +
        `?snapshot1=${encodeURIComponent(s1)}&snapshot2=${encodeURIComponent(s2)}&prefix=${encodeURIComponent(prefix)}`,
    ),

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
};
