import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import {
  Eye,
  Radio,
  Shield,
  Store,
  Activity,
  Zap,
  Waves,
  Lock,
  Users,
  Download,
  Star,
  TrendingUp,
  AlertTriangle,
  CheckCircle,
  Clock
} from "lucide-react";

export function ResonanceView() {
  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold">Resonance View</h1>
        <p className="text-muted-foreground">
          Visualize model interaction patterns and system adaptation over time
        </p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <Card className="bg-gradient-surface border-border/50 shadow-card">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Eye className="h-5 w-5 text-primary" />
              Dynamic Resonance Graph
            </CardTitle>
            <CardDescription>
              Real-time visualization of AI response evolution
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="h-64 bg-muted/20 rounded-lg flex items-center justify-center relative overflow-hidden">
              {/* Simulated resonance visualization */}
              <div className="absolute inset-4">
                <svg className="w-full h-full" viewBox="0 0 300 200">
                  <defs>
                    <linearGradient id="resonanceGrad" x1="0%" y1="0%" x2="100%" y2="100%">
                      <stop offset="0%" style={{stopColor: 'hsl(var(--primary))', stopOpacity: 0.8}} />
                      <stop offset="50%" style={{stopColor: 'hsl(var(--accent))', stopOpacity: 0.6}} />
                      <stop offset="100%" style={{stopColor: 'hsl(var(--secondary))', stopOpacity: 0.4}} />
                    </linearGradient>
                  </defs>
                  
                  {/* Resonance circles */}
                  <circle cx="150" cy="100" r="30" fill="none" stroke="url(#resonanceGrad)" strokeWidth="2" className="animate-resonance" />
                  <circle cx="150" cy="100" r="50" fill="none" stroke="url(#resonanceGrad)" strokeWidth="1" className="animate-resonance" style={{animationDelay: '0.5s'}} />
                  <circle cx="150" cy="100" r="70" fill="none" stroke="url(#resonanceGrad)" strokeWidth="1" className="animate-resonance" style={{animationDelay: '1s'}} />
                  
                  {/* Connection lines */}
                  <line x1="50" y1="60" x2="150" y2="100" stroke="hsl(var(--primary))" strokeWidth="2" className="animate-pulse" />
                  <line x1="250" y1="60" x2="150" y2="100" stroke="hsl(var(--secondary))" strokeWidth="2" className="animate-pulse" />
                  <line x1="150" y1="100" x2="150" y2="180" stroke="hsl(var(--accent))" strokeWidth="2" className="animate-pulse" />
                  
                  {/* Model nodes */}
                  <circle cx="50" cy="60" r="8" fill="hsl(var(--primary))" className="animate-frequency" />
                  <circle cx="250" cy="60" r="8" fill="hsl(var(--secondary))" className="animate-frequency" />
                  <circle cx="150" cy="180" r="8" fill="hsl(var(--accent))" className="animate-frequency" />
                </svg>
              </div>
              
              <div className="absolute bottom-2 right-2 text-xs text-muted-foreground">
                Live DRR Mode
              </div>
            </div>
            
            <div className="mt-4 space-y-2">
              <div className="flex justify-between items-center">
                <span className="text-sm">Coherence Score</span>
                <span className="text-sm font-medium text-primary">94.2%</span>
              </div>
              <Progress value={94} className="h-2" />
            </div>
          </CardContent>
        </Card>

        <Card className="bg-gradient-surface border-border/50 shadow-card">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <TrendingUp className="h-5 w-5 text-secondary" />
              Adaptation Patterns
            </CardTitle>
            <CardDescription>
              System learning and optimization metrics
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="space-y-4">
              {[
                { model: "GPT-4", adaptation: 87, trend: "improving" },
                { model: "Claude Sonnet", adaptation: 92, trend: "stable" },
                { model: "Midjourney", adaptation: 78, trend: "learning" },
              ].map((item) => (
                <div key={item.model} className="space-y-2">
                  <div className="flex justify-between items-center">
                    <span className="text-sm font-medium">{item.model}</span>
                    <Badge variant={item.trend === "improving" ? "default" : item.trend === "stable" ? "secondary" : "outline"}>
                      {item.trend}
                    </Badge>
                  </div>
                  <Progress value={item.adaptation} className="h-2" />
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

export function AmbientMonitor() {
  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold">Ambient Data Monitor</h1>
        <p className="text-muted-foreground">
          Real-time sensor streams and environmental signal processing
        </p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        {[
          { name: "Voice Patterns", value: "432 Hz", status: "active", icon: Waves },
          { name: "Biometric Sync", value: "78 BPM", status: "connected", icon: Activity },
          { name: "Environmental", value: "22°C", status: "monitoring", icon: Radio }
        ].map((sensor) => {
          const Icon = sensor.icon;
          return (
            <Card key={sensor.name} className="bg-gradient-surface border-border/50 shadow-card">
              <CardContent className="p-4">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-3">
                    <Icon className="h-5 w-5 text-primary animate-frequency" />
                    <div>
                      <p className="font-medium">{sensor.name}</p>
                      <p className="text-sm text-muted-foreground">{sensor.value}</p>
                    </div>
                  </div>
                  <Badge className="bg-success/20 text-success border-success/30">
                    {sensor.status}
                  </Badge>
                </div>
              </CardContent>
            </Card>
          );
        })}
      </div>

      <Card className="bg-gradient-surface border-border/50 shadow-card">
        <CardHeader>
          <CardTitle>Ambient Agent Mesh</CardTitle>
          <CardDescription>
            Contextual awareness and response orchestration
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
            <div className="space-y-4">
              <h3 className="font-semibold">Active Sensors</h3>
              {[
                "Microphone Array (4 channels)",
                "Heart Rate Variability",
                "Ambient Temperature",
                "Light Level Detection",
                "Motion Sensors"
              ].map((sensor, index) => (
                <div key={index} className="flex items-center gap-2">
                  <CheckCircle className="h-4 w-4 text-success" />
                  <span className="text-sm">{sensor}</span>
                </div>
              ))}
            </div>
            
            <div className="space-y-4">
              <h3 className="font-semibold">Response Triggers</h3>
              {[
                "Stress level threshold exceeded",
                "Voice tone shift detected",
                "Temperature change > 2°C",
                "Ambient noise level spike",
                "Biometric anomaly detected"
              ].map((trigger, index) => (
                <div key={index} className="flex items-center gap-2">
                  <Zap className="h-4 w-4 text-accent" />
                  <span className="text-sm">{trigger}</span>
                </div>
              ))}
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

export function TrustSecurity() {
  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold">Trust & Security Panel</h1>
        <p className="text-muted-foreground">
          Model lineage, audit trails, and encrypted verification systems
        </p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        {[
          { label: "Trust Score", value: "98.7%", icon: Shield, status: "excellent" },
          { label: "Verified Models", value: "8/8", icon: CheckCircle, status: "complete" },
          { label: "Audit Events", value: "1,247", icon: Clock, status: "current" },
          { label: "Security Level", value: "HIPAA", icon: Lock, status: "compliant" }
        ].map((metric) => {
          const Icon = metric.icon;
          return (
            <Card key={metric.label} className="bg-gradient-surface border-border/50 shadow-card">
              <CardContent className="p-4">
                <div className="flex items-center gap-3">
                  <Icon className="h-5 w-5 text-primary" />
                  <div>
                    <p className="text-sm text-muted-foreground">{metric.label}</p>
                    <p className="text-lg font-bold">{metric.value}</p>
                  </div>
                </div>
              </CardContent>
            </Card>
          );
        })}
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <Card className="bg-gradient-surface border-border/50 shadow-card">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Shield className="h-5 w-5 text-primary" />
              Model Lineage Tracking
            </CardTitle>
            <CardDescription>
              Provenance and verification for all AI models
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="space-y-4">
              {[
                { model: "OpenAI GPT-4", verified: true, lastCheck: "2 min ago" },
                { model: "Anthropic Claude", verified: true, lastCheck: "5 min ago" },
                { model: "Midjourney v6", verified: true, lastCheck: "8 min ago" },
                { model: "ElevenLabs Voice", verified: true, lastCheck: "12 min ago" }
              ].map((item) => (
                <div key={item.model} className="flex items-center justify-between p-3 rounded-lg bg-muted/30">
                  <div className="flex items-center gap-3">
                    <CheckCircle className="h-4 w-4 text-success" />
                    <div>
                      <p className="font-medium">{item.model}</p>
                      <p className="text-xs text-muted-foreground">Last verified: {item.lastCheck}</p>
                    </div>
                  </div>
                  <Badge className="bg-success/20 text-success border-success/30">
                    Verified
                  </Badge>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>

        <Card className="bg-gradient-surface border-border/50 shadow-card">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Lock className="h-5 w-5 text-secondary" />
              Security Audit Log
            </CardTitle>
            <CardDescription>
              Recent security events and compliance checks
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="space-y-3">
              {[
                { event: "Key rotation completed", time: "3 min ago", level: "info" },
                { event: "Model verification passed", time: "8 min ago", level: "success" },
                { event: "Encryption protocol updated", time: "15 min ago", level: "info" },
                { event: "Access pattern analyzed", time: "22 min ago", level: "info" },
                { event: "Compliance check passed", time: "1 hour ago", level: "success" }
              ].map((item, index) => (
                <div key={index} className="flex items-start gap-3">
                  <div className={`w-2 h-2 rounded-full mt-2 ${
                    item.level === 'success' ? 'bg-success' : 'bg-primary'
                  } animate-pulse-glow`} />
                  <div>
                    <p className="text-sm font-medium">{item.event}</p>
                    <p className="text-xs text-muted-foreground">{item.time}</p>
                  </div>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

export function Marketplace() {
  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold">Marketplace</h1>
        <p className="text-muted-foreground">
          Community-contributed flows and signal-to-model templates
        </p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {[
          {
            name: "Voice Wellness Monitor",
            author: "Dr. Sarah Chen",
            downloads: 1247,
            rating: 4.8,
            price: "Free",
            description: "Monitor stress levels through voice patterns and trigger wellness interventions"
          },
          {
            name: "Creative Biometric Suite",
            author: "ArtTech Labs",
            downloads: 892,
            rating: 4.6,
            price: "$29",
            description: "Generate art based on heart rate variability and emotional states"
          },
          {
            name: "Security Frequency Scanner",
            author: "CyberResonance",
            downloads: 2156,
            rating: 4.9,
            price: "$49",
            description: "Detect security threats through ambient frequency analysis"
          },
          {
            name: "Ambient Learning Assistant",
            author: "EduFlow Inc",
            downloads: 756,
            rating: 4.5,
            price: "Free",
            description: "Adapt learning content based on environmental and biometric signals"
          },
          {
            name: "IoT Orchestration Hub",
            author: "SmartHome Pro",
            downloads: 1891,
            rating: 4.7,
            price: "$39",
            description: "Connect IoT devices to AI models for intelligent automation"
          },
          {
            name: "Medical Signal Processor",
            author: "HealthTech Solutions",
            downloads: 445,
            rating: 4.9,
            price: "$99",
            description: "HIPAA-compliant medical sensor integration with AI analysis"
          }
        ].map((template, index) => (
          <Card key={index} className="bg-gradient-surface border-border/50 shadow-card">
            <CardHeader>
              <CardTitle className="text-base">{template.name}</CardTitle>
              <CardDescription className="text-sm">
                by {template.author}
              </CardDescription>
            </CardHeader>
            <CardContent>
              <p className="text-sm text-muted-foreground mb-4">
                {template.description}
              </p>
              
              <div className="flex items-center justify-between mb-4">
                <div className="flex items-center gap-1">
                  <Star className="h-4 w-4 text-yellow-500 fill-current" />
                  <span className="text-sm font-medium">{template.rating}</span>
                </div>
                <div className="flex items-center gap-1">
                  <Download className="h-4 w-4 text-muted-foreground" />
                  <span className="text-sm text-muted-foreground">{template.downloads}</span>
                </div>
              </div>
              
              <div className="flex items-center justify-between">
                <Badge variant={template.price === "Free" ? "secondary" : "default"}>
                  {template.price}
                </Badge>
                <Button size="sm" className="bg-gradient-primary hover:opacity-90">
                  Install
                </Button>
              </div>
            </CardContent>
          </Card>
        ))}
      </div>
    </div>
  );
}