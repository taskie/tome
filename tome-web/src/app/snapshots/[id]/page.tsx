import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";
import { api } from "@/lib/api";

export const dynamic = "force-dynamic";

type Props = {
  params: Promise<{ id: string }>;
  searchParams: Promise<{ [key: string]: string | string[] | undefined }>;
};

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { id } = await params;
  return { title: `Snapshot ${id.slice(0, 10)}` };
}

function parentPrefix(prefix: string): string {
  const trimmed = prefix.endsWith("/") ? prefix.slice(0, -1) : prefix;
  const idx = trimmed.lastIndexOf("/");
  return idx < 0 ? "" : trimmed.slice(0, idx + 1);
}

export default async function SnapshotPage({ params, searchParams }: Props) {
  const { id } = await params;
  const sp = await searchParams;
  const prefix = (Array.isArray(sp.prefix) ? sp.prefix[0] : sp.prefix) ?? "";
  const repo = Array.isArray(sp.repo) ? sp.repo[0] : sp.repo;

  let entries;
  try {
    entries = await api.entries(id, prefix);
  } catch {
    notFound();
  }

  const sorted = [...entries].sort((a, b) => a.path.localeCompare(b.path));
  const presentCount = entries.filter((e) => e.status === 1).length;
  const deletedCount = entries.filter((e) => e.status === 0).length;
  const idShort = id.slice(0, 10);
  const parent = parentPrefix(prefix);
  const baseUrl = `/snapshots/${id}`;
  const repoParam = repo ? `&repo=${encodeURIComponent(repo)}` : "";

  return (
    <>
      <nav className="text-xs text-gray-400 mb-4">
        <Link href="/" className="hover:underline">
          Repositories
        </Link>
        {repo && (
          <>
            {" / "}
            <Link href={`/repositories/${encodeURIComponent(repo)}`} className="hover:underline text-gray-600">
              {repo}
            </Link>
          </>
        )}
        {" / snapshot / "}
        <code className="text-gray-600">{idShort}</code>
      </nav>

      <h1 className="text-base font-semibold mb-1">
        Snapshot <code className="text-blue-700">{idShort}</code>
      </h1>
      <p className="text-xs text-gray-400 mb-4">
        {presentCount} present
        {deletedCount > 0 && <>, {deletedCount} deleted</>}
      </p>

      {/* Filter form */}
      <form method="GET" action={baseUrl} className="flex gap-2 items-center mb-4">
        <input
          name="prefix"
          defaultValue={prefix}
          placeholder="Path prefix…"
          className="text-xs border border-gray-300 rounded px-2 py-1 w-56"
        />
        {repo && <input type="hidden" name="repo" value={repo} />}
        <button type="submit" className="text-xs px-3 py-1 rounded bg-gray-200 hover:bg-gray-300">
          Filter
        </button>
        {prefix && (
          <Link
            href={`${baseUrl}?prefix=${encodeURIComponent(parent)}${repoParam}`}
            className="text-xs px-2 py-1 rounded border border-gray-300 hover:bg-gray-100"
          >
            ↑ Up
          </Link>
        )}
        {prefix && (
          <Link
            href={`${baseUrl}?${repo ? `repo=${encodeURIComponent(repo)}` : ""}`}
            className="text-xs text-gray-400 hover:underline"
          >
            Clear
          </Link>
        )}
      </form>

      {sorted.length === 0 ? (
        <p className="text-gray-400">No entries.</p>
      ) : (
        <table className="w-full border-collapse bg-white shadow-sm rounded overflow-hidden text-xs">
          <thead>
            <tr className="bg-gray-100 text-left">
              <th className="px-3 py-2 border-b-2 border-gray-200">Path</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Status</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Modified</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Digest</th>
            </tr>
          </thead>
          <tbody>
            {sorted.map((e) => {
              const isPresent = e.status === 1;
              const historyHref = repo
                ? `/repositories/${encodeURIComponent(repo)}/history?path=${encodeURIComponent(e.path)}`
                : null;
              return (
                <tr key={e.id} className="border-b border-gray-100 hover:bg-gray-50">
                  <td className={`px-3 py-1.5 ${isPresent ? "" : "line-through text-gray-300"}`}>
                    {historyHref && isPresent ? (
                      <Link href={historyHref} className="hover:underline text-blue-600">
                        {e.mode === 16384 ? "📁 " : ""}
                        {e.path}
                      </Link>
                    ) : (
                      <>
                        {e.mode === 16384 ? "📁 " : ""}
                        {e.path}
                      </>
                    )}
                  </td>
                  <td className="px-3 py-1.5">
                    {isPresent ? (
                      <span className="px-1.5 py-0.5 rounded bg-green-50 text-green-700">present</span>
                    ) : (
                      <span className="px-1.5 py-0.5 rounded bg-red-50 text-red-600">deleted</span>
                    )}
                  </td>
                  <td className="px-3 py-1.5 text-gray-400">{e.mtime ? new Date(e.mtime).toLocaleString() : ""}</td>
                  <td className="px-3 py-1.5 font-mono text-gray-400">
                    {e.digest ? (
                      <Link href={`/objects/${e.digest}`} className="hover:underline text-blue-500">
                        {e.digest.slice(0, 12)}
                      </Link>
                    ) : (
                      ""
                    )}
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
