import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";
import { api } from "@/lib/api";
import type { DiffResponse, Snapshot } from "@/lib/types";

export const dynamic = "force-dynamic";

type Props = {
  params: Promise<{ name: string }>;
  searchParams: Promise<{ [key: string]: string | string[] | undefined }>;
};

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { name } = await params;
  return { title: `Diff — ${decodeURIComponent(name)}` };
}

function parentPrefix(prefix: string): string {
  const trimmed = prefix.endsWith("/") ? prefix.slice(0, -1) : prefix;
  const idx = trimmed.lastIndexOf("/");
  return idx < 0 ? "" : trimmed.slice(0, idx + 1);
}

function fmtLabel(s: Snapshot): string {
  return `${s.id.slice(0, 10)}  ${new Date(s.created_at).toLocaleString()}`;
}

type DiffRow = {
  blobId: string;
  digest: string;
  size: number;
  paths1: string[];
  paths2: string[];
};

function buildRows(data: DiffResponse): DiffRow[] {
  const rows: DiffRow[] = Object.entries(data.diff).map(([blobId, [eids1, eids2]]) => ({
    blobId,
    digest: data.blobs[blobId]?.digest ?? "",
    size: data.blobs[blobId]?.size ?? 0,
    paths1: eids1.map((id) => data.entries[id]?.path ?? ""),
    paths2: eids2.map((id) => data.entries[id]?.path ?? ""),
  }));

  rows.sort((a, b) => {
    // deleted (only in s1) → added (only in s2) → same/moved
    const aDeleted = a.paths2.length === 0;
    const bDeleted = b.paths2.length === 0;
    const aAdded = a.paths1.length === 0;
    const bAdded = b.paths1.length === 0;
    if (aDeleted !== bDeleted) return aDeleted ? -1 : 1;
    if (aAdded !== bAdded) return aAdded ? -1 : 1;
    return (a.paths1[0] ?? a.paths2[0] ?? "").localeCompare(b.paths1[0] ?? b.paths2[0] ?? "");
  });

  return rows;
}

export default async function DiffPage({ params, searchParams }: Props) {
  const { name } = await params;
  const sp = await searchParams;
  const repoName = decodeURIComponent(name);

  const s1 = Array.isArray(sp.s1) ? sp.s1[0] : sp.s1;
  const s2 = Array.isArray(sp.s2) ? sp.s2[0] : sp.s2;
  const prefix = (Array.isArray(sp.prefix) ? sp.prefix[0] : sp.prefix) ?? "";

  let snapshots;
  try {
    snapshots = await api.snapshots(repoName);
  } catch {
    notFound();
  }

  const sorted = [...snapshots].sort((a, b) => b.created_at.localeCompare(a.created_at));

  let diffData: DiffResponse | null = null;
  let diffError: string | null = null;
  if (s1 && s2) {
    try {
      diffData = await api.diff(repoName, s1, s2, prefix);
    } catch (e) {
      diffError = e instanceof Error ? e.message : String(e);
    }
  }

  const rows = diffData ? buildRows(diffData) : [];
  const parent = parentPrefix(prefix);
  const baseUrl = `/repositories/${name}/diff`;
  const withSnaps = `${baseUrl}?s1=${s1 ?? ""}&s2=${s2 ?? ""}`;

  return (
    <>
      <nav className="text-xs text-gray-400 mb-4">
        <Link href="/" className="hover:underline">
          Repositories
        </Link>
        {" / "}
        <Link href={`/repositories/${name}`} className="hover:underline text-gray-600">
          {repoName}
        </Link>
        {" / diff"}
      </nav>

      <h1 className="text-base font-semibold mb-4">
        Diff — <span className="text-blue-700">{repoName}</span>
      </h1>

      {/* Snapshot selector */}
      <form method="GET" action={baseUrl} className="flex flex-wrap gap-3 items-end mb-6">
        <div className="flex flex-col gap-1">
          <label className="text-xs text-gray-500">Snapshot 1 (before)</label>
          <select
            name="s1"
            defaultValue={s1 ?? ""}
            className="text-xs border border-gray-300 rounded px-2 py-1 font-mono"
          >
            <option value="">— select —</option>
            {sorted.map((s) => (
              <option key={s.id} value={s.id}>
                {fmtLabel(s)}
              </option>
            ))}
          </select>
        </div>
        <div className="flex flex-col gap-1">
          <label className="text-xs text-gray-500">Snapshot 2 (after)</label>
          <select
            name="s2"
            defaultValue={s2 ?? ""}
            className="text-xs border border-gray-300 rounded px-2 py-1 font-mono"
          >
            <option value="">— select —</option>
            {sorted.map((s) => (
              <option key={s.id} value={s.id}>
                {fmtLabel(s)}
              </option>
            ))}
          </select>
        </div>
        <input type="hidden" name="prefix" value={prefix} />
        <button
          type="submit"
          className="text-xs px-3 py-1.5 rounded bg-blue-600 text-white hover:bg-blue-700"
        >
          Compare
        </button>
      </form>

      {diffError && (
        <p className="text-sm text-red-600 mb-4">{diffError}</p>
      )}

      {/* Diff result */}
      {diffData && (
        <>
          {/* Path navigation */}
          <div className="flex items-center gap-2 mb-3 text-xs">
            <span className="text-gray-500">Prefix:</span>
            <code className="text-gray-700 bg-gray-100 px-1.5 py-0.5 rounded">
              /{prefix}
            </code>
            {prefix && (
              <Link
                href={`${withSnaps}&prefix=${encodeURIComponent(parent)}`}
                className="px-2 py-0.5 rounded border border-gray-300 hover:bg-gray-100"
              >
                ↑ Up
              </Link>
            )}
          </div>

          {/* Summary */}
          <p className="text-xs text-gray-400 mb-3">
            <span className="text-green-700">+{rows.filter((r) => r.paths1.length === 0).length}</span>
            {" added, "}
            <span className="text-red-600">−{rows.filter((r) => r.paths2.length === 0).length}</span>
            {" deleted, "}
            {rows.filter((r) => r.paths1.length > 0 && r.paths2.length > 0).length}
            {" unchanged/moved"}
          </p>

          {rows.length === 0 ? (
            <p className="text-gray-400 text-sm">No differences found.</p>
          ) : (
            <table className="w-full border-collapse bg-white shadow-sm rounded overflow-hidden text-xs">
              <thead>
                <tr className="bg-gray-100 text-left">
                  <th className="px-3 py-2 border-b-2 border-gray-200 w-6"></th>
                  <th className="px-3 py-2 border-b-2 border-gray-200">Snapshot 1</th>
                  <th className="px-3 py-2 border-b-2 border-gray-200">Snapshot 2</th>
                  <th className="px-3 py-2 border-b-2 border-gray-200 text-right">Size</th>
                  <th className="px-3 py-2 border-b-2 border-gray-200">Digest</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((row) => {
                  const isAdded = row.paths1.length === 0;
                  const isDeleted = row.paths2.length === 0;
                  const isSame =
                    !isAdded &&
                    !isDeleted &&
                    row.paths1.length === row.paths2.length &&
                    row.paths1.every((p, i) => p === row.paths2[i]);

                  const rowBg = isAdded ? "bg-green-50" : isDeleted ? "bg-red-50" : "";
                  const badge = isAdded ? (
                    <span className="px-1 py-0.5 rounded bg-green-100 text-green-700">+</span>
                  ) : isDeleted ? (
                    <span className="px-1 py-0.5 rounded bg-red-100 text-red-600">−</span>
                  ) : isSame ? (
                    <span className="px-1 py-0.5 rounded bg-gray-100 text-gray-400">=</span>
                  ) : (
                    <span className="px-1 py-0.5 rounded bg-yellow-100 text-yellow-700">~</span>
                  );

                  return (
                    <tr key={row.blobId || `no-blob-${row.paths1[0]}`} className={`border-b border-gray-100 hover:bg-gray-50 ${rowBg}`}>
                      <td className="px-3 py-1.5">{badge}</td>
                      <td className="px-3 py-1.5 text-gray-700">
                        {row.paths1.length > 0 ? (
                          row.paths1.map((p) => <div key={p}>{p}</div>)
                        ) : (
                          <span className="text-gray-300">—</span>
                        )}
                      </td>
                      <td className="px-3 py-1.5 text-gray-700">
                        {row.paths2.length > 0 ? (
                          row.paths2.map((p) => <div key={p}>{p}</div>)
                        ) : (
                          <span className="text-gray-300">—</span>
                        )}
                      </td>
                      <td className="px-3 py-1.5 text-right text-gray-400">
                        {row.size > 0 ? row.size.toLocaleString() : ""}
                      </td>
                      <td className="px-3 py-1.5 text-gray-400 font-mono">
                        {row.digest.slice(0, 12)}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </>
      )}
    </>
  );
}
