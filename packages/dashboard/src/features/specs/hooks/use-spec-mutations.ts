'use client'

import { useMutation, useQueryClient } from '@tanstack/react-query'
import { specsApi } from '@/lib/api-client'
import { specKeys } from './query-keys'
import type { CreateSpecInput, UpdateSpecInput } from '../types'

export function useCreateSpec() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: CreateSpecInput) => specsApi.create(input),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: specKeys.all }),
  })
}

export function useUpdateSpec() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, input }: { id: string; input: UpdateSpecInput }) =>
      specsApi.update(id, input),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: specKeys.all }),
  })
}

export function useDeleteSpec() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => specsApi.delete(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: specKeys.all }),
  })
}
