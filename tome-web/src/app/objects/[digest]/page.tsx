import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";
import { api } from "@/lib/api";

export const dynamic = "force-dynamic";

type Props = { params: Promise<{ digest: string }> };

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { digest } = await params;
  return { title: `Object ${digest.slice(0, 12)}` };
}

export default async function ObjectPage({ params }: Props) {
  const { digest } = await params;

  let obj;
  let entries;
  let repositories;
  try {
    [obj, entries, repositories] = await Promise.all([
      api.object(digest),
      api.objectEntries(digest),
      api.repositories(),
    ]);
  } catch {
    notFound();
  }

  const repoById = new Map(repositories.map((r) => [r.id, r.name]));

  return (
    <>
      <nav className="text-xs text-gray-400 mb-4">
        <Link href="/" className="hover:underline">
          Repositories
        </Link>
        {" / object / "}
        <code className="text-gray-600">{digest.slice(0, 12)}</code>
      </nav>

      <h1 className="text-base font-semibold mb-4">
        <code className="text-blue-700 font-mono text-sm">{digest.slice(0, 20)}…</code>
      </h1>

      {/* Object metadata */}
      <table className="text-xs mb-6 border-collapse bg-white shadow-sm rounded overflow-hidden w-auto">
        <tbody>
          <tr className="border-b border-gray-100">
            <td className="px-3 py-1.5 text-gray-500 w-28">Digest</td>
            <td className="px-3 py-1.5 font-mono break-all">{digest}</td>
          </tr>
          <tr className="border-b border-gray-100">
            <td className="px-3 py-1.5 text-gray-500">Size</td>
            <td className="px-3 py-1.5">{obj.size.toLocaleString()} bytes</td>
          </tr>
          <tr>
            <td className="px-3 py-1.5 text-gray-500">Fast digest</td>
            <td className="px-3 py-1.5 font-mono">{obj.fast_digest}</td>
          </tr>
        </tbody>
      </table>

      <h2 className="text-sm font-semibold mb-2">
        Entries <span className="text-gray-400 font-normal">({entries.length})</span>
      </h2>

      {entries.length === 0 ? (
        <p className="text-gray-400 text-sm">No entries found.</p>
      ) : (
        <table className="w-full border-collapse bg-white shadow-sm rounded overflow-hidden text-xs">
          <thead>
            <tr className="bg-gray-100 text-left">
              <th className="px-3 py-2 border-b-2 border-gray-200">Path</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Snapshot</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Time</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Modified</th>
            </tr>
          </thead>
          <tbody>
            {entries.map(({ snapshot: s, entry: e }) => {
              const repoName = repoById.get(s.repository_id);
              const historyHref = repoName
                ? `/repositories/${encodeURIComponent(repoName)}/history?path=${encodeURIComponent(e.path)}`
                : null;
              return (
                <tr key={e.id} className="border-b border-gray-100 hover:bg-gray-50">
                  <td className="px-3 py-1.5 text-gray-700">
                    {historyHref ? (
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
                    <Link
                      href={`/snapshots/${s.id}${repoName ? `?repo=${encodeURIComponent(repoName)}` : ""}`}
                      className="font-mono text-blue-600 hover:underline"
                    >
                      {s.id.slice(0, 10)}
                    </Link>
                  </td>
                  <td className="px-3 py-1.5 text-gray-400">{new Date(s.created_at).toLocaleString()}</td>
                  <td className="px-3 py-1.5 text-gray-400">{e.mtime ? new Date(e.mtime).toLocaleString() : ""}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </>
  );
}
