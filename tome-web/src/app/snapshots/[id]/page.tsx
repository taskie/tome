import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";
import { api } from "@/lib/api";

export const dynamic = "force-dynamic";

type Props = { params: Promise<{ id: string }> };

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { id } = await params;
  return { title: `Snapshot ${id.slice(0, 10)}` };
}

export default async function SnapshotPage({ params }: Props) {
  const { id } = await params;

  let entries;
  try {
    entries = await api.entries(id);
  } catch {
    notFound();
  }

  const sorted = [...entries].sort((a, b) => a.path.localeCompare(b.path));
  const presentCount = entries.filter((e) => e.status === 1).length;
  const deletedCount = entries.filter((e) => e.status === 0).length;
  const idShort = id.slice(0, 10);

  return (
    <>
      <nav className="text-xs text-gray-400 mb-4">
        <Link href="/" className="hover:underline">
          Repositories
        </Link>
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

      {sorted.length === 0 ? (
        <p className="text-gray-400">No entries in this snapshot.</p>
      ) : (
        <table className="w-full border-collapse bg-white shadow-sm rounded overflow-hidden">
          <thead>
            <tr className="bg-gray-100 text-left">
              <th className="px-3 py-2 border-b-2 border-gray-200">Path</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Status</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Modified</th>
            </tr>
          </thead>
          <tbody>
            {sorted.map((e) => {
              const isPresent = e.status === 1;
              return (
                <tr key={e.id} className="border-b border-gray-100 hover:bg-gray-50">
                  <td
                    className={`px-3 py-1.5 ${isPresent ? "" : "line-through text-gray-300"}`}
                  >
                    {e.path}
                  </td>
                  <td className="px-3 py-1.5">
                    {isPresent ? (
                      <span className="text-xs px-1.5 py-0.5 rounded bg-green-50 text-green-700">
                        present
                      </span>
                    ) : (
                      <span className="text-xs px-1.5 py-0.5 rounded bg-red-50 text-red-600">
                        deleted
                      </span>
                    )}
                  </td>
                  <td className="px-3 py-1.5 text-gray-400 text-xs">
                    {e.mtime ? new Date(e.mtime).toLocaleString() : ""}
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
