import { useState } from "react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Activity,
  Workflow,
  Eye,
  Radio,
  Shield,
  Store,
  Menu,
  X,
  Zap
} from "lucide-react";

const navigationItems = [
  {
    id: "dashboard",
    label: "Unified Dashboard",
    icon: Activity,
    description: "API and signal flow overview",
    active: true
  },
  {
    id: "workflows",
    label: "Workflow Builder",
    icon: Workflow,
    description: "Multimodal drag-drop flows"
  },
  {
    id: "resonance",
    label: "Resonance View",
    icon: Eye,
    description: "Model interaction visualization"
  },
  {
    id: "ambient",
    label: "Ambient Monitor",
    icon: Radio,
    description: "Real-time sensor streams"
  },
  {
    id: "security",
    label: "Trust & Security",
    icon: Shield,
    description: "Audit logs & lineage"
  },
  {
    id: "marketplace",
    label: "Marketplace",
    icon: Store,
    description: "Community templates"
  }
];

interface NavigationProps {
  onNavigate: (section: string) => void;
  activeSection: string;
}

export function Navigation({ onNavigate, activeSection }: NavigationProps) {
  const [isOpen, setIsOpen] = useState(false);

  return (
    <>
      {/* Mobile menu button */}
      <div className="lg:hidden fixed top-4 left-4 z-50">
        <Button
          variant="outline"
          size="icon"
          onClick={() => setIsOpen(!isOpen)}
          className="bg-card/80 backdrop-blur-sm border-border/50"
        >
          {isOpen ? <X className="h-4 w-4" /> : <Menu className="h-4 w-4" />}
        </Button>
      </div>

      {/* Navigation sidebar */}
      <nav className={cn(
        "fixed left-0 top-0 h-full w-80 bg-card/95 backdrop-blur-sm border-r border-border/50 z-40 transition-transform duration-300",
        "lg:translate-x-0",
        isOpen ? "translate-x-0" : "-translate-x-full lg:translate-x-0"
      )}>
        <div className="p-6">
          {/* Logo */}
          <div className="flex items-center gap-3 mb-8">
            <div className="w-10 h-10 rounded-lg bg-gradient-primary flex items-center justify-center">
              <Zap className="h-6 w-6 text-primary-foreground animate-pulse-glow" />
            </div>
            <div>
              <h1 className="text-xl font-bold bg-gradient-primary bg-clip-text text-transparent">
                ModelBridge
              </h1>
              <p className="text-xs text-muted-foreground">Resonant AI Router</p>
            </div>
          </div>

          {/* Navigation items */}
          <div className="space-y-2">
            {navigationItems.map((item) => {
              const Icon = item.icon;
              const isActive = activeSection === item.id;
              
              return (
                <button
                  key={item.id}
                  onClick={() => {
                    onNavigate(item.id);
                    setIsOpen(false);
                  }}
                  className={cn(
                    "w-full flex items-start gap-3 p-4 rounded-lg transition-all duration-200 text-left group",
                    isActive 
                      ? "bg-gradient-primary text-primary-foreground shadow-primary" 
                      : "hover:bg-muted/50 hover:shadow-card"
                  )}
                >
                  <Icon className={cn(
                    "h-5 w-5 mt-0.5 transition-transform group-hover:scale-110",
                    isActive ? "animate-frequency" : ""
                  )} />
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="font-medium">{item.label}</span>
                      {isActive && (
                        <Badge variant="secondary" className="px-1.5 py-0.5 text-xs">
                          Active
                        </Badge>
                      )}
                    </div>
                    <p className={cn(
                      "text-sm mt-1",
                      isActive ? "text-primary-foreground/80" : "text-muted-foreground"
                    )}>
                      {item.description}
                    </p>
                  </div>
                </button>
              );
            })}
          </div>

          {/* Status indicator */}
          <div className="mt-8 p-4 rounded-lg bg-muted/30 border border-border/30">
            <div className="flex items-center gap-2 mb-2">
              <div className="w-2 h-2 rounded-full bg-success animate-pulse-glow"></div>
              <span className="text-sm font-medium">System Active</span>
            </div>
            <p className="text-xs text-muted-foreground">
              All routing channels operational
            </p>
          </div>
        </div>
      </nav>

      {/* Overlay for mobile */}
      {isOpen && (
        <div 
          className="fixed inset-0 bg-background/80 backdrop-blur-sm z-30 lg:hidden"
          onClick={() => setIsOpen(false)}
        />
      )}
    </>
  );
}