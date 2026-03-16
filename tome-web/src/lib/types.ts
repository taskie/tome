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
  root_object_id: string | null;
};

export type Entry = {
  id: string;
  snapshot_id: string;
  path: string;
  /** 0 = deleted, 1 = present */
  status: number;
  object_id: string | null;
  digest?: string;
  mode: number | null;
  mtime: string | null;
  created_at: string;
};

export type TomeObject = {
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
  objects: Record<string, TomeObject>;
  entries: Record<string, Entry>;
  /** object_id → [entry_ids_in_snapshot1, entry_ids_in_snapshot2] */
  diff: Record<string, [string[], string[]]>;
};

export type RepoDiffResponse = {
  repo1: Repository;
  repo2: Repository;
  objects: Record<string, TomeObject>;
  /** "1:{path}" or "2:{path}" → CacheEntry */
  entries: Record<string, CacheEntry>;
  /** object_id → [entry_keys_in_repo1, entry_keys_in_repo2] */
  diff: Record<string, [string[], string[]]>;
  /** Entry keys for deleted paths (status=0, object_id=null) */
  deleted: string[];
};

export type Store = {
  id: string;
  name: string;
  url: string;
  created_at: string;
  updated_at: string;
};

export type Machine = {
  machine_id: number;
  name: string;
  description: string;
  last_seen_at: string | null;
  created_at: string;
};

export type Tag = {
  id: string;
  object_id: string;
  key: string;
  value: string | null;
  created_at: string;
};

export type SyncPeer = {
  id: string;
  name: string;
  url: string;
  repository_id: string;
  last_synced_at: string | null;
  last_snapshot_id: string | null;
  created_at: string;
  updated_at: string;
};
