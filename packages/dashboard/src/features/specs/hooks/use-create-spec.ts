'use client'

import { useMutation, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { specsApi } from '@/lib/api-client'
import { specKeys } from './query-keys'
import type { CreateSpecInput } from '../types'

export function useCreateSpec() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (input: CreateSpecInput) => specsApi.create(input),
    onSuccess: () => {
      toast.success('Spec created')
      queryClient.invalidateQueries({ queryKey: specKeys.lists() })
    },
    onError: (error: Error) => {
      toast.error(`Failed to create spec: ${error.message}`)
    },
  })
}
