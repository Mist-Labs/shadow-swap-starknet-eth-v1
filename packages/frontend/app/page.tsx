"use client"

import Header from "@/components/shared/header"
import Footer from "@/components/shared/footer"
import HeroSection from "@/components/landing/HeroSection"
import FeaturesSection from "@/components/landing/FeaturesSection"
import HowItWorksSection from "@/components/landing/HowItWorksSection"

export default function LandingPage() {
  return (
    <div className="min-h-screen bg-black">
      <Header />
      <HeroSection />
      <FeaturesSection />
      <HowItWorksSection />
      <Footer />
    </div>
  )
}
