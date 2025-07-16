import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
} from "@/components/ui/command"
import {
    Activity,
    Workflow,
    Eye,
    Radio,
    Shield,
    Store,
    Plus
} from "lucide-react";

interface OmniBarProps {
    onNavigate: (section: string) => void;
}

export function OmniBar({ onNavigate }: OmniBarProps) {
  return (
    <div className="w-full max-w-2xl mx-auto my-8">
        <Command className="rounded-full border shadow-lg shadow-primary/10">
            <CommandInput placeholder="Type a command or search..." />
            <CommandList>
                <CommandEmpty>No results found.</CommandEmpty>
                <CommandGroup heading="Suggestions">
                <CommandItem onSelect={() => onNavigate('dashboard')}>
                    <Activity className="mr-2 h-4 w-4" />
                    <span>Dashboard</span>
                </CommandItem>
                <CommandItem onSelect={() => onNavigate('workflows')}>
                    <Workflow className="mr-2 h-4 w-4" />
                    <span>Workflow Builder</span>
                </CommandItem>
                <CommandItem onSelect={() => onNavigate('resonance')}>
                    <Eye className="mr-2 h-4 w-4" />
                    <span>Resonance View</span>
                </CommandItem>
                </CommandGroup>
                <CommandSeparator />
                <CommandGroup heading="Actions">
                <CommandItem onSelect={() => onNavigate('workflows')}>
                    <Plus className="mr-2 h-4 w-4" />
                    <span>Create New Workflow</span>
                </CommandItem>
                </CommandGroup>
            </CommandList>
        </Command>
    </div>
  )
}
