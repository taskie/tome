import type { Metadata } from "next";
import { api } from "@/lib/api";

export const dynamic = "force-dynamic";

export const metadata: Metadata = { title: "Stores" };

export default async function StoresPage() {
  const stores = await api.stores();

  return (
    <>
      <h1 className="text-base font-semibold mb-4">Stores</h1>

      {stores.length === 0 ? (
        <p className="text-gray-400">
          No stores yet. Run <code className="bg-gray-100 px-1 rounded">tome store add</code> first.
        </p>
      ) : (
        <table className="w-full border-collapse bg-white shadow-sm rounded overflow-hidden">
          <thead>
            <tr className="bg-gray-100 text-left">
              <th className="px-3 py-2 border-b-2 border-gray-200">Name</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">URL</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Created</th>
            </tr>
          </thead>
          <tbody>
            {stores.map((s) => (
              <tr key={s.id} className="border-b border-gray-100 hover:bg-gray-50">
                <td className="px-3 py-2 font-mono text-blue-700">{s.name}</td>
                <td className="px-3 py-2 text-gray-500 text-xs truncate max-w-sm">{s.url}</td>
                <td className="px-3 py-2 text-gray-400">{new Date(s.created_at).toLocaleString()}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </>
  );
}
