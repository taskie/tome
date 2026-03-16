import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";
import { api } from "@/lib/api";

export const dynamic = "force-dynamic";

const PER_PAGE = 100;

type Props = {
  params: Promise<{ name: string }>;
  searchParams: Promise<{ [key: string]: string | string[] | undefined }>;
};

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { name } = await params;
  return { title: `Files — ${decodeURIComponent(name)}` };
}

function sp1(sp: { [key: string]: string | string[] | undefined }, key: string): string | undefined {
  const v = sp[key];
  return Array.isArray(v) ? v[0] : v;
}

function parentPrefix(prefix: string): string {
  const trimmed = prefix.endsWith("/") ? prefix.slice(0, -1) : prefix;
  const idx = trimmed.lastIndexOf("/");
  return idx < 0 ? "" : trimmed.slice(0, idx + 1);
}

function buildUrl(base: string, params: Record<string, string | number | boolean | undefined>): string {
  const p = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined && v !== "" && v !== false && v !== 1 && v !== 0) {
      p.set(k, String(v));
    }
    // keep falsy defaults out of URL except include_deleted=true
    if (k === "include_deleted" && v === true) p.set(k, "true");
  }
  const qs = p.toString();
  return qs ? `${base}?${qs}` : base;
}

export default async function FilesPage({ params, searchParams }: Props) {
  const { name } = await params;
  const sp = await searchParams;
  const repoName = decodeURIComponent(name);

  const prefix = sp1(sp, "prefix") ?? "";
  const includeDeleted = sp1(sp, "include_deleted") === "true";
  const page = Math.max(1, Number(sp1(sp, "page") ?? "1"));

  let data;
  try {
    data = await api.files(repoName, {
      prefix,
      includeDeleted,
      page,
      perPage: PER_PAGE,
    });
  } catch {
    notFound();
  }

  const { total, items } = data;
  const totalPages = Math.max(1, Math.ceil(total / PER_PAGE));
  const parent = parentPrefix(prefix);
  const baseUrl = `/repositories/${name}/files`;

  function pageUrl(p: number) {
    return buildUrl(baseUrl, { prefix, include_deleted: includeDeleted || undefined, page: p > 1 ? p : undefined });
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
        {" / files"}
      </nav>

      <h1 className="text-base font-semibold mb-3">
        Repository: <span className="text-blue-700">{repoName}</span>
      </h1>
      <nav className="flex gap-1 mb-4 border-b border-gray-200 pb-0">
        <Link
          href={`/repositories/${name}`}
          className="text-xs px-3 py-1.5 text-gray-500 hover:text-gray-800 hover:border-b-2 hover:border-gray-300 -mb-px"
        >
          Snapshots
        </Link>
        <span className="text-xs px-3 py-1.5 border-b-2 border-blue-600 text-blue-700 font-medium -mb-px">Files</span>
        <Link
          href={`/repositories/${name}/diff`}
          className="text-xs px-3 py-1.5 text-gray-500 hover:text-gray-800 hover:border-b-2 hover:border-gray-300 -mb-px"
        >
          Diff
        </Link>
      </nav>

      {/* Filter form */}
      <form method="GET" action={baseUrl} className="flex flex-wrap gap-2 items-center mb-4">
        <input
          name="prefix"
          defaultValue={prefix}
          placeholder="Path prefix…"
          className="text-xs border border-gray-300 rounded px-2 py-1 w-56"
        />
        <label className="flex items-center gap-1 text-xs text-gray-600 cursor-pointer">
          <input type="checkbox" name="include_deleted" value="true" defaultChecked={includeDeleted} />
          Show deleted
        </label>
        <button type="submit" className="text-xs px-3 py-1 rounded bg-gray-200 hover:bg-gray-300">
          Filter
        </button>
        {prefix && (
          <Link
            href={buildUrl(baseUrl, { prefix: parent || undefined, include_deleted: includeDeleted || undefined })}
            className="text-xs px-2 py-1 rounded border border-gray-300 hover:bg-gray-100"
          >
            ↑ Up
          </Link>
        )}
        {(prefix || includeDeleted) && (
          <Link href={baseUrl} className="text-xs text-gray-400 hover:underline">
            Clear
          </Link>
        )}
      </form>

      {/* Summary + pagination info */}
      <p className="text-xs text-gray-400 mb-3">
        {total.toLocaleString()} {includeDeleted ? "entries" : "files"}
        {prefix && (
          <>
            {" "}
            matching <code className="font-mono">{prefix}…</code>
          </>
        )}
        {totalPages > 1 && (
          <>
            {" "}
            &mdash; page {page} / {totalPages}
          </>
        )}
      </p>

      {items.length === 0 ? (
        <p className="text-gray-400 text-sm">No files found.</p>
      ) : (
        <table className="w-full border-collapse bg-white shadow-sm rounded overflow-hidden text-xs">
          <thead>
            <tr className="bg-gray-100 text-left">
              <th className="px-3 py-2 border-b-2 border-gray-200">Path</th>
              {includeDeleted && <th className="px-3 py-2 border-b-2 border-gray-200">Status</th>}
              <th className="px-3 py-2 border-b-2 border-gray-200 text-right">Size</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Modified</th>
              <th className="px-3 py-2 border-b-2 border-gray-200">Digest</th>
            </tr>
          </thead>
          <tbody>
            {items.map((e) => {
              const isPresent = e.status === 1;
              const historyHref = `/repositories/${name}/history?path=${encodeURIComponent(e.path)}`;
              return (
                <tr
                  key={e.path}
                  className={`border-b border-gray-100 hover:bg-gray-50 ${!isPresent ? "opacity-50" : ""}`}
                >
                  <td className={`px-3 py-1.5 ${!isPresent ? "line-through text-gray-400" : ""}`}>
                    {isPresent ? (
                      <Link href={historyHref} className="hover:underline text-blue-600">
                        {e.path}
                      </Link>
                    ) : (
                      e.path
                    )}
                  </td>
                  {includeDeleted && (
                    <td className="px-3 py-1.5">
                      {isPresent ? (
                        <span className="px-1.5 py-0.5 rounded bg-green-50 text-green-700">present</span>
                      ) : (
                        <span className="px-1.5 py-0.5 rounded bg-red-50 text-red-600">deleted</span>
                      )}
                    </td>
                  )}
                  <td className="px-3 py-1.5 text-right text-gray-500">
                    {e.size != null ? e.size.toLocaleString() : ""}
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

      {/* Pagination controls */}
      {totalPages > 1 && (
        <div className="flex gap-2 items-center mt-4 text-xs">
          {page > 1 ? (
            <Link href={pageUrl(page - 1)} className="px-2 py-1 rounded border border-gray-300 hover:bg-gray-100">
              ← Prev
            </Link>
          ) : (
            <span className="px-2 py-1 rounded border border-gray-200 text-gray-300">← Prev</span>
          )}
          <span className="text-gray-500">
            {page} / {totalPages}
          </span>
          {page < totalPages ? (
            <Link href={pageUrl(page + 1)} className="px-2 py-1 rounded border border-gray-300 hover:bg-gray-100">
              Next →
            </Link>
          ) : (
            <span className="px-2 py-1 rounded border border-gray-200 text-gray-300">Next →</span>
          )}
        </div>
      )}
    </>
  );
}
