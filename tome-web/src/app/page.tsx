import type { Metadata } from "next";
import Link from "next/link";
import { api } from "@/lib/api";

export const dynamic = "force-dynamic";

export const metadata: Metadata = { title: "Repositories" };

export default async function RepositoriesPage() {
  const repos = await api.repositories();

  return (
    <>
      <h1 className="text-base font-semibold mb-4">Repositories</h1>

      {repos.length === 0 ? (
        <p className="text-gray-400">
          No repositories yet. Run <code className="bg-gray-100 px-1 rounded">tome scan</code> first.
        </p>
      ) : (
        <table className="w-full border-collapse bg-white shadow-sm rounded overflow-hidden">
          <thead>
            <tr className="bg-gray-100 text-left">
              <th className="px-3 py-2 border-b-2 border-gray-200">Name</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Description</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Created</th>
            </tr>
          </thead>
          <tbody>
            {repos.map((r) => (
              <tr key={r.id} className="border-b border-gray-100 hover:bg-gray-50">
                <td className="px-3 py-2">
                  <Link
                    href={`/repositories/${encodeURIComponent(r.name)}`}
                    className="text-blue-600 hover:underline"
                  >
                    {r.name}
                  </Link>
                </td>
                <td className="px-3 py-2 text-gray-500">{r.description}</td>
                <td className="px-3 py-2 text-gray-400">
                  {new Date(r.created_at).toLocaleString()}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </>
  );
}
