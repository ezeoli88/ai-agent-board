'use client'

import { useMutation, useQueryClient } from '@tanstack/react-query'
import { specsApi } from '@/lib/api-client'
import { specKeys } from './query-keys'

export function useRefineSpec() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => specsApi.refine(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: specKeys.all }),
  })
}

export function useSpecFeedback() {
  return useMutation({
    mutationFn: ({ id, message }: { id: string; message: string }) =>
      specsApi.sendFeedback(id, message),
  })
}

export function useCancelSpec() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => specsApi.cancel(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: specKeys.all }),
  })
}

export function useApproveSpec() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => specsApi.approve(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: specKeys.all }),
  })
}
