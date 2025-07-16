import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import {
  Activity,
  Zap,
  Brain,
  Waves,
  TrendingUp,
  Shield,
  Clock,
  Settings,
  Plus,
  BarChart3,
  Radio,
  Cpu
} from "lucide-react";

export function Dashboard() {
  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex flex-col lg:flex-row lg:items-center justify-between gap-4">
        <div>
          <h1 className="text-3xl font-bold">Unified Dashboard</h1>
          <p className="text-muted-foreground">
            Monitor your AI routing signals and model interactions in real-time
          </p>
        </div>
        <div className="flex gap-2">
          <Button size="sm" className="bg-gradient-primary hover:opacity-90">
            <Plus className="h-4 w-4 mr-2" />
            New Flow
          </Button>
          <Button variant="outline" size="sm">
            <Settings className="h-4 w-4 mr-2" />
            Configure
          </Button>
        </div>
      </div>

      {/* Status Cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        <Card className="bg-gradient-surface border-border/50 shadow-card">
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Active Flows</CardTitle>
            <Activity className="h-4 w-4 text-primary animate-frequency" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">23</div>
            <p className="text-xs text-muted-foreground">
              +12% from last hour
            </p>
          </CardContent>
        </Card>

        <Card className="bg-gradient-surface border-border/50 shadow-card">
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Signal Events</CardTitle>
            <Waves className="h-4 w-4 text-secondary animate-wave" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">1,247</div>
            <p className="text-xs text-muted-foreground">
              Real-time processing
            </p>
          </CardContent>
        </Card>

        <Card className="bg-gradient-surface border-border/50 shadow-card">
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Models Connected</CardTitle>
            <Brain className="h-4 w-4 text-accent animate-resonance" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">8</div>
            <p className="text-xs text-muted-foreground">
              OpenAI, Claude, Midjourney
            </p>
          </CardContent>
        </Card>

        <Card className="bg-gradient-surface border-border/50 shadow-card">
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Trust Score</CardTitle>
            <Shield className="h-4 w-4 text-success" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">98.7%</div>
            <p className="text-xs text-muted-foreground">
              Verified lineage
            </p>
          </CardContent>
        </Card>
      </div>

      {/* Main Content Grid */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        
        {/* Real-time Activity */}
        <Card className="lg:col-span-2 bg-gradient-surface border-border/50 shadow-card">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <BarChart3 className="h-5 w-5 text-primary" />
              Real-time Signal Flow
            </CardTitle>
            <CardDescription>
              Live visualization of cymatic patterns and AI routing
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="space-y-4">
              {/* Simulated waveform */}
              <div className="h-32 bg-muted/30 rounded-lg flex items-end justify-center gap-1 p-4">
                {Array.from({ length: 40 }, (_, i) => (
                  <div
                    key={i}
                    className="bg-gradient-primary rounded-sm animate-wave"
                    style={{
                      width: '4px',
                      height: `${Math.random() * 80 + 20}%`,
                      animationDelay: `${i * 0.1}s`
                    }}
                  />
                ))}
              </div>
              
              {/* Activity indicators */}
              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <span className="text-sm font-medium">OpenAI GPT-4</span>
                  <Badge className="bg-primary/20 text-primary border-primary/30">
                    Active
                  </Badge>
                </div>
                <Progress value={85} className="h-2" />
                
                <div className="flex items-center justify-between">
                  <span className="text-sm font-medium">Claude Sonnet</span>
                  <Badge className="bg-secondary/20 text-secondary border-secondary/30">
                    Processing
                  </Badge>
                </div>
                <Progress value={62} className="h-2" />
                
                <div className="flex items-center justify-between">
                  <span className="text-sm font-medium">Midjourney</span>
                  <Badge variant="outline">
                    Idle
                  </Badge>
                </div>
                <Progress value={23} className="h-2" />
              </div>
            </div>
          </CardContent>
        </Card>

        {/* Cymatic Enhancer */}
        <Card className="bg-gradient-surface border-border/50 shadow-card">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Radio className="h-5 w-5 text-secondary" />
              Cymatic IO Layer
            </CardTitle>
            <CardDescription>
              Frequency-based input enhancement
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="text-center">
              <div className="w-20 h-20 mx-auto rounded-full bg-gradient-secondary/20 border-2 border-secondary/40 flex items-center justify-center animate-resonance">
                <Waves className="h-8 w-8 text-secondary" />
              </div>
              <p className="text-sm font-medium mt-2">Resonance Active</p>
              <p className="text-xs text-muted-foreground">432 Hz base frequency</p>
            </div>
            
            <div className="space-y-3">
              <div className="flex justify-between items-center">
                <span className="text-sm">Voice Pattern</span>
                <span className="text-sm text-primary font-medium">Optimized</span>
              </div>
              
              <div className="flex justify-between items-center">
                <span className="text-sm">Biometric Sync</span>
                <span className="text-sm text-success font-medium">Connected</span>
              </div>
              
              <div className="flex justify-between items-center">
                <span className="text-sm">Ambient Sensors</span>
                <span className="text-sm text-warning font-medium">Calibrating</span>
              </div>
            </div>
          </CardContent>
        </Card>

        {/* Recent Events */}
        <Card className="bg-gradient-surface border-border/50 shadow-card">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Clock className="h-5 w-5 text-accent" />
              Recent Events
            </CardTitle>
            <CardDescription>
              Latest system activities
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="space-y-3">
              {[
                { time: "2m ago", event: "New workflow activated", type: "success" },
                { time: "5m ago", event: "Voice pattern analyzed", type: "info" },
                { time: "8m ago", event: "Model switched to Claude", type: "warning" },
                { time: "12m ago", event: "Biometric sync established", type: "success" },
                { time: "18m ago", event: "Trust verification completed", type: "info" }
              ].map((item, index) => (
                <div key={index} className="flex items-start gap-3">
                  <div className={`w-2 h-2 rounded-full mt-2 ${
                    item.type === 'success' ? 'bg-success' :
                    item.type === 'warning' ? 'bg-warning' : 'bg-primary'
                  } animate-pulse-glow`} />
                  <div className="flex-1 min-w-0">
                    <p className="text-sm font-medium">{item.event}</p>
                    <p className="text-xs text-muted-foreground">{item.time}</p>
                  </div>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>

        {/* Performance Metrics */}
        <Card className="lg:col-span-2 bg-gradient-surface border-border/50 shadow-card">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Cpu className="h-5 w-5 text-primary" />
              System Performance
            </CardTitle>
            <CardDescription>
              Resource utilization and optimization metrics
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              <div className="text-center">
                <div className="text-2xl font-bold text-primary">127ms</div>
                <p className="text-xs text-muted-foreground">Avg Latency</p>
              </div>
              <div className="text-center">
                <div className="text-2xl font-bold text-secondary">99.8%</div>
                <p className="text-xs text-muted-foreground">Uptime</p>
              </div>
              <div className="text-center">
                <div className="text-2xl font-bold text-accent">2.4TB</div>
                <p className="text-xs text-muted-foreground">Data Processed</p>
              </div>
              <div className="text-center">
                <div className="text-2xl font-bold text-success">$47.23</div>
                <p className="text-xs text-muted-foreground">Cost Today</p>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}