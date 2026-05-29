import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Capsulet Dashboard",
  description: "Kubernetes-native automation and job execution dashboard"
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
