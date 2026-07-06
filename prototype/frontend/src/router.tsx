import {
  Outlet,
  createRootRoute,
  createRoute,
  createRouter,
} from "@tanstack/react-router";
import { SidebarInset, SidebarProvider } from "@/components/ui/sidebar";
import { AppSidebar } from "@/components/layout/app-sidebar";
import { Header } from "@/components/layout/header";
import { Dashboard } from "@/routes/dashboard";
import { Graph } from "@/routes/graph";
import { Belief } from "@/routes/belief";
import { Memory } from "@/routes/memory";
import { Chat } from "@/routes/chat";

const rootRoute = createRootRoute({
  component: () => (
    <SidebarProvider>
      <AppSidebar />
      <SidebarInset>
        <Header />
        <main className="flex-1 overflow-auto p-4">
          <Outlet />
        </main>
      </SidebarInset>
    </SidebarProvider>
  ),
});

const dashboardRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: Dashboard,
});
const graphRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/graph",
  component: Graph,
});
const chatRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/chat",
  component: Chat,
});
const memoryRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/memory",
  component: Memory,
});
const beliefRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/belief",
  component: Belief,
});

const routeTree = rootRoute.addChildren([
  dashboardRoute,
  graphRoute,
  chatRoute,
  memoryRoute,
  beliefRoute,
]);

export const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
