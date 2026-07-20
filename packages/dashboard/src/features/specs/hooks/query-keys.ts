export const specKeys = {
  all: ['specs'] as const,
  lists: () => [...specKeys.all, 'list'] as const,
  list: (filters?: Record<string, unknown>) => [...specKeys.lists(), filters ?? {}] as const,
  details: () => [...specKeys.all, 'detail'] as const,
  detail: (id: string) => [...specKeys.details(), id] as const,
  agentStatus: (id: string) => [...specKeys.detail(id), 'agent-status'] as const,
};
