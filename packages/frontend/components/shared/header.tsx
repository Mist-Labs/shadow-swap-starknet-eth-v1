"use client"

import { useState } from "react"
import Link from "next/link"
import { usePathname } from "next/navigation"
import { Menu, X } from "lucide-react"
import { DynamicWidget } from "@dynamic-labs/sdk-react-core"

export default function Header() {
    const [mobileMenuOpen, setMobileMenuOpen] = useState(false)
    const pathname = usePathname()

    const navItems = [
        { name: "Bridge", href: "/bridge" },
        { name: "Activity", href: "/activity" },
        { name: "Stats", href: "/stats" },
        { name: "Docs", href: "/docs" },
    ]

    const isActive = (href: string) => pathname === href

    return (
        <header className="fixed left-0 right-0 top-0 z-50 h-16 border-b border-neutral-800 bg-neutral-900/80 backdrop-blur-xl">
            <div className="mx-auto flex h-full max-w-7xl items-center justify-between px-4 sm:px-6">
                <div className="flex items-center gap-8">
                    <Link href="/" className="flex items-center gap-2">
                        <div className="h-8 w-8 rotate-45 rounded bg-gradient-to-br from-orange-500 to-pink-500"></div>
                        <h1 className="text-lg font-bold tracking-wider text-orange-500">SHADOW SWAP</h1>
                    </Link>

                    {/* Desktop Navigation */}
                    <nav className="hidden items-center gap-6 md:flex">
                        {navItems.map((item) => (
                            <Link
                                key={item.name}
                                href={item.href}
                                className={`text-sm transition-colors ${isActive(item.href) ? "font-semibold text-orange-500" : "text-neutral-400 hover:text-white"
                                    }`}
                            >
                                {item.name.toUpperCase()}
                            </Link>
                        ))}
                    </nav>
                </div>

                <div className="flex items-center gap-3">
                    {/* Dynamic Widget handles Wallet Connection AND Network Switching */}
                    <DynamicWidget />

                    {/* Mobile Menu */}
                    <button
                        className="text-neutral-400 hover:text-white md:hidden"
                        onClick={() => setMobileMenuOpen(!mobileMenuOpen)}
                    >
                        {mobileMenuOpen ? <X className="h-6 w-6" /> : <Menu className="h-6 w-6" />}
                    </button>
                </div>
            </div>

            {/* Mobile Menu Dropdown */}
            {mobileMenuOpen && (
                <div className="absolute left-0 right-0 top-16 border-b border-neutral-800 bg-neutral-900 shadow-2xl md:hidden">
                    <nav className="flex flex-col gap-2 p-4">
                        {navItems.map((item) => (
                            <Link
                                key={item.name}
                                href={item.href}
                                onClick={() => setMobileMenuOpen(false)}
                                className={`rounded px-3 py-2 text-left transition-colors ${isActive(item.href)
                                        ? "bg-orange-500 text-white"
                                        : "text-neutral-400 hover:bg-neutral-800 hover:text-white"
                                    }`}
                            >
                                {item.name.toUpperCase()}
                            </Link>
                        ))}
                    </nav>
                </div>
            )}
        </header>
    )
}
