import Link from "next/link";

export default function NotFound() {
  return (
    <div className="text-center py-16 text-gray-400">
      <p className="text-2xl font-semibold mb-2">404</p>
      <p className="mb-4">Not found.</p>
      <Link href="/" className="text-blue-500 hover:underline">
        ← Back to repositories
      </Link>
    </div>
  );
}
