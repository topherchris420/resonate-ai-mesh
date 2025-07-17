import {
  Activity,
  Workflow,
  Eye,
  Radio,
  Shield,
  Store,
  Menu,
  LogOut,
  User,
  Settings,
} from "lucide-react";
import {
  NavigationMenu,
  NavigationMenuItem,
  NavigationMenuLink,
  NavigationMenuList,
  navigationMenuTriggerStyle,
} from "@/components/ui/navigation-menu";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Sheet, SheetContent, SheetTrigger } from "@/components/ui/sheet";
import { Button } from "@/components/ui/button";
import { Logo } from "@/components/ui/Logo";
import { useAuth } from "@/hooks/useAuth";
import { cn } from "@/lib/utils";

const navigationItems = [
    { id: "dashboard", label: "Dashboard", icon: Activity },
    { id: "workflows", label: "Workflows", icon: Workflow },
    { id: "resonance", label: "Resonance", icon: Eye },
    { id: "ambient", label: "Ambient", icon: Radio },
    { id: "security", label: "Security", icon: Shield },
    { id: "marketplace", label: "Marketplace", icon: Store },
];

interface HeaderProps {
    onNavigate: (section: string) => void;
    activeSection: string;
}

export function Header({ onNavigate, activeSection }: HeaderProps) {
    const { user, signOut } = useAuth();

    return (
    <header className="w-full bg-background border-b border-border/20 py-4">
      <div className="container mx-auto px-6 flex items-center justify-between">
        {/* Left - Logo */}
        <button onClick={() => onNavigate("dashboard")} className="flex items-center space-x-3">
          <Logo className="h-8 w-8" />
        </button>

        {/* Center - Brand Name */}
        <div className="absolute left-1/2 transform -translate-x-1/2">
          <button 
            onClick={() => onNavigate("dashboard")} 
            className="text-2xl font-light text-foreground hover:text-primary transition-colors"
          >
            resonate-ai.com
          </button>
        </div>

        {/* Right - Mobile Menu + User */}
        <div className="flex items-center space-x-4">
          <Sheet>
            <SheetTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="md:hidden"
              >
                <Menu className="h-5 w-5" />
              </Button>
            </SheetTrigger>
            <SheetContent side="left" className="w-80">
              <div className="flex flex-col h-full">
                <div className="border-b p-4">
                  <button onClick={() => onNavigate("dashboard")} className="flex items-center space-x-2">
                    <Logo className="h-6 w-6" />
                    <span className="font-bold">Resonate AI</span>
                  </button>
                </div>
                <nav className="flex-1 space-y-2 p-4">
                  {navigationItems.map((item) => (
                    <Button
                      key={item.id}
                      variant={activeSection === item.id ? "secondary" : "ghost"}
                      onClick={() => onNavigate(item.id)}
                      className="w-full justify-start"
                    >
                      <item.icon className="mr-2 h-4 w-4" />
                      {item.label}
                    </Button>
                  ))}
                </nav>
              </div>
            </SheetContent>
          </Sheet>
          
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                className="relative h-10 w-10 rounded-full bg-muted"
              >
                <User className="h-5 w-5" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent className="w-56" align="end">
              <DropdownMenuLabel className="font-normal">
                <div className="flex flex-col space-y-1">
                  <p className="text-sm font-medium">My Account</p>
                  <p className="text-xs text-muted-foreground truncate">
                    {user?.email}
                  </p>
                </div>
              </DropdownMenuLabel>
              <DropdownMenuSeparator />
              <DropdownMenuItem>
                <Settings className="mr-2 h-4 w-4" />
                Settings
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem onClick={() => signOut()}>
                <LogOut className="mr-2 h-4 w-4" />
                Log out
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>
    </header>
  );
}
