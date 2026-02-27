import type { Entry, Repository, Snapshot } from "./types";

const API_BASE = process.env.TOME_API_URL ?? "http://localhost:3000";

async function get<T>(path: string): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, { next: { revalidate: 10 } });
  if (!res.ok) {
    throw new Error(`API ${path} → ${res.status} ${res.statusText}`);
  }
  return res.json() as Promise<T>;
}

export const api = {
  repositories: (): Promise<Repository[]> => get("/repositories"),

  repository: (name: string): Promise<Repository> =>
    get(`/repositories/${encodeURIComponent(name)}`),

  snapshots: (name: string): Promise<Snapshot[]> =>
    get(`/repositories/${encodeURIComponent(name)}/snapshots`),

  entries: (id: string): Promise<Entry[]> =>
    get(`/snapshots/${encodeURIComponent(id)}/entries`),
};
