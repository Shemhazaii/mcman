import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "mcman — MC Server Mod Manager",
  description: "Minecraft server mod manager",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="id">
      <body>{children}</body>
    </html>
  );
}
