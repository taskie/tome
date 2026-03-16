import type { Metadata } from "next";
import Link from "next/link";
import { api } from "@/lib/api";

export const dynamic = "force-dynamic";

export const metadata: Metadata = { title: "Tags" };

export default async function TagsPage() {
  const tags = await api.tags();

  return (
    <>
      <h1 className="text-base font-semibold mb-4">Tags</h1>

      {tags.length === 0 ? (
        <p className="text-gray-400">
          No tags yet. Run <code className="bg-gray-100 px-1 rounded">tome tag</code> to create tags.
        </p>
      ) : (
        <table className="w-full border-collapse bg-white shadow-sm rounded overflow-hidden">
          <thead>
            <tr className="bg-gray-100 text-left">
              <th className="px-3 py-2 border-b-2 border-gray-200">Key</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Value</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Object</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Created</th>
            </tr>
          </thead>
          <tbody>
            {tags.map((t) => (
              <tr key={t.id} className="border-b border-gray-100 hover:bg-gray-50">
                <td className="px-3 py-2 font-mono text-blue-700">{t.key}</td>
                <td className="px-3 py-2 text-gray-500">{t.value ?? "—"}</td>
                <td className="px-3 py-2 font-mono text-xs">
                  <Link href={`/objects/${t.object_id}`} className="text-blue-600 hover:underline">
                    {t.object_id.slice(0, 10)}
                  </Link>
                </td>
                <td className="px-3 py-2 text-gray-400">{new Date(t.created_at).toLocaleString()}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </>
  );
}
