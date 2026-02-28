import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";
import { api } from "@/lib/api";

export const dynamic = "force-dynamic";

type Props = {
  params: Promise<{ name: string }>;
  searchParams: Promise<{ [key: string]: string | string[] | undefined }>;
};

export async function generateMetadata({ params, searchParams }: Props): Promise<Metadata> {
  const { name } = await params;
  const sp = await searchParams;
  const path = Array.isArray(sp.path) ? sp.path[0] : sp.path;
  return { title: `History — ${path ?? decodeURIComponent(name)}` };
}

export default async function HistoryPage({ params, searchParams }: Props) {
  const { name } = await params;
  const sp = await searchParams;
  const repoName = decodeURIComponent(name);
  const path = Array.isArray(sp.path) ? sp.path[0] : sp.path;

  if (!path) notFound();

  let history;
  try {
    history = await api.history(repoName, path);
  } catch {
    notFound();
  }

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
        {" / history"}
      </nav>

      <h1 className="text-base font-semibold mb-1">History</h1>
      <p className="text-xs text-gray-500 font-mono mb-4">{path}</p>

      {history.length === 0 ? (
        <p className="text-gray-400 text-sm">No history found for this path.</p>
      ) : (
        <table className="w-full border-collapse bg-white shadow-sm rounded overflow-hidden text-xs">
          <thead>
            <tr className="bg-gray-100 text-left">
              <th className="px-3 py-2 border-b-2 border-gray-200">Snapshot</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Time</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Status</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Modified</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Digest</th>
            </tr>
          </thead>
          <tbody>
            {history.map(({ snapshot: s, entry: e }) => {
              const isPresent = e.status === 1;
              return (
                <tr key={e.id} className="border-b border-gray-100 hover:bg-gray-50">
                  <td className="px-3 py-1.5">
                    <Link
                      href={`/snapshots/${s.id}?repo=${encodeURIComponent(repoName)}`}
                      className="font-mono text-blue-600 hover:underline"
                    >
                      {s.id.slice(0, 10)}
                    </Link>
                  </td>
                  <td className="px-3 py-1.5 text-gray-400">
                    {new Date(s.created_at).toLocaleString()}
                  </td>
                  <td className="px-3 py-1.5">
                    {isPresent ? (
                      <span className="px-1.5 py-0.5 rounded bg-green-50 text-green-700">present</span>
                    ) : (
                      <span className="px-1.5 py-0.5 rounded bg-red-50 text-red-600">deleted</span>
                    )}
                  </td>
                  <td className="px-3 py-1.5 text-gray-400">
                    {e.mtime ? new Date(e.mtime).toLocaleString() : ""}
                  </td>
                  <td className="px-3 py-1.5 font-mono text-gray-400">
                    {e.digest ? (
                      <Link href={`/blobs/${e.digest}`} className="hover:underline text-blue-500">
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
