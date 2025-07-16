import { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { useAuth } from "@/hooks/useAuth";
import { Navigation } from "@/components/Navigation";
import { Dashboard } from "@/components/Dashboard";
import { WorkflowBuilder } from "@/components/WorkflowBuilder";
import { ResonanceView, AmbientMonitor, TrustSecurity, Marketplace } from "@/components/PlaceholderSections";
import { Loader } from "lucide-react";

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
      <div className="min-h-screen bg-cyber-background flex items-center justify-center">
        <Loader className="h-8 w-8 animate-spin text-cyber-primary" />
      </div>
    );
  }

  if (!user) {
    return null;
  }

  const renderContent = () => {
    switch (activeSection) {
      case "dashboard":
        return <Dashboard />;
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
        return <Dashboard />;
    }
  };

  return (
    <div className="min-h-screen bg-background">
      <Navigation onNavigate={setActiveSection} activeSection={activeSection} />
      
      <main className="lg:ml-80 p-6">
        <div className="max-w-7xl mx-auto">
          {renderContent()}
        </div>
      </main>
    </div>
  );
};

export default Index;
