export type Repository = {
  id: string;
  name: string;
  description: string;
  created_at: string;
  updated_at: string;
};

export type SnapshotMetadata = {
  scan_root?: string;
  scanned?: number;
  added?: number;
  modified?: number;
  unchanged?: number;
  deleted?: number;
  errors?: number;
};

export type Snapshot = {
  id: string;
  repository_id: string;
  parent_id: string | null;
  message: string;
  metadata: SnapshotMetadata;
  created_at: string;
};

export type Entry = {
  id: string;
  snapshot_id: string;
  path: string;
  /** 0 = deleted, 1 = present */
  status: number;
  blob_id: string | null;
  digest?: string;
  mode: number | null;
  mtime: string | null;
  created_at: string;
};

export type Blob = {
  id: string;
  digest: string;
  size: number;
  fast_digest: string;
  created_at: string;
};

export type SnapshotEntry = {
  snapshot: Snapshot;
  entry: Entry;
};

export type CacheEntry = {
  path: string;
  /** 0 = deleted, 1 = present */
  status: number;
  size: number | null;
  mtime: string | null;
  digest: string | null;
  fast_digest: string | null;
  snapshot_id: string;
  entry_id: string;
};

export type FilesResponse = {
  total: number;
  page: number;
  per_page: number;
  items: CacheEntry[];
};

export type DiffResponse = {
  snapshot1: Snapshot;
  snapshot2: Snapshot;
  blobs: Record<string, Blob>;
  entries: Record<string, Entry>;
  /** blob_id → [entry_ids_in_snapshot1, entry_ids_in_snapshot2] */
  diff: Record<string, [string[], string[]]>;
};
