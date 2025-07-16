import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import {
  Workflow,
  Plus,
  Play,
  Pause,
  Settings,
  Mic,
  Camera,
  Brain,
  Waves,
  Zap,
  ArrowRight,
  MoreVertical,
  Search
} from "lucide-react";

const workflowTemplates = [
  {
    id: "voice-to-text",
    name: "Voice → Text Analysis",
    description: "Convert speech to text and analyze sentiment",
    triggers: ["Voice Pattern", "Ambient Sound"],
    models: ["Whisper", "GPT-4"],
    status: "Active"
  },
  {
    id: "biometric-content",
    name: "Biometric → Content Generation", 
    description: "Generate content based on physiological signals",
    triggers: ["Heart Rate", "Stress Level"],
    models: ["Claude", "Midjourney"],
    status: "Draft"
  },
  {
    id: "ambient-orchestration",
    name: "Ambient Orchestration",
    description: "Multi-model response to environmental changes",
    triggers: ["Temperature", "Light Level", "Sound"],
    models: ["GPT-4", "DALL-E", "ElevenLabs"],
    status: "Active"
  }
];

const availableNodes = [
  { type: "input", icon: Mic, label: "Voice Input", color: "primary" },
  { type: "input", icon: Camera, label: "Visual Input", color: "secondary" },
  { type: "input", icon: Waves, label: "Biometric", color: "accent" },
  { type: "process", icon: Brain, label: "AI Model", color: "primary" },
  { type: "process", icon: Zap, label: "Transformer", color: "secondary" },
  { type: "output", icon: Settings, label: "Action", color: "accent" }
];

export function WorkflowBuilder() {
  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex flex-col lg:flex-row lg:items-center justify-between gap-4">
        <div>
          <h1 className="text-3xl font-bold">Multimodal Workflow Builder</h1>
          <p className="text-muted-foreground">
            Create drag-drop flows with conditional triggers and cymatic enhancement
          </p>
        </div>
        <div className="flex gap-2">
          <Button size="sm" className="bg-gradient-primary hover:opacity-90">
            <Plus className="h-4 w-4 mr-2" />
            New Workflow
          </Button>
          <Button variant="outline" size="sm">
            <Search className="h-4 w-4 mr-2" />
            Templates
          </Button>
        </div>
      </div>

      {/* Quick Actions */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <Card className="bg-gradient-primary/10 border-primary/20 shadow-primary/20">
          <CardContent className="p-4">
            <div className="flex items-center gap-3">
              <div className="w-10 h-10 rounded-lg bg-primary/20 flex items-center justify-center">
                <Mic className="h-5 w-5 text-primary" />
              </div>
              <div>
                <h3 className="font-semibold">Voice Flow</h3>
                <p className="text-sm text-muted-foreground">Start with voice input</p>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card className="bg-gradient-secondary/10 border-secondary/20 shadow-secondary/20">
          <CardContent className="p-4">
            <div className="flex items-center gap-3">
              <div className="w-10 h-10 rounded-lg bg-secondary/20 flex items-center justify-center">
                <Waves className="h-5 w-5 text-secondary" />
              </div>
              <div>
                <h3 className="font-semibold">Biometric Flow</h3>
                <p className="text-sm text-muted-foreground">Physiological triggers</p>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card className="bg-gradient-resonance/10 border-accent/20 shadow-glow/20">
          <CardContent className="p-4">
            <div className="flex items-center gap-3">
              <div className="w-10 h-10 rounded-lg bg-accent/20 flex items-center justify-center">
                <Brain className="h-5 w-5 text-accent" />
              </div>
              <div>
                <h3 className="font-semibold">AI Orchestra</h3>
                <p className="text-sm text-muted-foreground">Multi-model workflow</p>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-4 gap-6">
        {/* Node Palette */}
        <Card className="bg-gradient-surface border-border/50 shadow-card">
          <CardHeader>
            <CardTitle className="text-sm">Node Palette</CardTitle>
            <CardDescription>Drag components to build flows</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="space-y-3">
              {availableNodes.map((node, index) => {
                const Icon = node.icon;
                return (
                  <div 
                    key={index}
                    className={`p-3 rounded-lg border-2 border-dashed border-${node.color}/30 hover:border-${node.color}/60 cursor-grab hover:bg-${node.color}/5 transition-all group`}
                  >
                    <div className="flex items-center gap-2">
                      <Icon className={`h-4 w-4 text-${node.color} group-hover:animate-frequency`} />
                      <span className="text-sm font-medium">{node.label}</span>
                    </div>
                    <div className={`mt-1 text-xs text-muted-foreground`}>
                      {node.type}
                    </div>
                  </div>
                );
              })}
            </div>
          </CardContent>
        </Card>

        {/* Canvas Area */}
        <Card className="lg:col-span-2 bg-gradient-surface border-border/50 shadow-card">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Workflow className="h-5 w-5 text-primary" />
              Canvas
            </CardTitle>
            <CardDescription>
              Drag nodes here to create your workflow
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="min-h-96 bg-muted/20 rounded-lg border-2 border-dashed border-border/30 flex items-center justify-center relative overflow-hidden">
              {/* Grid pattern */}
              <div className="absolute inset-0 opacity-20">
                <div className="w-full h-full" style={{
                  backgroundImage: `radial-gradient(circle at 1px 1px, rgba(255,255,255,0.15) 1px, transparent 0)`,
                  backgroundSize: '20px 20px'
                }}></div>
              </div>
              
              {/* Demo workflow nodes */}
              <div className="relative z-10 flex items-center gap-8">
                <div className="flex flex-col items-center gap-2">
                  <div className="w-12 h-12 rounded-lg bg-primary/20 border-2 border-primary/40 flex items-center justify-center animate-pulse-glow">
                    <Mic className="h-6 w-6 text-primary" />
                  </div>
                  <span className="text-xs font-medium">Voice Input</span>
                </div>
                
                <ArrowRight className="h-6 w-6 text-muted-foreground animate-frequency" />
                
                <div className="flex flex-col items-center gap-2">
                  <div className="w-12 h-12 rounded-lg bg-secondary/20 border-2 border-secondary/40 flex items-center justify-center animate-wave">
                    <Brain className="h-6 w-6 text-secondary" />
                  </div>
                  <span className="text-xs font-medium">GPT-4</span>
                </div>
                
                <ArrowRight className="h-6 w-6 text-muted-foreground animate-frequency" />
                
                <div className="flex flex-col items-center gap-2">
                  <div className="w-12 h-12 rounded-lg bg-accent/20 border-2 border-accent/40 flex items-center justify-center animate-resonance">
                    <Zap className="h-6 w-6 text-accent" />
                  </div>
                  <span className="text-xs font-medium">Response</span>
                </div>
              </div>
              
              <div className="absolute bottom-4 right-4 text-xs text-muted-foreground">
                Drop nodes here to start building
              </div>
            </div>
          </CardContent>
        </Card>

        {/* Properties Panel */}
        <Card className="bg-gradient-surface border-border/50 shadow-card">
          <CardHeader>
            <CardTitle className="text-sm">Properties</CardTitle>
            <CardDescription>Configure selected node</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="space-y-4">
              <div>
                <label className="text-sm font-medium">Node Name</label>
                <Input placeholder="Voice Input" className="mt-1" />
              </div>
              
              <div>
                <label className="text-sm font-medium">Trigger Type</label>
                <div className="mt-2 space-y-2">
                  {["Voice Pattern", "Frequency Match", "Ambient Level"].map((trigger) => (
                    <div key={trigger} className="flex items-center gap-2">
                      <input type="radio" name="trigger" className="w-3 h-3" />
                      <span className="text-sm">{trigger}</span>
                    </div>
                  ))}
                </div>
              </div>
              
              <div>
                <label className="text-sm font-medium">Sensitivity</label>
                <div className="mt-2 space-y-2">
                  <div className="flex justify-between text-xs">
                    <span>Low</span>
                    <span>High</span>
                  </div>
                  <div className="w-full h-2 bg-muted rounded-full">
                    <div className="w-3/4 h-full bg-gradient-primary rounded-full"></div>
                  </div>
                </div>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Workflow Templates */}
      <Card className="bg-gradient-surface border-border/50 shadow-card">
        <CardHeader>
          <CardTitle>Workflow Templates</CardTitle>
          <CardDescription>
            Pre-built flows for common use cases
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            {workflowTemplates.map((template) => (
              <Card key={template.id} className="bg-muted/30 border-border/30">
                <CardHeader className="pb-3">
                  <div className="flex items-start justify-between">
                    <CardTitle className="text-base">{template.name}</CardTitle>
                    <Badge variant={template.status === "Active" ? "default" : "secondary"} className="text-xs">
                      {template.status}
                    </Badge>
                  </div>
                  <CardDescription className="text-sm">
                    {template.description}
                  </CardDescription>
                </CardHeader>
                <CardContent className="pt-0">
                  <div className="space-y-3">
                    <div>
                      <p className="text-xs font-medium mb-1">Triggers</p>
                      <div className="flex flex-wrap gap-1">
                        {template.triggers.map((trigger) => (
                          <Badge key={trigger} variant="outline" className="text-xs">
                            {trigger}
                          </Badge>
                        ))}
                      </div>
                    </div>
                    
                    <div>
                      <p className="text-xs font-medium mb-1">Models</p>
                      <div className="flex flex-wrap gap-1">
                        {template.models.map((model) => (
                          <Badge key={model} variant="secondary" className="text-xs">
                            {model}
                          </Badge>
                        ))}
                      </div>
                    </div>
                    
                    <div className="flex gap-2 pt-2">
                      <Button size="sm" variant="outline" className="flex-1">
                        <Play className="h-3 w-3 mr-1" />
                        Use
                      </Button>
                      <Button size="sm" variant="ghost">
                        <MoreVertical className="h-3 w-3" />
                      </Button>
                    </div>
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}