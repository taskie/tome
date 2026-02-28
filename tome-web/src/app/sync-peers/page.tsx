import type { Metadata } from "next";
import { api } from "@/lib/api";

export const dynamic = "force-dynamic";

export const metadata: Metadata = { title: "Sync Peers" };

export default async function SyncPeersPage() {
  const peers = await api.syncPeers();

  return (
    <>
      <h1 className="text-base font-semibold mb-4">Sync Peers</h1>

      {peers.length === 0 ? (
        <p className="text-gray-400">
          No sync peers yet. Run <code className="bg-gray-100 px-1 rounded">tome sync add</code> to register a peer.
        </p>
      ) : (
        <table className="w-full border-collapse bg-white shadow-sm rounded overflow-hidden">
          <thead>
            <tr className="bg-gray-100 text-left">
              <th className="px-3 py-2 border-b-2 border-gray-200">Name</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">URL</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Repository</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Last Synced</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Created</th>
            </tr>
          </thead>
          <tbody>
            {peers.map((p) => (
              <tr key={p.id} className="border-b border-gray-100 hover:bg-gray-50">
                <td className="px-3 py-2 font-mono text-blue-700">{p.name}</td>
                <td className="px-3 py-2 text-gray-500 text-xs truncate max-w-sm">{p.url}</td>
                <td className="px-3 py-2 font-mono text-xs">{p.repository_id.slice(0, 10)}</td>
                <td className="px-3 py-2 text-gray-400">
                  {p.last_synced_at ? new Date(p.last_synced_at).toLocaleString() : "—"}
                </td>
                <td className="px-3 py-2 text-gray-400">{new Date(p.created_at).toLocaleString()}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </>
  );
}
