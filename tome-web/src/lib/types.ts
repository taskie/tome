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
  mode: number | null;
  mtime: string | null;
  created_at: string;
};
