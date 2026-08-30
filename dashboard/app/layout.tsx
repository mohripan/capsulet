import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Capsulet",
  description: "Correctness-first AI-agent workflow platform"
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
// capsulet-claims: CAP-PRODUCT-001, CAP-DASHBOARD-001
