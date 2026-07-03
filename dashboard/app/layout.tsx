import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Capsulet Memory Studio",
  description: "Local-first governed graph memory platform for private AI agents"
};

export default function RootLayout({
  children
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
