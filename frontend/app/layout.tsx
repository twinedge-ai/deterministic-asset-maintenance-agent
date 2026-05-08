import type { ReactNode } from "react";
import "./globals.css";

export const metadata = {
  title: "AssetPilot | Deterministic self-improving maintenance agent for industrial assets",
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
