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
import { 
  Loader, 
  Activity,
  Workflow,
  Eye,
  Radio,
  Shield,
  Store,
} from "lucide-react";
import { Logo } from "@/components/ui/Logo";

const navigationItems = [
  { id: "dashboard", label: "Dashboard", icon: Activity },
  { id: "workflows", label: "Workflows", icon: Workflow },
  { id: "resonance", label: "Resonance", icon: Eye },
  { id: "ambient", label: "Ambient", icon: Radio },
  { id: "security", label: "Security", icon: Shield },
  { id: "marketplace", label: "Marketplace", icon: Store },
];

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
    <div className="min-h-screen bg-background text-foreground">
      <Header onNavigate={setActiveSection} activeSection={activeSection} />
      
      {activeSection === "dashboard" ? (
        <main className="container mx-auto px-6 py-16 max-w-4xl">
          <div className="text-center space-y-12">
            {/* Hero Video Section */}
            <div className="space-y-6">
              <div className="relative mx-auto max-w-2xl">
                <div className="aspect-video bg-muted rounded-lg flex items-center justify-center group cursor-pointer hover:bg-muted/80 transition-colors">
                  <div className="flex items-center space-x-3 text-muted-foreground group-hover:text-foreground transition-colors">
                    <Logo className="h-8 w-8" />
                    <span className="text-lg">The #1 AI Platform to Connect Any Model to Any System...</span>
                  </div>
                  <div className="absolute right-4 bottom-4">
                    <div className="w-12 h-12 bg-primary rounded-full flex items-center justify-center">
                      <div className="w-0 h-0 border-l-[8px] border-l-background border-t-[6px] border-t-transparent border-b-[6px] border-b-transparent ml-1"></div>
                    </div>
                  </div>
                </div>
              </div>
            </div>

            {/* Main Heading */}
            <div className="space-y-8">
              <h1 className="text-6xl md:text-7xl font-light bg-gradient-to-r from-primary via-primary to-secondary bg-clip-text text-transparent">
                AI Mesh of the Day
              </h1>
              
              {/* Quick Access Menu */}
              <div className="grid grid-cols-2 md:grid-cols-3 gap-4 max-w-2xl mx-auto mt-12">
                {navigationItems.map((item) => (
                  <button
                    key={item.id}
                    onClick={() => setActiveSection(item.id)}
                    className="p-6 rounded-lg border border-border hover:border-primary/50 hover:bg-muted/50 transition-all group"
                  >
                    <item.icon className="h-8 w-8 mx-auto mb-3 text-muted-foreground group-hover:text-primary transition-colors" />
                    <p className="text-sm font-medium">{item.label}</p>
                  </button>
                ))}
              </div>
            </div>
          </div>
        </main>
      ) : (
        <main className="container mx-auto px-6 py-8">
          <div className="max-w-7xl mx-auto">
            {renderContent()}
          </div>
        </main>
      )}
    </div>
  );
};

export default Index;
