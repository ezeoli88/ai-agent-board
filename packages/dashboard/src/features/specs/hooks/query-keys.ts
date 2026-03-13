export const specKeys = {
  all: ['specs'] as const,
  lists: () => [...specKeys.all, 'list'] as const,
  list: (filters: { repository_id?: string }) => [...specKeys.lists(), filters] as const,
  details: () => [...specKeys.all, 'detail'] as const,
  detail: (id: string) => [...specKeys.details(), id] as const,
}
