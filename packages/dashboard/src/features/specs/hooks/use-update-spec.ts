'use client'

import { useMutation, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { specsApi } from '@/lib/api-client'
import { specKeys } from './query-keys'
import type { UpdateSpecInput } from '../types'

export function useUpdateSpec(specId: string) {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (input: UpdateSpecInput) => specsApi.update(specId, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: specKeys.detail(specId) })
      queryClient.invalidateQueries({ queryKey: specKeys.lists() })
    },
    onError: (error: Error) => {
      toast.error(`Failed to update spec: ${error.message}`)
    },
  })
}
