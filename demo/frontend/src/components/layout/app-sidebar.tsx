import type { LucideIcon } from "lucide-react";
import {
  BrainCircuit,
  Database,
  FileInput,
  FolderTree,
  LayoutDashboard,
  MessageSquare,
  Network,
  Scale,
  Waypoints,
} from "lucide-react";
import { Link } from "@tanstack/react-router";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar";

export type NavItem = {
  title: string;
  to: string;
  icon: LucideIcon;
};

export const NAV_ITEMS: NavItem[] = [
  { title: "Dashboard", to: "/", icon: LayoutDashboard },
  { title: "Graph", to: "/graph", icon: Waypoints },
  { title: "Explorer", to: "/explorer", icon: Network },
  { title: "Ingest", to: "/ingest", icon: FileInput },
  { title: "Knowledge", to: "/knowledge", icon: FolderTree },
  { title: "Belief", to: "/belief", icon: Scale },
  { title: "Memory", to: "/memory", icon: Database },
  { title: "Chat", to: "/chat", icon: MessageSquare },
];

export function AppSidebar() {
  return (
    <Sidebar>
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton size="lg" asChild>
              <Link to="/">
                <div className="flex aspect-square size-8 items-center justify-center rounded-lg bg-sidebar-primary text-sidebar-primary-foreground">
                  <BrainCircuit className="size-4" />
                </div>
                <div className="grid flex-1 text-left text-sm leading-tight">
                  <span className="truncate font-semibold">Engram</span>
                  <span className="truncate text-xs text-muted-foreground">
                    knowledge platform
                  </span>
                </div>
              </Link>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupLabel>Workspace</SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu>
              {NAV_ITEMS.map((item) => (
                <SidebarMenuItem key={item.to}>
                  <SidebarMenuButton asChild>
                    <Link to={item.to}>
                      <item.icon />
                      <span>{item.title}</span>
                    </Link>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              ))}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>
      <SidebarFooter />
    </Sidebar>
  );
}
