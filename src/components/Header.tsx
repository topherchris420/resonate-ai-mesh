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
    <header className="sticky top-0 z-50 w-full border-b border-border/40 bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
      <div className="container flex h-14 max-w-screen-2xl items-center">
        <div className="mr-4 hidden md:flex">
          <button onClick={() => onNavigate("dashboard")} className="mr-6 flex items-center space-x-2">
            <Logo className="h-6 w-6" />
            <span className="hidden font-bold sm:inline-block bg-gradient-heading bg-clip-text text-transparent">
                Resonate
            </span>
          </button>
          <nav className="flex items-center gap-4 text-sm lg:gap-6">
            <NavigationMenu>
                <NavigationMenuList>
                    {navigationItems.map((item) => (
                    <NavigationMenuItem key={item.id}>
                        <NavigationMenuLink
                        asChild
                        active={activeSection === item.id}
                        >
                        <button
                            onClick={() => onNavigate(item.id)}
                            className={cn(navigationMenuTriggerStyle(), "bg-transparent")}
                        >
                            {item.label}
                        </button>
                        </NavigationMenuLink>
                    </NavigationMenuItem>
                    ))}
                </NavigationMenuList>
            </NavigationMenu>
          </nav>
        </div>

        {/* Mobile Nav */}
        <Sheet>
            <SheetTrigger asChild>
                <Button
                variant="ghost"
                size="icon"
                className="md:hidden"
                >
                <Menu className="size-5" />
                <span className="sr-only">Toggle Menu</span>
                </Button>
            </SheetTrigger>
            <SheetContent side="left">
            <div className="flex flex-col h-full">
              <div className="border-b p-4">
                 <button onClick={() => onNavigate("dashboard")} className="flex items-center space-x-2">
                    <Logo className="h-6 w-6" />
                    <span className="font-bold bg-gradient-heading bg-clip-text text-transparent">
                        Resonate
                    </span>
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
        
        <div className="flex flex-1 items-center justify-end space-x-2">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="outline"
                className="relative h-8 w-8 rounded-full"
              >
                <User className="h-4 w-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent className="w-56" align="end" forceMount>
              <DropdownMenuLabel className="font-normal">
                <div className="flex flex-col space-y-1">
                  <p className="text-sm font-medium leading-none">My Account</p>
                  <p className="text-xs leading-none text-muted-foreground truncate">
                    {user?.email}
                  </p>
                </div>
              </DropdownMenuLabel>
              <DropdownMenuSeparator />
              <DropdownMenuItem>
                <Settings className="mr-2 h-4 w-4" />
                <span>Settings</span>
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem onClick={() => signOut()}>
                <LogOut className="mr-2 h-4 w-4" />
                <span>Log out</span>
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>
    </header>
  );
}
