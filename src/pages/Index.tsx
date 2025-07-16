import { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { useAuth } from "@/hooks/useAuth";
import { Header } from "@/components/Header";
import { Dashboard } from "@/components/Dashboard";
import { WorkflowBuilder } from "@/components/WorkflowBuilder";
import {
  ResonanceView,
  AmbientMonitor,
  TrustSecurity,
  Marketplace,
} from "@/components/PlaceholderSections";
import { Loader } from "lucide-react";
import { OmniBar } from "@/components/OmniBar";

const Index = () => {
  const [activeSection, setActiveSection] = useState("dashboard");
  const { user, loading } = useAuth();
  const navigate = useNavigate();

  useEffect(() => {
    if (!loading && !user) {
      navigate("/auth");
    }
  }, [user, loading, navigate]);

  if (loading) {
    return (
      <div className="min-h-screen bg-background flex items-center justify-center">
        <Loader className="h-8 w-8 animate-spin text-primary" />
      </div>
    );
  }

  if (!user) {
    return null;
  }

  const renderContent = () => {
    switch (activeSection) {
      case "dashboard":
        return <Dashboard onNavigate={setActiveSection} />;
      case "workflows":
        return <WorkflowBuilder />;
      case "resonance":
        return <ResonanceView />;
      case "ambient":
        return <AmbientMonitor />;
      case "security":
        return <TrustSecurity />;
      case "marketplace":
        return <Marketplace />;
      default:
        return <Dashboard onNavigate={setActiveSection} />;
    }
  };

  return (
    <div className="min-h-screen bg-background text-foreground">
      <Header onNavigate={setActiveSection} activeSection={activeSection} />
      
      <main className="container mx-auto px-4 sm:px-6 lg:px-8 py-8">
        {activeSection === "dashboard" && <OmniBar onNavigate={setActiveSection} />}
        <div className="max-w-7xl mx-auto">
          {renderContent()}
        </div>
      </main>
    </div>
  );
};

export default Index;
