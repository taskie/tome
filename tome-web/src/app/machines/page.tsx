import type { Metadata } from "next";
import { api } from "@/lib/api";

export const dynamic = "force-dynamic";

export const metadata: Metadata = { title: "Machines" };

export default async function MachinesPage() {
  const machines = await api.machines();

  return (
    <>
      <h1 className="text-base font-semibold mb-4">Machines</h1>

      {machines.length === 0 ? (
        <p className="text-gray-400">
          No machines registered. Run <code className="bg-gray-100 px-1 rounded">tome init --server</code> to register.
        </p>
      ) : (
        <table className="w-full border-collapse bg-white shadow-sm rounded overflow-hidden">
          <thead>
            <tr className="bg-gray-100 text-left">
              <th className="px-3 py-2 border-b-2 border-gray-200">ID</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Name</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Description</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Last Seen</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Created</th>
            </tr>
          </thead>
          <tbody>
            {machines.map((m) => (
              <tr key={m.machine_id} className="border-b border-gray-100 hover:bg-gray-50">
                <td className="px-3 py-2 font-mono">{m.machine_id}</td>
                <td className="px-3 py-2 font-mono text-blue-700">{m.name}</td>
                <td className="px-3 py-2 text-gray-500">{m.description || "—"}</td>
                <td className="px-3 py-2 text-gray-400">
                  {m.last_seen_at ? new Date(m.last_seen_at).toLocaleString() : "—"}
                </td>
                <td className="px-3 py-2 text-gray-400">{new Date(m.created_at).toLocaleString()}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </>
  );
}
