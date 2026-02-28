import type { Metadata } from "next";
import Link from "next/link";
import { api } from "@/lib/api";
import type { RepoDiffResponse } from "@/lib/types";

export const dynamic = "force-dynamic";

export const metadata: Metadata = { title: "Diff" };

type Props = {
  searchParams: Promise<{ [key: string]: string | string[] | undefined }>;
};

function sp1(sp: { [key: string]: string | string[] | undefined }, key: string): string {
  const v = sp[key];
  return (Array.isArray(v) ? v[0] : v) ?? "";
}

type DiffRow = {
  blobId: string;
  digest: string;
  size: number;
  paths1: string[];
  paths2: string[];
  isDeleted: boolean;
};

function buildRows(data: RepoDiffResponse): DiffRow[] {
  const rows: DiffRow[] = Object.entries(data.diff).map(([blobId, [keys1, keys2]]) => ({
    blobId,
    digest: data.blobs[blobId]?.digest ?? "",
    size: data.blobs[blobId]?.size ?? 0,
    paths1: keys1.map((k) => data.entries[k]?.path ?? ""),
    paths2: keys2.map((k) => data.entries[k]?.path ?? ""),
    isDeleted: false,
  }));

  for (const key of data.deleted ?? []) {
    const entry = data.entries[key];
    if (!entry) continue;
    const isRepo1 = key.startsWith("1:");
    rows.push({
      blobId: "",
      digest: "",
      size: 0,
      paths1: isRepo1 ? [entry.path] : [],
      paths2: isRepo1 ? [] : [entry.path],
      isDeleted: true,
    });
  }

  rows.sort((a, b) => {
    const aOnly1 = a.paths2.length === 0;
    const bOnly1 = b.paths2.length === 0;
    const aOnly2 = a.paths1.length === 0;
    const bOnly2 = b.paths1.length === 0;
    if (aOnly1 !== bOnly1) return aOnly1 ? -1 : 1;
    if (aOnly2 !== bOnly2) return aOnly2 ? -1 : 1;
    return (a.paths1[0] ?? a.paths2[0] ?? "").localeCompare(b.paths1[0] ?? b.paths2[0] ?? "");
  });

  return rows;
}

export default async function RepoDiffPage({ searchParams }: Props) {
  const sp = await searchParams;
  const repo1 = sp1(sp, "repo1");
  const prefix1 = sp1(sp, "prefix1");
  const repo2 = sp1(sp, "repo2");
  const prefix2 = sp1(sp, "prefix2");

  const repositories = await api.repositories();

  let diffData: RepoDiffResponse | null = null;
  let diffError: string | null = null;
  if (repo1 && repo2) {
    try {
      diffData = await api.repoDiff(repo1, prefix1, repo2, prefix2);
    } catch (e) {
      diffError = e instanceof Error ? e.message : String(e);
    }
  }

  const rows = diffData ? buildRows(diffData) : [];

  return (
    <>
      <nav className="text-xs text-gray-400 mb-4">
        <Link href="/" className="hover:underline">
          Repositories
        </Link>
        {" / diff"}
      </nav>

      <h1 className="text-base font-semibold mb-4">Cross-repository Diff</h1>

      {/* Selector form */}
      <form method="GET" action="/diff" className="flex flex-wrap gap-6 items-end mb-6">
        <div className="flex flex-col gap-2">
          <span className="text-xs font-medium text-gray-600">Side 1</span>
          <div className="flex flex-col gap-1">
            <label className="text-xs text-gray-500">Repository</label>
            <select
              name="repo1"
              defaultValue={repo1}
              className="text-xs border border-gray-300 rounded px-2 py-1 font-mono"
            >
              <option value="">— select —</option>
              {repositories.map((r) => (
                <option key={r.id} value={r.name}>
                  {r.name}
                </option>
              ))}
            </select>
          </div>
          <div className="flex flex-col gap-1">
            <label className="text-xs text-gray-500">Path prefix</label>
            <input
              name="prefix1"
              defaultValue={prefix1}
              placeholder="(all files)"
              className="text-xs border border-gray-300 rounded px-2 py-1 w-48"
            />
          </div>
        </div>

        <div className="flex flex-col gap-2">
          <span className="text-xs font-medium text-gray-600">Side 2</span>
          <div className="flex flex-col gap-1">
            <label className="text-xs text-gray-500">Repository</label>
            <select
              name="repo2"
              defaultValue={repo2}
              className="text-xs border border-gray-300 rounded px-2 py-1 font-mono"
            >
              <option value="">— select —</option>
              {repositories.map((r) => (
                <option key={r.id} value={r.name}>
                  {r.name}
                </option>
              ))}
            </select>
          </div>
          <div className="flex flex-col gap-1">
            <label className="text-xs text-gray-500">Path prefix</label>
            <input
              name="prefix2"
              defaultValue={prefix2}
              placeholder="(all files)"
              className="text-xs border border-gray-300 rounded px-2 py-1 w-48"
            />
          </div>
        </div>

        <button type="submit" className="text-xs px-3 py-1.5 rounded bg-blue-600 text-white hover:bg-blue-700">
          Compare
        </button>
      </form>

      {diffError && <p className="text-sm text-red-600 mb-4">{diffError}</p>}

      {diffData && (
        <>
          <p className="text-xs text-gray-400 mb-3">
            <span className="text-red-600">−{rows.filter((r) => !r.isDeleted && r.paths2.length === 0).length}</span>
            {" only in "}
            <span className="font-medium text-gray-600">{diffData.repo1.name}</span>
            {", "}
            <span className="text-green-700">+{rows.filter((r) => !r.isDeleted && r.paths1.length === 0).length}</span>
            {" only in "}
            <span className="font-medium text-gray-600">{diffData.repo2.name}</span>
            {", "}
            {rows.filter((r) => !r.isDeleted && r.paths1.length > 0 && r.paths2.length > 0).length}
            {" in both"}
            {rows.some((r) => r.isDeleted) && (
              <>
                {", "}
                <span className="text-gray-500">✗{rows.filter((r) => r.isDeleted).length}</span>
                {" deleted"}
              </>
            )}
          </p>

          {rows.length === 0 ? (
            <p className="text-gray-400 text-sm">No differences found.</p>
          ) : (
            <table className="w-full border-collapse bg-white shadow-sm rounded overflow-hidden text-xs">
              <thead>
                <tr className="bg-gray-100 text-left">
                  <th className="px-3 py-2 border-b-2 border-gray-200 w-6"></th>
                  <th className="px-3 py-2 border-b-2 border-gray-200">{diffData.repo1.name}</th>
                  <th className="px-3 py-2 border-b-2 border-gray-200">{diffData.repo2.name}</th>
                  <th className="px-3 py-2 border-b-2 border-gray-200 text-right">Size</th>
                  <th className="px-3 py-2 border-b-2 border-gray-200">Digest</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((row) => {
                  const isOnly1 = row.paths2.length === 0;
                  const isOnly2 = row.paths1.length === 0;
                  const isSame =
                    !row.isDeleted &&
                    !isOnly1 &&
                    !isOnly2 &&
                    row.paths1.length === row.paths2.length &&
                    row.paths1.every((p, i) => p === row.paths2[i]);

                  const rowBg = row.isDeleted ? "bg-gray-50" : isOnly2 ? "bg-green-50" : isOnly1 ? "bg-red-50" : "";
                  const badge = row.isDeleted ? (
                    <span className="px-1 py-0.5 rounded bg-gray-200 text-gray-500">✗</span>
                  ) : isOnly2 ? (
                    <span className="px-1 py-0.5 rounded bg-green-100 text-green-700">+</span>
                  ) : isOnly1 ? (
                    <span className="px-1 py-0.5 rounded bg-red-100 text-red-600">−</span>
                  ) : isSame ? (
                    <span className="px-1 py-0.5 rounded bg-gray-100 text-gray-400">=</span>
                  ) : (
                    <span className="px-1 py-0.5 rounded bg-yellow-100 text-yellow-700">~</span>
                  );

                  const pathClass = row.isDeleted ? "line-through text-gray-400" : "hover:underline text-blue-600";

                  return (
                    <tr
                      key={row.blobId || `deleted-${row.paths1[0] ?? row.paths2[0]}`}
                      className={`border-b border-gray-100 hover:bg-gray-50 ${rowBg}`}
                    >
                      <td className="px-3 py-1.5">{badge}</td>
                      <td className="px-3 py-1.5 text-gray-700">
                        {row.paths1.length > 0 ? (
                          row.paths1.map((p) => (
                            <div key={p}>
                              {row.isDeleted ? (
                                <span className={pathClass}>{p}</span>
                              ) : (
                                <Link
                                  href={`/repositories/${encodeURIComponent(repo1)}/history?path=${encodeURIComponent(p)}`}
                                  className={pathClass}
                                >
                                  {p}
                                </Link>
                              )}
                            </div>
                          ))
                        ) : (
                          <span className="text-gray-300">—</span>
                        )}
                      </td>
                      <td className="px-3 py-1.5 text-gray-700">
                        {row.paths2.length > 0 ? (
                          row.paths2.map((p) => (
                            <div key={p}>
                              {row.isDeleted ? (
                                <span className={pathClass}>{p}</span>
                              ) : (
                                <Link
                                  href={`/repositories/${encodeURIComponent(repo2)}/history?path=${encodeURIComponent(p)}`}
                                  className={pathClass}
                                >
                                  {p}
                                </Link>
                              )}
                            </div>
                          ))
                        ) : (
                          <span className="text-gray-300">—</span>
                        )}
                      </td>
                      <td className="px-3 py-1.5 text-right text-gray-400">
                        {row.size > 0 ? row.size.toLocaleString() : ""}
                      </td>
                      <td className="px-3 py-1.5 text-gray-400 font-mono">
                        {row.digest ? (
                          <Link href={`/blobs/${row.digest}`} className="hover:underline text-blue-500">
                            {row.digest.slice(0, 12)}
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
      )}
    </>
  );
}
