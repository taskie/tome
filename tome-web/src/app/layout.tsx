import type { Metadata } from "next";
import Link from "next/link";
import "./globals.css";

export const metadata: Metadata = {
  title: { default: "tome", template: "%s — tome" },
  description: "File change tracking",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <body className="bg-gray-50 text-gray-900 text-sm">
        <header className="border-b border-gray-200 bg-white px-4 py-2">
          <Link href="/" className="text-gray-700 hover:text-blue-600 font-semibold">
            tome
          </Link>
        </header>
        <main className="max-w-5xl mx-auto px-4 py-6">{children}</main>
      </body>
    </html>
  );
}
