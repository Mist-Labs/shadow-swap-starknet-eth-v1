import type React from "react"
import type { Metadata } from "next"
// import { Geist_Mono as GeistMono } from "next/font/google"
import "./globals.css"
import Providers from "@/components/providers"
import { ThemeProvider } from "@/components/theme-provider"
import { Toaster } from "@/components/ui/sonner"

// const geistMono = GeistMono({ subsets: ["latin"] })

export const metadata: Metadata = {
  title: "Shadow Swap - Privacy-Enhanced Cross-Chain Bridge",
  description: "Bridge assets across chains privately and instantly with Shadow Swap on Starknet and Ethereum",
}

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" suppressHydrationWarning>
      {/* <body className={`${geistMono.className} bg-background text-foreground antialiased`}> */}
      <body className={`bg-background text-foreground antialiased`}>
        <ThemeProvider attribute="class" defaultTheme="dark" enableSystem={false}>
          <Providers>
            {children}
            <Toaster />
          </Providers>
        </ThemeProvider>
      </body>
    </html>
  )
}
