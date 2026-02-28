import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";
import { api } from "@/lib/api";
import type { SnapshotMetadata } from "@/lib/types";

export const dynamic = "force-dynamic";

type Props = { params: Promise<{ name: string }> };

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { name } = await params;
  return { title: decodeURIComponent(name) };
}

function fmtChanges(m: SnapshotMetadata): string {
  const parts: string[] = [];
  if (m.added) parts.push(`+${m.added}`);
  if (m.modified) parts.push(`~${m.modified}`);
  if (m.deleted) parts.push(`-${m.deleted}`);
  return parts.length > 0 ? parts.join(" ") : "—";
}

export default async function RepositoryPage({ params }: Props) {
  const { name } = await params;
  const repoName = decodeURIComponent(name);

  let snapshots;
  try {
    snapshots = await api.snapshots(repoName);
  } catch {
    notFound();
  }

  // 新しい順に表示
  const sorted = [...snapshots].sort((a, b) => b.created_at.localeCompare(a.created_at));

  return (
    <>
      <nav className="text-xs text-gray-400 mb-4">
        <Link href="/" className="hover:underline">
          Repositories
        </Link>
        {" / "}
        <span className="text-gray-600">{repoName}</span>
      </nav>

      <h1 className="text-base font-semibold mb-2">
        Repository: <span className="text-blue-700">{repoName}</span>
      </h1>
      <div className="mb-4">
        <Link
          href={`/repositories/${name}/diff`}
          className="text-xs px-2 py-1 rounded border border-gray-300 hover:bg-gray-100"
        >
          Diff
        </Link>
      </div>

      {sorted.length === 0 ? (
        <p className="text-gray-400">No snapshots yet.</p>
      ) : (
        <table className="w-full border-collapse bg-white shadow-sm rounded overflow-hidden">
          <thead>
            <tr className="bg-gray-100 text-left">
              <th className="px-3 py-2 border-b-2 border-gray-200">Snapshot</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Time</th>
              <th className="px-3 py-2 border-b-2 border-gray-200 text-right">Files</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Changes</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Root</th>
            </tr>
          </thead>
          <tbody>
            {sorted.map((s) => {
              const idShort = s.id.slice(0, 10);
              const hasErrors = (s.metadata.errors ?? 0) > 0;
              return (
                <tr key={s.id} className="border-b border-gray-100 hover:bg-gray-50">
                  <td className="px-3 py-2">
                    <Link
                      href={`/snapshots/${s.id}?repo=${encodeURIComponent(repoName)}`}
                      className="text-blue-600 hover:underline font-mono"
                    >
                      {idShort}
                    </Link>
                  </td>
                  <td className="px-3 py-2 text-gray-400">
                    {new Date(s.created_at).toLocaleString()}
                  </td>
                  <td className="px-3 py-2 text-right text-gray-700">
                    {s.metadata.scanned ?? "—"}
                  </td>
                  <td className="px-3 py-2 text-gray-500">
                    {fmtChanges(s.metadata)}
                    {hasErrors && (
                      <span className="ml-2 text-xs px-1.5 py-0.5 rounded bg-red-50 text-red-600">
                        ×{s.metadata.errors}
                      </span>
                    )}
                  </td>
                  <td className="px-3 py-2 text-gray-400 text-xs truncate max-w-48">
                    {s.metadata.scan_root ?? ""}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </>
  );
}
